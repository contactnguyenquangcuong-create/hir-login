//! Optional team profile sync against a self-hosted server (see
//! `sync-server/` at the repo root). Off unless Settings has a server_url +
//! token configured; every public function is then a no-op that returns Ok
//! immediately, so a profile launches exactly as before this module existed.
//!
//! Model: a profile is a lock plus a zip bundle of the same account-carrying
//! files `trash.rs` keeps (cookies, logins, storage — not cache). Starting a
//! profile locks it on the server and pulls the latest bundle down over the
//! local copy; closing it pushes the local copy up and releases the lock. The
//! server refuses a lock already held by another device, so two machines can
//! never run the same profile at once.

use crate::{cookies, settings, store, trash};
use anyhow::{Context, Result};
use settings::SyncConfig;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

// ---- the sync log ----
//
// What the sync did, in a file a person can read. A packaged app has no console, so a sync that
// quietly restored nothing (a login that did not come across, say) left no trace anywhere.

/// `sync.log` beside the settings. Kept short: past 400 KB the older half goes.
fn sync_log_path() -> Option<std::path::PathBuf> {
    store::user_files_root().ok().map(|r| r.join("sync.log"))
}

fn sync_log(line: &str) {
    eprintln!("[sync] {line}");
    let Some(path) = sync_log_path() else { return };
    static LOCK: Mutex<()> = Mutex::new(());
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if fs::metadata(&path).map(|m| m.len() > 400_000).unwrap_or(false) {
        if let Ok(text) = fs::read_to_string(&path) {
            let keep: String = text.chars().skip(text.chars().count() / 2).collect();
            let keep = keep.split_once('\n').map(|(_, rest)| rest.to_string()).unwrap_or(keep);
            let _ = fs::write(&path, keep);
        }
    }
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{} {line}", crate::localtime::now_local());
    }
}

/// One line into the sync log from outside this module (the launch timings).
pub(crate) fn log_line(line: &str) {
    sync_log(line);
}

macro_rules! slog {
    ($($arg:tt)*) => { sync_log(&format!($($arg)*)) };
}

/// The last `lines` lines of the sync log, newest last. Empty when nothing was logged yet.
pub fn log_tail(lines: usize) -> String {
    let Some(path) = sync_log_path() else { return String::new() };
    let text = fs::read_to_string(path).unwrap_or_default();
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

// ---- activity + state ----

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

fn busy_map() -> &'static Mutex<HashMap<String, &'static str>> {
    static M: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Bumped whenever a background pull changed a local profile, so the UI knows
/// to reload its list without being told which one.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Marks a profile as mid-sync for as long as it lives; the UI shows "syncing".
struct BusyGuard(String);
impl Drop for BusyGuard {
    fn drop(&mut self) {
        if let Ok(mut m) = busy_map().lock() {
            m.remove(&self.0);
        }
    }
}

fn try_begin(id: &str, phase: &'static str) -> Option<BusyGuard> {
    let mut m = busy_map().lock().ok()?;
    if m.contains_key(id) {
        return None;
    }
    m.insert(id.to_string(), phase);
    Some(BusyGuard(id.to_string()))
}

/// For the launch/close paths: waits (bounded) for a background pass that is
/// already working on this profile instead of failing the user's click.
async fn begin_wait(id: &str, phase: &'static str) -> BusyGuard {
    for _ in 0..120 {
        if let Some(g) = try_begin(id, phase) {
            return g;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    if let Ok(mut m) = busy_map().lock() {
        m.insert(id.to_string(), phase);
    }
    BusyGuard(id.to_string())
}

#[derive(serde::Serialize)]
pub struct SyncActivity {
    pub busy: Vec<String>,
    pub generation: u64,
}

pub fn activity() -> SyncActivity {
    let busy = busy_map()
        .lock()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();
    SyncActivity { busy, generation: GENERATION.load(Ordering::Relaxed) }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct StateItem {
    /// The server's `updatedAt` for this profile when we last matched it.
    remote: String,
    /// Unix seconds of that moment; a local edit newer than this is unsynced.
    at: u64,
    /// The same moment in milliseconds (0 in state written by an older build). Whole seconds
    /// cannot tell "changed just after the sync" from "unchanged since it".
    #[serde(default)]
    at_ms: u64,
    /// Signature of the bound proxy then; a different one now is an unsynced change.
    #[serde(default)]
    proxy: String,
    /// What the login was restored by when this was recorded (`SYNC_FMT`; 0 from an older build).
    /// Only a copy restored the current way may skip the download on the next open: one restored
    /// by an earlier build may have the login in the wrong place or sealed with the wrong key, and
    /// "nothing new on the server" would keep it that way.
    #[serde(default)]
    fmt: u32,
}

/// Bumped whenever the way a login is restored changes in a way old copies need to be redone.
const SYNC_FMT: u32 = 2;

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct SyncState {
    #[serde(default)]
    items: HashMap<String, StateItem>,
    /// Profiles whose last close did not reach the server. This machine holds
    /// the newest copy (a fresh login, for one) that the server does not have,
    /// so the next open must not pull the server's older copy over it.
    #[serde(default)]
    pending: std::collections::HashSet<String>,
    /// Proxies that arrived with a team profile (their logins travel inside the bundle). Removed
    /// with the profiles when this machine leaves the team or is removed from it.
    #[serde(default)]
    team_proxies: std::collections::HashSet<String>,
}

fn state_lock() -> &'static Mutex<()> {
    static M: OnceLock<Mutex<()>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(()))
}

fn state_path() -> Result<std::path::PathBuf> {
    Ok(store::user_files_root()?.join("sync-state.json"))
}

fn load_state() -> SyncState {
    state_path()
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|b| serde_json::from_str(&b).ok())
        .unwrap_or_default()
}

fn update_state(f: impl FnOnce(&mut SyncState)) {
    let _g = state_lock().lock().unwrap_or_else(|e| e.into_inner());
    let mut st = load_state();
    f(&mut st);
    if let (Ok(p), Ok(body)) = (state_path(), serde_json::to_string_pretty(&st)) {
        let tmp = p.with_extension("json.tmp");
        if fs::write(&tmp, body).is_ok() {
            let _ = fs::rename(&tmp, &p);
        }
    }
}

fn unix_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Records that this machine now matches what the server holds for `id`.
async fn mark_synced(id: &str) {
    let Ok(remote) = list_remote().await else { return };
    let Some(row) = remote.into_iter().find(|r| r.id == id) else { return };
    if let Some(updated) = row.updated_at {
        update_state(|st| {
            st.items.insert(id.to_string(), StateItem { remote: updated, at: unix_now(), at_ms: unix_now_ms(), proxy: proxy_signature(id), fmt: SYNC_FMT });
        });
    }
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        // A server that is switched off or unreachable must fail in seconds, not hold a
        // profile launch for the system's own TCP timeout (more than a minute).
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(180))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// None when sync isn't configured — every caller treats that as "do nothing".
fn active_config() -> Result<Option<(SyncConfig, String, String)>> {
    let cfg = settings::load()?.sync;
    if !cfg.enabled {
        return Ok(None);
    }
    let (Some(base), Some(token)) = (cfg.server_url.clone(), cfg.token.clone()) else {
        return Ok(None);
    };
    let base = base.trim_end_matches('/').to_string();
    if base.is_empty() || token.is_empty() {
        return Ok(None);
    }
    Ok(Some((cfg, base, token)))
}

/// What the team server said no to, as a message the UI can translate.
async fn denied(resp: reqwest::Response, what: &str) -> anyhow::Error {
    let code = resp.status().as_u16();
    // Whatever the request was, a 401 means this machine's token is no longer valid. Opening a
    // profile (the lock request) used to take that for an ordinary failure, so a removed member
    // only found out at the next background round.
    if code == 401 {
        kicked_out();
    }
    let body = resp.json::<serde_json::Value>().await.ok();
    let text = |k: &str| body.as_ref()
        .and_then(|v| v.get(k).and_then(|r| r.as_str().map(String::from)))
        .unwrap_or_default();
    let (reason, error) = (text("reason"), text("error"));
    match (code, reason.as_str()) {
        (404, _) => anyhow::anyhow!("permission denied: no access to this profile"),
        (403, _) if error.contains("hold the lock") => anyhow::anyhow!("you do not hold the lock for this profile"),
        (403, "edit") => anyhow::anyhow!("permission denied: you may use this profile but not change its settings"),
        (403, "add") => anyhow::anyhow!("permission denied: only an admin or manager can add"),
        (403, "move") => anyhow::anyhow!("permission denied: you may not move this profile to another folder"),
        (403, "delete") => anyhow::anyhow!("permission denied: you may not delete this profile"),
        (403, _) => anyhow::anyhow!("permission denied: not allowed"),
        _ => anyhow::anyhow!("sync server rejected the {what}: {code}"),
    }
}

/// The server said 401: this machine's token is no longer valid — disabled or
/// deleted from above, not a network blip or a permission limit on one action
/// (that's 403). Turns sync off locally at once and tells the running app, so
/// a revoked member is logged out within moments instead of only finding out
/// the next time they happen to notice a failed action. Safe to call
/// repeatedly (e.g. a retry loop hitting the same 401 every few seconds):
/// once `sync.enabled` is already false, this is a no-op.
fn kicked_out() {
    let Ok(mut s) = settings::load() else { return };
    if !s.sync.enabled {
        return;
    }
    s.sync.enabled = false;
    s.sync.server_url = None;
    s.sync.token = None;
    if settings::save(&s).is_err() {
        return;
    }
    if let Some(app) = crate::app_handle() {
        use tauri::Emitter;
        let _ = app.emit("team:kicked-out", ());
    }
    // Being removed from the team takes the team's profiles with it: switching sync off alone left
    // every profile that had been downloaded fully usable on the machine of someone who no longer
    // has any right to it.
    tauri::async_runtime::spawn(async {
        let n = wipe_team_data().await;
        slog!("this machine was removed from the team — {n} team profiles were deleted from it");
    });
}

/// Whether this machine may open `id`. A profile that came from a team is opened only while this
/// machine is in that team (or runs the team's server): once it has left, been removed, or had
/// sync switched off in the settings, it is not. Without this check, "no longer a member" changed
/// nothing about the profiles already on the disk.
///
/// This stops the app from opening them; it cannot stop someone who copies files out of a profile
/// folder while they are still a member. Machines that never joined a team are unaffected.
pub fn ensure_access(id: &str) -> Result<()> {
    let Ok(s) = settings::load() else { return Ok(()) };
    let hosts_a_team = s.server_host.token.as_deref().is_some_and(|t| !t.trim().is_empty());
    if s.sync.enabled || hosts_a_team {
        return Ok(());
    }
    let st = load_state();
    if st.items.contains_key(id) || st.pending.contains(id) {
        anyhow::bail!("profile này thuộc nhóm, mà máy này không còn kết nối với nhóm nên không mở được — tham gia lại nhóm bằng mã mới để dùng tiếp");
    }
    Ok(())
}

/// Deletes from this machine every profile that came from (or went to) the team, with its browser
/// data, its copy in the trash, and the proxies that arrived with them; stops any of them that is
/// running first; forgets the sync state. Local-only profiles are left alone. Returns how many
/// profiles were deleted. Nothing is reported to the server — it already has them.
pub async fn wipe_team_data() -> usize {
    let st = load_state();
    let mut ids: Vec<String> = st.items.keys().cloned().collect();
    ids.extend(st.pending.iter().cloned());
    ids.sort();
    ids.dedup();
    let mut gone = 0;
    for id in &ids {
        if crate::process::Tracker::shared().is_running(id) {
            let _ = crate::process::Tracker::shared().kill(id).await;
            crate::cdp::detach(id);
        }
        let existed = crate::profile::load_raw(id).is_ok();
        let _ = crate::profile::delete(id);
        let _ = trash::purge(id);
        if existed {
            gone += 1;
        }
    }
    for pid in &st.team_proxies {
        let _ = crate::proxy::delete(pid);
    }
    update_state(|s| {
        s.items.clear();
        s.pending.clear();
        s.team_proxies.clear();
    });
    if let Some(app) = crate::app_handle() {
        use tauri::Emitter;
        let _ = app.emit("team:wiped", gone);
    }
    crate::notify_store_changed("profiles");
    crate::notify_store_changed("proxies");
    gone
}

/// A call to the team server's member/permission API with this machine's token.
pub async fn admin_call(method: &str, path: &str, body: Option<serde_json::Value>) -> Result<serde_json::Value> {
    let Some((_cfg, base, token)) = active_config()? else { anyhow::bail!("sync is not enabled") };
    let c = client();
    let url = format!("{base}{path}");
    let mut req = match method { "PUT" => c.put(url), "POST" => c.post(url), _ => c.get(url) }.bearer_auth(&token);
    if let Some(b) = body { req = req.json(&b); }
    let resp = req.send().await.context("contact sync server")?;
    if resp.status().as_u16() == 401 { kicked_out(); anyhow::bail!("sync server rejected the request: 401"); }
    if resp.status().as_u16() == 403 {
        let reason = resp.json::<serde_json::Value>().await.ok()
            .and_then(|v| v.get("reason").and_then(|r| r.as_str().map(String::from))).unwrap_or_default();
        if reason == "folder-owner" { anyhow::bail!("permission denied: folder made by someone above"); }
        // An admin trying to rank a peer admin IS "the admin" — the generic
        // "only the admin can manage members" text below would tell them the
        // opposite of what's actually true and send them looking in the wrong
        // place. Only the server's own token may do this.
        if reason == "admin-rank" { anyhow::bail!("permission denied: only the server token can change another admin's rank"); }
        if reason == "self-rotate" { anyhow::bail!("cannot rotate your own token"); }
        anyhow::bail!("permission denied: only the admin can manage members");
    }
    if !resp.status().is_success() {
        let status = resp.status();
        // Validation errors (duplicate name, missing field, …) carry a specific
        // reason in the body — without it every one of them collapsed into the
        // same generic "server rejected the request, try again later", which
        // is actively wrong advice for something that will never succeed by
        // retrying (e.g. two members can't share a name).
        let msg = resp.json::<serde_json::Value>().await.ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str().map(String::from)))
            .filter(|m| !m.is_empty());
        match msg {
            Some(m) => anyhow::bail!("sync server rejected the request: {status} — {m}"),
            None => anyhow::bail!("sync server rejected the request: {status}"),
        }
    }
    Ok(resp.json().await.unwrap_or(serde_json::Value::Null))
}

fn device_name(cfg: &SyncConfig) -> String {
    if let Some(n) = cfg.device_name.as_deref().filter(|s| !s.trim().is_empty()) {
        return n.to_string();
    }
    let mut cmd = std::process::Command::new("hostname");
    // This runs on every checkout/checkin — i.e. every profile open/close, for
    // anyone on the team, not just an admin doing something occasional. Missing
    // this flag here specifically is what a member kept seeing as a console
    // flashing "thỉnh thoảng" (on ordinary use, not admin actions).
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    cmd.output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown-device".into())
}

/// What travels between machines besides `trash::KEEP` (the account data a
/// restored-from-trash profile needs): the open tabs and the history-adjacent
/// state, so a profile picks up where it was left. Caches stay out.
const SYNC_EXTRA: &[&str] = &[
    "Default/Sessions",
    "Default/Session Storage",
    "Default/Top Sites",
    "Default/Shortcuts",
    "Default/Network Action Predictor",
    "Default/Account Web Data",
    "Default/Extension Rules",
    "Default/Extension Scripts",
    "Default/WebStorage",
    "Default/Storage",
    "Default/DIPS",
    // Extensions installed inside the profile and the data they keep.
    "Default/Extensions",
    "Default/Managed Extension Settings",
    "Default/Sync Extension Settings",
    "Default/Local App Settings",
];

pub(crate) fn synced_paths() -> impl Iterator<Item = &'static str> {
    trash::KEEP.iter().copied().chain(SYNC_EXTRA.iter().copied())
}

/// Files that are sealed with (or hold) this machine's key: cookies, saved passwords and
/// Local State, where the key itself lives. Sent raw they are noise on another machine —
/// and `Local State` would replace the receiving machine's own key. They travel as
/// re-sealable copies under `portable/` instead (see `cookies::portable_copy`).
const SEALED: &[(&str, &str, cookies::Sealed)] = &[
    ("Default/Network/Cookies", "portable/Cookies", cookies::Sealed::Cookies),
    ("Default/Login Data", "portable/Login Data", cookies::Sealed::Logins),
    ("Default/Login Data For Account", "portable/Login Data For Account", cookies::Sealed::Logins),
];

/// Never sent raw, and never taken raw from a bundle (an older build sent them).
fn is_unportable(rel: &str) -> bool {
    rel == "Local State"
        || rel == "Default/Cookies"
        || SEALED.iter().any(|(path, _, _)| *path == rel)
}

/// What travels as plain files: everything in `synced_paths` that is not sealed.
fn raw_paths() -> impl Iterator<Item = &'static str> {
    synced_paths().filter(|p| !is_unportable(p))
}

/// The proxy a profile is bound to, if any. It travels inside the bundle so a
/// profile arriving on another machine finds its proxy there too, instead of
/// pointing at an id that machine has never heard of.
fn bound_proxy(id: &str) -> Option<crate::proxy::ProxyEntry> {
    let stored = crate::profile::load_raw(id).ok()?;
    let pid = stored.meta.proxy_id.as_deref()?;
    crate::proxy::get(pid).ok().flatten()
}

/// What "the proxy changed" means for sync: everything but the country tag,
/// which each machine refreshes on its own.
fn proxy_signature(id: &str) -> String {
    match bound_proxy(id) {
        Some(mut p) => {
            p.country.clear();
            serde_json::to_string(&p).unwrap_or_default()
        }
        None => String::new(),
    }
}

/// Zips the same files `trash.rs` archives — profile.json plus the
/// account-carrying subset of user-data, never the cache.
fn build_bundle(id: &str) -> Result<Vec<u8>> {
    let stored = crate::profile::load_raw(id)?;
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("profile.json", opts)?;
        zip.write_all(serde_json::to_string_pretty(&stored)?.as_bytes())?;
        // `null` when the profile connects directly, so unbinding syncs too.
        zip.start_file("proxy.json", opts)?;
        zip.write_all(serde_json::to_string(&bound_proxy(id))?.as_bytes())?;

        let udd = store::user_data_root()?.join(id);
        if udd.exists() {
            for rel in raw_paths() {
                let src = udd.join(rel);
                if src.is_dir() {
                    add_dir(&mut zip, &src, &format!("user-data/{rel}"), opts)?;
                } else if src.is_file() {
                    add_file(&mut zip, &src, &format!("user-data/{rel}"), opts)?;
                }
            }
            // The login: cookies and saved passwords, as copies another machine can re-seal.
            //
            // What goes up is this machine's login *united with* the one the server gave us at the
            // last pull, the more recent row of each winning. A machine that could not restore the
            // login (or never had it) used to send up a bundle without it, and the server's copy —
            // the one every new machine starts from — lost it for everybody. Now a machine can only
            // add to what the server holds, never take from it.
            for (rel, name, kind) in SEALED {
                let src = if *kind == cookies::Sealed::Cookies { cookies::cookie_db(&udd) } else { udd.join(rel) };
                let local = if src.is_file() {
                    match cookies::portable_copy(&udd, &src, *kind) {
                        Ok((bytes, st)) => {
                            let note = if st.dropped > 0 && st.portable == 0 {
                                " — NONE could be opened with this machine's key, so no login travels"
                            } else {
                                ""
                            };
                            slog!("{id}: {name}: {} rows, {} made portable, {} dropped{note}", st.rows, st.portable, st.dropped);
                            Some(bytes)
                        }
                        Err(e) => {
                            slog!("{id}: could not prepare {rel} for another machine: {e:#}");
                            None
                        }
                    }
                } else {
                    None
                };
                let held = cache_load(id, name);
                let outgoing = match (local, held) {
                    (Some(mine), Some(theirs)) => match union_portable(&mine, &theirs, *kind) {
                        Ok((united, added)) => {
                            if added > 0 {
                                slog!("{id}: {name}: {added} values the server already had were missing here and were kept in what is sent");
                            }
                            united
                        }
                        Err(e) => {
                            slog!("{id}: {name}: could not unite with the server's copy ({e:#}) — sending this machine's own");
                            mine
                        }
                    },
                    (Some(mine), None) => mine,
                    (None, Some(theirs)) => {
                        slog!("{id}: {name}: this machine has none — the server's copy is sent back unchanged");
                        theirs
                    }
                    (None, None) => continue,
                };
                cache_save(id, name, &outgoing);
                zip.start_file(*name, opts)?;
                zip.write_all(&outgoing)?;
            }
        }
        // The extensions this profile uses travel with it, so it never depends on
        // a shared library on the server: each one is a folder named by its id.
        if let Ok(ext_root) = store::extensions_dir() {
            for ext_id in &stored.meta.extensions {
                let dir = ext_root.join(ext_id);
                if dir.is_dir() {
                    add_dir(&mut zip, &dir, &format!("extensions/{ext_id}"), opts)?;
                }
            }
        }
        zip.finish()?;
    }
    Ok(buf.into_inner())
}

fn add_file<W: Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    src: &Path,
    name: &str,
    opts: zip::write::SimpleFileOptions,
) -> Result<()> {
    let Ok(bytes) = fs::read(src) else { return Ok(()) };
    zip.start_file(name, opts)?;
    zip.write_all(&bytes)?;
    Ok(())
}

fn add_dir<W: Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    src: &Path,
    prefix: &str,
    opts: zip::write::SimpleFileOptions,
) -> Result<()> {
    let Ok(rd) = fs::read_dir(src) else { return Ok(()) };
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let child = format!("{prefix}/{name}");
        match entry.file_type() {
            Ok(t) if t.is_dir() => add_dir(zip, &entry.path(), &child, opts)?,
            Ok(_) => add_file(zip, &entry.path(), &child, opts)?,
            Err(_) => {}
        }
    }
    Ok(())
}

/// Extracts a downloaded bundle over the profile's existing files. Unlike
/// `trash::restore`, this never touches the profile's id or deletes anything
/// local first — a partial remote bundle only overwrites what it contains.
fn apply_bundle(id: &str, bytes: &[u8]) -> Result<()> {
    apply_bundle_with(id, bytes, false)
}

/// `keep_local_session`: take the profile's configuration, proxy and extensions
/// from the bundle but leave this machine's own browser data (logins, cookies,
/// storage) untouched — for a profile whose last close never reached the server.
fn apply_bundle_with(id: &str, bytes: &[u8], keep_local_session: bool) -> Result<()> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    let udd = store::user_data_root()?.join(id);
    fs::create_dir_all(&udd)?;
    // Several of these are databases made of many files (Local Storage,
    // IndexedDB, Session Storage, Sessions). Writing the bundle's files over a
    // local copy mixes two generations of the same database — a stale manifest
    // pointing at the wrong table files — so every path the bundle carries is
    // cleared first and then written whole.
    let mut roots: std::collections::HashSet<&'static str> = std::collections::HashSet::new();
    for i in 0..zip.len() {
        let Ok(f) = zip.by_index(i) else { continue };
        let name = f.name().replace('\\', "/");
        let Some(sub) = name.strip_prefix("user-data/") else { continue };
        if let Some(root) = raw_paths().find(|r| sub == *r || sub.starts_with(&format!("{r}/"))) {
            roots.insert(root);
        }
    }
    if keep_local_session {
        roots.clear();
    }
    for root in roots {
        let p = udd.join(root);
        if p.is_dir() {
            let _ = fs::remove_dir_all(&p);
        } else if p.is_file() {
            let _ = fs::remove_file(&p);
        }
    }
    // Some(None) = the bundle says "no proxy"; None = an older bundle that says nothing.
    let mut proxy_in_bundle: Option<Option<crate::proxy::ProxyEntry>> = None;
    // extension id -> (relative path, bytes) of every file the bundle carries for it
    let mut bundled_ext: HashMap<String, Vec<(String, Vec<u8>)>> = HashMap::new();
    // The portable files this bundle carried (cookies, saved passwords), by name.
    let mut saw_portable: Vec<String> = Vec::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i)?;
        let Some(rel) = f.enclosed_name() else { continue };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if f.is_dir() {
            continue;
        }
        let mut buf = Vec::with_capacity(f.size() as usize);
        f.read_to_end(&mut buf)?;
        if let Some(rest) = rel_str.strip_prefix("extensions/") {
            if let Some((ext_id, sub)) = rest.split_once('/') {
                if !ext_id.is_empty() && !ext_id.starts_with('.') && !ext_id.contains("..") {
                    bundled_ext.entry(ext_id.to_string()).or_default().push((sub.to_string(), buf));
                }
            }
            continue;
        }
        if rel_str == "proxy.json" {
            if let Ok(p) = serde_json::from_slice::<Option<crate::proxy::ProxyEntry>>(&buf) {
                proxy_in_bundle = Some(p);
            }
            continue;
        }
        if rel_str == "profile.json" {
            // Merge just the fingerprint config + name/notes; keep this
            // machine's own _meta (folder, pin, extensions) as-is so a pull
            // never reshuffles local organisation.
            if let Ok(remote) = serde_json::from_slice::<crate::profile::StoredProfile>(&buf) {
                let existed = crate::profile::load_raw(id).is_ok();
                let mut local = crate::profile::load_raw(id).unwrap_or(remote.clone());
                local.config = remote.config;
                // How the team organises the profile follows it across machines.
                local.meta.extensions = remote.meta.extensions.clone();
                local.meta.color = remote.meta.color.clone();
                local.meta.android_media = remote.meta.android_media;
                let _ = crate::profile::save_raw(&mut local);
                if existed {
                    // `save_raw` keeps this machine's pin and folder on purpose
                    // (they have their own setters), so set them explicitly.
                    if crate::profile::load_raw(id).map(|l| l.meta.pinned).ok() != Some(remote.meta.pinned) {
                        let _ = crate::profile::set_pin(id, remote.meta.pinned);
                    }
                    if crate::profile::load_raw(id).map(|l| l.meta.folder).ok().as_deref() != Some(remote.meta.folder.as_str()) {
                        let _ = crate::profile::set_folder(id, &remote.meta.folder);
                    }
                }
            }
            continue;
        }
        if let Some((dest, _, kind)) = SEALED.iter().find(|(_, name, _)| *name == rel_str).map(|(d, n, k)| (*d, *n, *k)) {
            // A login travelling as a portable copy: seal it with this machine's key, then put it in place.
            saw_portable.push(rel_str.clone());
            cache_save(id, &rel_str, &buf);
            if !keep_local_session {
                match install_portable(&udd, dest, &buf, kind) {
                    Ok(n) => slog!("{id}: restored {dest}: {n} values sealed for this machine"),
                    Err(e) => slog!("{id}: could not restore {dest}: {e:#}"),
                }
            }
            continue;
        }
        let Some(sub) = rel_str.strip_prefix("user-data/") else { continue };
        if keep_local_session || is_unportable(sub) {
            // An older build sent its own sealed files raw; they cannot be opened here and
            // would replace this machine's key and logins with the other machine's.
            continue;
        }
        let out = udd.join(sub);
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(out, buf)?;
    }
    if keep_local_session {
        slog!("{id}: kept this machine's own login (its last close had not reached the server)");
    } else if !saw_portable.iter().any(|n| n == "portable/Cookies") {
        slog!("{id}: this bundle carries no portable cookies — it was made by an older version, or the profile had none — so the login is NOT restored here. Open and close the profile on the machine that is logged in, or use \"Đẩy lại đăng nhập\" there");
    }
    install_bundled_extensions(bundled_ext);
    // A bundle from an older build may carry random-id extensions; fold them into
    // the ones already here so nothing is installed twice.
    crate::extensions::canonicalize_all();
    // Bring the proxy across and point the profile at it (same id, so the
    // binding survives). Remote wins: the profile's proxy is part of what the
    // team shares.
    if let Some(proxy) = proxy_in_bundle {
        if let Ok(mut local) = crate::profile::load_raw(id) {
            match proxy {
                Some(mut p) => {
                    // The country tag is this machine's own reading of the exit IP.
                    if let Ok(Some(existing)) = crate::proxy::get(&p.id) {
                        p.country = existing.country;
                    }
                    if let Ok(saved) = crate::proxy::upsert(p) {
                        let pid = saved.id.clone();
                        update_state(|st| { st.team_proxies.insert(pid); });
                        local.meta.proxy_id = Some(saved.id);
                    }
                }
                None => local.meta.proxy_id = None,
            }
            let _ = crate::profile::save_raw(&mut local);
        }
    }
    Ok(())
}

/// Where the portable login the server last gave this machine is kept (see `build_bundle`).
fn cache_file(id: &str, name: &str) -> Option<std::path::PathBuf> {
    let leaf = name.rsplit('/').next().unwrap_or(name);
    store::user_files_root().ok().map(|r| r.join("sync-cache").join(id).join(format!("{leaf}.portable")))
}

fn cache_save(id: &str, name: &str, bytes: &[u8]) {
    if let Some(p) = cache_file(id, name) {
        if let Some(dir) = p.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let tmp = p.with_extension("portable.tmp");
        if fs::write(&tmp, bytes).is_ok() {
            let _ = crate::winfs::rename_replace(&tmp, &p);
        }
    }
}

fn cache_load(id: &str, name: &str) -> Option<Vec<u8>> {
    fs::read(cache_file(id, name)?).ok()
}

/// `mine` with every row of `theirs` that it lacks (or has an older version of) added: the union of
/// two portable logins. Returns the result and how many rows came from `theirs`.
fn union_portable(mine: &[u8], theirs: &[u8], kind: cookies::Sealed) -> Result<(Vec<u8>, usize)> {
    let dir = std::env::temp_dir().join(format!("hir-union-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir)?;
    let (a, b) = (dir.join("mine.db"), dir.join("theirs.db"));
    let result = (|| -> Result<(Vec<u8>, usize)> {
        fs::write(&a, mine)?;
        fs::write(&b, theirs)?;
        let added = cookies::merge_sealed(&a, &b, kind)?;
        Ok((fs::read(&a)?, added))
    })();
    let _ = fs::remove_dir_all(&dir);
    result
}

/// Seals a portable copy with this machine's key and puts it where the browser expects it.
fn install_portable(udd: &Path, dest_rel: &str, bytes: &[u8], kind: cookies::Sealed) -> Result<usize> {
    // Current engines read cookies from Network/Cookies and from nowhere else. A profile can
    // also carry the old Default/Cookies beside it (a Mac profile had 5 stale rows there and 76
    // live ones in Network/Cookies): writing into whichever file existed first put the restored
    // login where the browser never looks. So it always goes to the live place, and any old file
    // is left exactly as it is.
    let dest = if kind == cookies::Sealed::Cookies {
        udd.join("Default/Network/Cookies")
    } else {
        udd.join(dest_rel)
    };
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = dest.with_extension("hir-incoming");
    fs::write(&tmp, bytes)?;
    let done = cookies::localize_file(udd, &tmp, kind).and_then(|sealed| {
        let here = cookies::row_count(&dest, kind).unwrap_or(0);
        // Can the key this launcher assumes open what this machine's own browser wrote? If not,
        // nothing sealed with it will ever be read here, whatever else is right.
        if let Some((looked, opened)) = cookies::readable_rows(udd, &dest, kind) {
            if looked > 0 {
                let verdict = if opened == 0 {
                    " — NONE of them: the browser here seals with a different key than the one assumed, so a login written by this launcher is never read"
                } else {
                    ""
                };
                slog!("{dest_rel}: of this machine's own {looked} sealed values, {opened} can be opened with the key assumed here{verdict}");
            }
        }
        let swapped = if here > 0 {
            // This machine already has a login of its own. It is merged with the incoming one,
            // not replaced by it: whichever of the two is more recent stays for each cookie, and
            // nothing this machine has is lost. A copy pushed by a machine that had gone logged out
            // can therefore add what was missing and take nothing away.
            let merged = dest.with_extension("hir-merged");
            fs::copy(&dest, &merged)?;
            // Rows here that this machine's key cannot open are dropped first (see `purge_unreadable`).
            match cookies::purge_unreadable(udd, &merged, kind) {
                Ok(0) => {}
                Ok(n) => slog!("{dest_rel}: {n} of this machine's own values could not be opened with the key used here (sealed by another key) and were dropped before merging"),
                Err(e) => slog!("{dest_rel}: could not check this machine's own values: {e:#}"),
            }
            let result = cookies::merge_sealed(&merged, &tmp, kind);
            let _ = fs::remove_file(&tmp);
            match result {
                Ok(changed) => {
                    slog!("{dest_rel}: merged with this machine's own — {changed} of the incoming values were new or newer, the {here} already here were kept where they were as recent");
                    Ok(merged)
                }
                Err(e) => {
                    let _ = fs::remove_file(&merged);
                    Err(e)
                }
            }
        } else {
            Ok(tmp.clone())
        };
        let source = swapped?;
        for suffix in ["-journal", "-wal", "-shm"] {
            let mut o = dest.as_os_str().to_owned();
            o.push(suffix);
            let _ = fs::remove_file(std::path::PathBuf::from(o));
        }
        crate::winfs::rename_replace(&source, &dest).map(|_| sealed).map_err(Into::into)
    });
    if done.is_err() {
        let _ = fs::remove_file(&tmp);
        let _ = fs::remove_file(dest.with_extension("hir-merged"));
    }
    done
}

/// Puts the extensions that came in a bundle into this machine's extension
/// folder. An extension already here with the same files is left alone; a
/// different copy (an updated version) is replaced whole.
fn install_bundled_extensions(bundled: HashMap<String, Vec<(String, Vec<u8>)>>) {
    let Ok(root) = store::extensions_dir() else { return };
    for (id, files) in bundled {
        let dst = root.join(&id);
        let wanted: std::collections::BTreeMap<&str, usize> =
            files.iter().map(|(p, b)| (p.as_str(), b.len())).collect();
        let same = dst.is_dir() && {
            let mut have: std::collections::BTreeMap<String, usize> = Default::default();
            fn walk(base: &Path, dir: &Path, out: &mut std::collections::BTreeMap<String, usize>) {
                if let Ok(rd) = fs::read_dir(dir) {
                    for e in rd.flatten() {
                        let p = e.path();
                        if p.is_dir() {
                            walk(base, &p, out);
                        } else if let Ok(m) = e.metadata() {
                            let rel = p.strip_prefix(base).unwrap_or(&p).to_string_lossy().replace('\\', "/");
                            out.insert(rel, m.len() as usize);
                        }
                    }
                }
            }
            walk(&dst, &dst, &mut have);
            have.len() == wanted.len() && have.iter().all(|(k, v)| wanted.get(k.as_str()) == Some(v))
        };
        if same {
            continue;
        }
        // Never let an older copy from another machine replace a newer one here.
        if dst.is_dir() {
            let bundle_version = files
                .iter()
                .filter(|(p, _)| p == "manifest.json" || p.ends_with("/manifest.json"))
                .min_by_key(|(p, _)| p.matches('/').count())
                .and_then(|(_, b)| serde_json::from_slice::<serde_json::Value>(b).ok())
                .and_then(|m| m.get("version").and_then(|v| v.as_str().map(String::from)))
                .unwrap_or_default();
            if crate::extensions::version_lt(&bundle_version, &crate::extensions::version_of(&dst)) {
                continue;
            }
        }
        let tmp = root.join(format!(".incoming-{id}"));
        let _ = fs::remove_dir_all(&tmp);
        let wrote = files.iter().all(|(rel, bytes)| {
            let out = tmp.join(rel);
            out.parent().map_or(true, |p| fs::create_dir_all(p).is_ok()) && fs::write(out, bytes).is_ok()
        });
        if wrote {
            let _ = fs::remove_dir_all(&dst);
            if fs::rename(&tmp, &dst).is_err() {
                let _ = fs::remove_dir_all(&tmp);
            }
        } else {
            let _ = fs::remove_dir_all(&tmp);
        }
    }
}

/// Takes (or renews) this device's lock on a profile: pushes the expiry out
/// while we still hold it, and re-acquires it if it lapsed or the server
/// forgot it — but never takes it from another device (409).
async fn relock(c: &reqwest::Client, base: &str, token: &str, id: &str, holder: &str) -> Result<()> {
    let resp = c
        .post(format!("{base}/profiles/{id}/lock"))
        .bearer_auth(token)
        .json(&serde_json::json!({ "holder": holder }))
        .timeout(Duration::from_secs(25))
        .send()
        .await
        .context("renew lock")?;
    if resp.status().as_u16() == 401 { kicked_out(); }
    if resp.status().as_u16() == 409 {
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        let other = body.get("holder").and_then(|v| v.as_str()).unwrap_or("another device");
        anyhow::bail!("profile is now held by {other}; this session could not be saved to the server");
    }
    if !resp.status().is_success() {
        return Err(denied(resp, "lock request").await);
    }
    Ok(())
}

/// The lock a profile is opened under lasts 6 hours and nothing renewed it, so
/// a profile left open past that lost the right to save: the close-time upload
/// was refused, the failure only went to a log, and the next open replaced the
/// local copy (with the fresh login in it) by the older one on the server.
/// While the profile runs, renew it every half hour.
fn keep_lock_alive(profile_id: &str, holder: &str) {
    static RUNNING: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    let set = RUNNING.get_or_init(|| Mutex::new(std::collections::HashSet::new()));
    if !set.lock().map(|mut s| s.insert(profile_id.to_string())).unwrap_or(false) {
        return; // one heartbeat per profile is enough
    }
    let (id, holder) = (profile_id.to_string(), holder.to_string());
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30 * 60)).await;
            if !crate::process::Tracker::shared().is_running(&id) {
                break;
            }
            let Ok(Some((_cfg, base, token))) = active_config() else { break };
            if let Err(e) = relock(&client(), &base, &token, &id, &holder).await {
                slog!("renewing the lock on {id}: {e:#}");
            }
        }
        if let Ok(mut s) = set.lock() {
            s.remove(&id);
        }
    });
}

/// Tells the UI a close did not reach the server, so it is never silent.
fn report_checkin_failed(profile_id: &str, err: &anyhow::Error) {
    let name = crate::profile::load_raw(profile_id).ok()
        .and_then(|p| p.config.get("name").and_then(|v| v.as_str().map(String::from)))
        .unwrap_or_else(|| profile_id.to_string());
    if let Some(app) = crate::app_handle() {
        use tauri::Emitter;
        let _ = app.emit("sync:checkin-failed", serde_json::json!({ "id": profile_id, "name": name, "error": format!("{err:#}") }));
    }
}

/// Call before spawning the browser. Locks the profile on the server (fails
/// loudly if another device holds it) and pulls its latest bundle down. A
/// no-op when sync isn't configured.
pub async fn checkout(profile_id: &str) -> Result<()> {
    let Some((cfg, base, token)) = active_config()? else { return Ok(()) };
    let started = std::time::Instant::now();
    let _busy = begin_wait(profile_id, "pull").await;
    let holder = device_name(&cfg);
    let c = client();

    // The last close never reached the server, so this machine's browser data is
    // the newer copy. Pulling it would replace it — the login included — with the
    // older one still on the server, so the session stays; the next close saves
    // it. The configuration still comes from the server: whoever may change it
    // (an admin switching a phone profile to desktop, say) must reach this
    // machine even when it cannot push its own version back.
    let state = load_state();
    let keep_session = state.pending.contains(profile_id);
    if keep_session {
        slog!("{profile_id}: last close was not saved to the server — keeping this machine's session");
    }

    // The version this machine last synced, when nothing it would send has changed since:
    // then the server need not send the bundle again (it answers 304 if it still holds that
    // version). Reopening a profile on the machine that closed it last costs one round trip.
    let unchanged_since: Option<String> = state
        .items
        .get(profile_id)
        .filter(|k| {
            if keep_session || k.at_ms == 0 || k.fmt != SYNC_FMT {
                return false;
            }
            let newest = local_data_time_ms(profile_id);
            newest > 0 && newest <= k.at_ms
        })
        .map(|k| k.remote.clone());

    // The lock and the bundle are asked for together: they are independent until the lock is
    // refused, and asking one after the other put a whole round trip to a far-away server
    // between "open" and the browser appearing. A refused lock simply drops the bundle.
    let lock_req = c
        .post(format!("{base}/profiles/{profile_id}/lock"))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "holder": holder }))
        .timeout(Duration::from_secs(25))
        .send();
    let mut bundle_req = c.get(format!("{base}/profiles/{profile_id}/bundle")).bearer_auth(&token);
    if let Some(v) = &unchanged_since {
        bundle_req = bundle_req.header("If-None-Match", format!("\"{v}\""));
    }
    let (lock_res, bundle_res) = tokio::join!(lock_req, bundle_req.send());
    let resp = lock_res.context("contact sync server")?;

    if resp.status().as_u16() == 409 {
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        let other = body.get("holder").and_then(|v| v.as_str()).unwrap_or("another device");
        anyhow::bail!("this profile is in use by {other} — try again once they close it");
    }
    if !resp.status().is_success() {
        return Err(denied(resp, "lock request").await);
    }
    keep_lock_alive(profile_id, &holder);
    // A newer server says which version of the bundle this lock sits on.
    let lock_version: Option<String> = resp
        .json::<serde_json::Value>()
        .await
        .ok()
        .and_then(|v| v.get("updatedAt").and_then(|x| x.as_str().map(String::from)))
        .filter(|s| !s.is_empty());
    let lock_ms = started.elapsed().as_millis();

    let bundle = bundle_res.context("download profile bundle")?;
    let status = bundle.status();

    if status.as_u16() == 304 {
        slog!("{profile_id}: opened here in {} ms — already up to date with the server, nothing downloaded", started.elapsed().as_millis());
        return Ok(());
    }

    let mut download_ms = 0u128;
    let mut apply_ms = 0u128;
    let mut kb = 0usize;
    if status.is_success() {
        let etag = bundle
            .headers()
            .get("etag")
            .and_then(|h| h.to_str().ok())
            .map(|s| s.trim().trim_matches('"').to_string());
        let t = std::time::Instant::now();
        let mut bytes = bundle.bytes().await.context("read bundle body")?;
        // A push that landed between the two requests would leave the lock on one version and
        // the bundle on another. Both say which they are; if they differ, take the bundle again.
        if let (Some(lv), Some(ev)) = (&lock_version, &etag) {
            if lv != ev {
                slog!("{profile_id}: the bundle changed while it was being fetched ({ev} → {lv}) — fetching it again");
                let again = c
                    .get(format!("{base}/profiles/{profile_id}/bundle"))
                    .bearer_auth(&token)
                    .send()
                    .await
                    .context("download profile bundle")?;
                if again.status().is_success() {
                    bytes = again.bytes().await.context("read bundle body")?;
                }
            }
        }
        download_ms = t.elapsed().as_millis();
        kb = bytes.len() / 1024;
        let t = std::time::Instant::now();
        if let Err(e) = apply_bundle_with(profile_id, &bytes, keep_session) {
            // Best effort: don't strand the lock on a corrupt bundle.
            let _ = unlock(&base, &token, profile_id, &holder).await;
            return Err(e.context("apply downloaded bundle"));
        }
        apply_ms = t.elapsed().as_millis();
    } else if status.as_u16() != 404 {
        let _ = unlock(&base, &token, profile_id, &holder).await;
        return Err(denied(bundle, "download").await);
    }
    // 404 = no remote copy yet (first time this profile syncs) — fine, the
    // local copy becomes the first version on checkin.
    if !keep_session {
        match &lock_version {
            // The server told us the version: record it now, no further question asked.
            Some(v) => {
                let (id, v) = (profile_id.to_string(), v.clone());
                update_state(|st| {
                    st.items.insert(id.clone(), StateItem { remote: v, at: unix_now(), at_ms: unix_now_ms(), proxy: proxy_signature(&id), fmt: SYNC_FMT });
                });
            }
            // An older server does not: look it up, but not while the person waits for the browser.
            None => {
                let id = profile_id.to_string();
                tokio::spawn(async move { mark_synced(&id).await });
            }
        }
    }
    slog!(
        "{profile_id}: opened here in {} ms — lock {lock_ms} ms, download {kb} KB in {download_ms} ms, apply {apply_ms} ms",
        started.elapsed().as_millis()
    );

    Ok(())
}

/// The newest modification time, in milliseconds, of anything a bundle of this profile would
/// carry (the synced folders and files, and the profile's own settings file). Compared with the
/// moment of the last sync it answers "has this machine changed the profile since?".
fn local_data_time_ms(id: &str) -> u64 {
    fn ms(m: &fs::Metadata) -> u64 {
        m.modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
    fn walk(p: &Path, newest: &mut u64, seen: &mut usize) {
        let Ok(m) = fs::metadata(p) else { return };
        *newest = (*newest).max(ms(&m));
        if m.is_dir() && *seen < 20_000 {
            if let Ok(rd) = fs::read_dir(p) {
                for e in rd.flatten() {
                    *seen += 1;
                    walk(&e.path(), newest, seen);
                }
            }
        }
    }
    let Ok(root) = store::user_data_root() else { return 0 };
    let udd = root.join(id);
    let (mut newest, mut seen) = (0u64, 0usize);
    for rel in synced_paths() {
        walk(&udd.join(rel), &mut newest, &mut seen);
    }
    if newest == 0 {
        return 0; // nothing of it here at all: not "unchanged", simply absent
    }
    if let Ok(dir) = store::profiles_dir() {
        if let Ok(m) = fs::metadata(dir.join(format!("{id}.json"))) {
            newest = newest.max(ms(&m));
        }
    }
    newest
}

/// Call after the browser exits. Pushes the current bundle up and releases
/// the lock. A no-op when sync isn't configured. Best-effort: logged, never
/// fatal — the operator still has their local copy either way.
pub async fn checkin(profile_id: &str) -> Result<()> {
    let Some((cfg, base, token)) = active_config()? else { return Ok(()) };
    let _busy = begin_wait(profile_id, "push").await;
    let holder = device_name(&cfg);
    let c = client();

    let upload: Result<()> = async {
        let bytes = build_bundle(profile_id).context("zip profile for upload")?;
        // Renew first: the lock is only good for 6 hours, and an upload without
        // it is refused.
        relock(&c, &base, &token, profile_id, &holder).await?;
        let resp = c
            .put(format!("{base}/profiles/{profile_id}/bundle"))
            .bearer_auth(&token)
            .header("X-Sync-Holder", &holder)
            .body(bytes)
            .send()
            .await
            .context("upload profile bundle")?;
        if resp.status().as_u16() == 401 { kicked_out(); }
        if !resp.status().is_success() {
            return Err(denied(resp, "upload").await);
        }
        Ok(())
    }
    .await;
    match upload {
        Ok(()) => update_state(|st| { st.pending.remove(profile_id); }),
        Err(e) => {
            update_state(|st| { st.pending.insert(profile_id.to_string()); });
            report_checkin_failed(profile_id, &e);
            return Err(e);
        }
    }
    unlock(&base, &token, profile_id, &holder).await?;
    mark_synced(profile_id).await;

    // The upload and the unlock both succeeded, so the server has the account
    // data; the caches are the bulk of the disk and rebuild themselves. Never
    // reached on a failed upload (the `?` above returns first).
    if cfg.slim_local {
        slim_local_copy(profile_id);
    }
    Ok(())
}

/// Sends every profile on this machine to the server now, login included. For the machine that
/// holds the logged-in copies, after an update changed what travels: a profile is otherwise only
/// sent when it is opened and closed here or edited, so the server can keep holding bundles
/// made before the login could cross between machines — which arrive elsewhere logged out.
/// Returns (sent, skipped); a profile that is running here, busy, or locked by another machine
/// is skipped, not failed.
pub async fn push_all_local() -> Result<(usize, usize)> {
    let Some((cfg, base, token)) = active_config()? else {
        anyhow::bail!("Team Sync is not switched on on this machine");
    };
    let holder = device_name(&cfg);
    let ids: Vec<String> = crate::profile::list_all()?.into_iter().map(|p| p.id).collect();
    let (mut sent, mut skipped) = (0usize, 0usize);
    let mut touched: Vec<String> = Vec::new();
    slog!("push all: {} profiles on this machine", ids.len());
    for id in ids {
        if crate::process::Tracker::shared().is_running(&id) {
            slog!("push all: {id} is running here — skipped");
            skipped += 1;
            continue;
        }
        let Some(_guard) = try_begin(&id, "push") else { skipped += 1; continue };
        match push_profile(&base, &token, &holder, &id).await {
            Ok(true) => { sent += 1; touched.push(id.clone()); }
            Ok(false) => { slog!("push all: {id} is locked by another machine — skipped"); skipped += 1; }
            Err(e) => { slog!("push all: {id}: {e:#}"); skipped += 1; }
        }
    }
    for id in &touched {
        mark_synced(id).await;
    }
    if sent > 0 {
        GENERATION.fetch_add(1, Ordering::Relaxed);
    }
    slog!("push all: {sent} sent, {skipped} skipped");
    Ok((sent, skipped))
}

/// Chromium cache directories that are safe to delete: none of them carries
/// account state, and all are rebuilt on demand.
const CACHE_DIRS: &[&str] = &[
    "Default/Cache",
    "Default/Code Cache",
    "Default/GPUCache",
    "Default/DawnGraphiteCache",
    "Default/DawnWebGPUCache",
    "Default/Service Worker/CacheStorage",
    "Default/Service Worker/ScriptCache",
    "GrShaderCache",
    "GraphiteDawnCache",
    "ShaderCache",
    "Crashpad",
    "BrowserMetrics",
];

fn slim_local_copy(profile_id: &str) {
    let Ok(root) = store::user_data_root() else { return };
    let udd = root.join(profile_id);
    for rel in CACHE_DIRS {
        let p = udd.join(rel);
        if p.is_dir() {
            if let Err(e) = fs::remove_dir_all(&p) {
                eprintln!("[launcher] slim: could not remove {}: {e}", p.display());
            }
        }
    }
}

async fn unlock(base: &str, token: &str, profile_id: &str, holder: &str) -> Result<()> {
    let c = client();
    let resp = c
        .post(format!("{base}/profiles/{profile_id}/unlock"))
        .bearer_auth(token)
        .json(&serde_json::json!({ "holder": holder }))
        .timeout(Duration::from_secs(25))
        .send()
        .await
        .context("release lock")?;
    if !resp.status().is_success() {
        anyhow::bail!("sync server rejected unlock: {}", resp.status());
    }
    Ok(())
}

/// One row for the Settings "Team Sync" test/status action.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RemoteProfileStatus {
    pub id: String,
    pub locked: bool,
    pub holder: Option<String>,
    pub updated_by: Option<String>,
    pub updated_at: Option<String>,
    pub deleted: bool,
}

/// Pulls any remote profiles that don't exist locally. Returns count pulled.
pub async fn pull_missing() -> Result<usize> {
    let Some((_cfg, base, token)) = active_config()? else {
        anyhow::bail!("sync is not enabled");
    };
    let remote = list_remote().await?;
    let c = client();
    let mut pulled = 0usize;
    for r in &remote {
        // Skip if profile already exists locally
        if crate::profile::load_raw(&r.id).is_ok() {
            continue;
        }
        let resp = c
            .get(format!("{base}/profiles/{}/bundle", r.id))
            .bearer_auth(&token)
            .send()
            .await
            .context("download bundle")?;
        if !resp.status().is_success() {
            slog!("pull {} failed: {}", r.id, resp.status());
            continue;
        }
        let bytes = resp.bytes().await.context("read bundle")?;
        if let Err(e) = apply_bundle(&r.id, &bytes) {
            slog!("apply {} failed: {e}", r.id);
            continue;
        }
        pulled += 1;
    }
    Ok(pulled)
}

/// Lists what the server knows about every profile it has seen. Used by the
/// Settings page to prove the connection works before the operator relies on it.
pub async fn list_remote() -> Result<Vec<RemoteProfileStatus>> {
    let Some((_cfg, base, token)) = active_config()? else {
        anyhow::bail!("sync is not enabled");
    };
    let c = client();
    let resp = c
        .get(format!("{base}/profiles"))
        .bearer_auth(&token)
        .send()
        .await
        .context("contact sync server")?;
    if resp.status().as_u16() == 401 { kicked_out(); }
    if !resp.status().is_success() {
        anyhow::bail!("sync server error: {}", resp.status());
    }
    #[derive(serde::Deserialize)]
    struct Row {
        id: String,
        locked: bool,
        holder: Option<String>,
        #[serde(rename = "updatedBy")]
        updated_by: Option<String>,
        #[serde(rename = "updatedAt")]
        updated_at: Option<String>,
        #[serde(default)]
        deleted: bool,
    }
    #[derive(serde::Deserialize)]
    struct Resp {
        profiles: Vec<Row>,
    }
    let body: Resp = resp.json().await.context("parse sync server response")?;
    Ok(body
        .profiles
        .into_iter()
        .map(|r| RemoteProfileStatus {
            id: r.id,
            locked: r.locked,
            holder: r.holder,
            updated_by: r.updated_by,
            updated_at: r.updated_at,
            deleted: r.deleted,
        })
        .collect())
}


// ---- deletions ----

/// Ids being trashed *because the team deleted them*, so the trash hook does not
/// report them straight back.
fn sync_trashing() -> &'static Mutex<std::collections::HashSet<String>> {
    static S: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}

/// Ids whose deletion this run already told the server about (so a refusal is not repeated every round).
fn deletion_reported() -> &'static Mutex<std::collections::HashSet<String>> {
    static S: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}

async fn report_deleted(id: String) {
    let Ok(Some((cfg, base, token))) = active_config() else { return };
    let holder = device_name(&cfg);
    let res = client()
        .post(format!("{base}/profiles/{id}/delete"))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "holder": holder }))
        .timeout(Duration::from_secs(25))
        .send()
        .await;
    match res {
        Ok(r) if r.status().is_success() => mark_synced(&id).await,
        Ok(r) => slog!("delete report {id}: {}", r.status()),
        Err(e) => slog!("delete report {id}: {e}"),
    }
}

/// Called after a profile went to the trash on this machine.
pub fn on_trashed(id: &str) {
    if sync_trashing().lock().map(|s| s.contains(id)).unwrap_or(false) {
        return;
    }
    let id = id.to_string();
    tauri::async_runtime::spawn(report_deleted(id));
}

/// Called after a profile came back from the trash: put it back on the server
/// (which clears the tombstone) so the other machines get it again.
pub fn on_restored(id: &str) {
    let id = id.to_string();
    tauri::async_runtime::spawn(async move {
        let Ok(Some((cfg, base, token))) = active_config() else { return };
        let holder = device_name(&cfg);
        let Some(_guard) = try_begin(&id, "push") else { return };
        match push_profile(&base, &token, &holder, &id).await {
            Ok(_) => mark_synced(&id).await,
            Err(e) => slog!("republish {id}: {e:#}"),
        }
    });
}

// ---- shared library: custom fingerprints ----

async fn library_ids(base: &str, token: &str, kind: &str) -> Result<std::collections::HashSet<String>> {
    let resp = client()
        .get(format!("{base}/library/{kind}"))
        .bearer_auth(token)
        .send()
        .await
        .context("contact sync server")?;
    if !resp.status().is_success() {
        anyhow::bail!("library list rejected: {}", resp.status());
    }
    let v: serde_json::Value = resp.json().await.context("parse library list")?;
    Ok(v.get("items")
        .and_then(|i| i.as_array())
        .map(|a| a.iter().filter_map(|x| x.get("id").and_then(|i| i.as_str()).map(String::from)).collect())
        .unwrap_or_default())
}

async fn library_put(base: &str, token: &str, kind: &str, id: &str, bytes: Vec<u8>) -> Result<()> {
    let resp = client()
        .put(format!("{base}/library/{kind}/{id}"))
        .bearer_auth(token)
        .body(bytes)
        .send()
        .await
        .context("upload library item")?;
    if !resp.status().is_success() {
        anyhow::bail!("library upload rejected: {}", resp.status());
    }
    Ok(())
}

async fn library_get(base: &str, token: &str, kind: &str, id: &str) -> Result<Vec<u8>> {
    let resp = client()
        .get(format!("{base}/library/{kind}/{id}"))
        .bearer_auth(token)
        .send()
        .await
        .context("download library item")?;
    if !resp.status().is_success() {
        anyhow::bail!("library download rejected: {}", resp.status());
    }
    Ok(resp.bytes().await.context("read library item")?.to_vec())
}

/// Brings the two machines' fingerprint libraries to the union: what the server lacks goes
/// up, what this machine lacks comes down. Additive only — deleting an
/// extension or fingerprint on one machine does not delete it on the others.
/// Returns how many items were installed locally.
async fn sync_library(base: &str, token: &str) -> usize {
    let mut installed = 0usize;

    // Fingerprints the operator added themselves (the shipped set is already everywhere).
    if let (Ok(remote), Ok(dir)) = (library_ids(base, token, "fingerprints").await, store::fingerprints_dir()) {
        for id in crate::fingerprints::custom_ids().iter().filter(|id| !remote.contains(*id)) {
            if let Ok(bytes) = fs::read(dir.join(format!("{id}.json"))) {
                if let Err(e) = library_put(base, token, "fingerprints", id, bytes).await {
                    slog!("fingerprint {id} up: {e:#}");
                }
            }
        }
        for id in remote.iter() {
            let path = dir.join(format!("{id}.json"));
            if path.exists() {
                continue;
            }
            match library_get(base, token, "fingerprints", id).await {
                Ok(bytes) => {
                    if fs::write(&path, bytes).is_ok() {
                        crate::fingerprints::note_custom(id);
                        installed += 1;
                    }
                }
                Err(e) => slog!("fingerprint {id} down: {e:#}"),
            }
        }
    }
    installed
}

// ---- background sync ----

/// Uploads a profile the server does not have yet, or one edited here since the
/// last sync. Skips it (Ok(false)) when another device holds the lock.
async fn push_profile(base: &str, token: &str, holder: &str, id: &str) -> Result<bool> {
    let c = client();
    let resp = c
        .post(format!("{base}/profiles/{id}/lock"))
        .bearer_auth(token)
        .json(&serde_json::json!({ "holder": holder }))
        .timeout(Duration::from_secs(25))
        .send()
        .await
        .context("contact sync server")?;
    if resp.status().as_u16() == 409 {
        return Ok(false);
    }
    if !resp.status().is_success() {
        return Err(denied(resp, "lock request").await);
    }
    let result: Result<()> = async {
        let bytes = build_bundle(id).context("zip profile for upload")?;
        let put = c
            .put(format!("{base}/profiles/{id}/bundle"))
            .bearer_auth(token)
            .header("X-Sync-Holder", holder)
            .body(bytes)
            .send()
            .await
            .context("upload profile bundle")?;
        if !put.status().is_success() {
            return Err(denied(put, "upload").await);
        }
        Ok(())
    }
    .await;
    // Always release, or a failed upload would leave the profile locked.
    let _ = unlock(base, token, id, holder).await;
    result?;
    Ok(true)
}

/// Downloads and applies a profile's bundle. Ok(false) = the server has none.
async fn pull_profile(base: &str, token: &str, id: &str) -> Result<bool> {
    let resp = client()
        .get(format!("{base}/profiles/{id}/bundle"))
        .bearer_auth(token)
        .send()
        .await
        .context("download bundle")?;
    if resp.status().as_u16() == 404 {
        return Ok(false);
    }
    if !resp.status().is_success() {
        anyhow::bail!("download rejected: {}", resp.status());
    }
    let bytes = resp.bytes().await.context("read bundle")?;
    apply_bundle(id, &bytes)?;
    Ok(true)
}

fn local_edit_time(id: &str) -> u64 {
    store::profiles_dir()
        .ok()
        .map(|d| d.join(format!("{id}.json")))
        .and_then(|p| fs::metadata(p).ok())
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// One pass: bring in what other machines added or changed, and send up what
/// this one added or edited. Never touches a profile that is running, mid-sync,
/// or locked by someone else. Returns how many local profiles changed.
pub async fn sync_round() -> Result<usize> {
    let Some((cfg, base, token)) = active_config()? else { return Ok(0) };
    let holder = device_name(&cfg);
    let remote = list_remote().await?;
    let state = {
        let _g = state_lock().lock().unwrap_or_else(|e| e.into_inner());
        load_state()
    };
    let local: std::collections::HashSet<String> = crate::profile::list_all()?
        .into_iter()
        .map(|p| p.id)
        .collect();

    let mut changed = 0usize;
    let mut touched: Vec<String> = Vec::new();

    for r in &remote {
        let id = r.id.as_str();
        if crate::process::Tracker::shared().is_running(id) {
            continue;
        }
        if r.locked && r.holder.as_deref() != Some(holder.as_str()) {
            continue;
        }
        let known = state.items.get(id);
        let Some(_guard) = try_begin(id, "pull") else { continue };

        if r.deleted {
            // Deleted on another machine. Only follow if this machine has synced
            // the profile before; otherwise it is just an old copy of something
            // that is gone, and is left alone.
            if local.contains(id) && known.is_some() {
                if let Ok(mut set) = sync_trashing().lock() {
                    set.insert(id.to_string());
                }
                let res = crate::trash::move_to_trash(id);
                if let Ok(mut set) = sync_trashing().lock() {
                    set.remove(id);
                }
                match res {
                    Ok(_) => changed += 1,
                    Err(e) => slog!("trash {id}: {e:#}"),
                }
            }
            if let Some(u) = r.updated_at.clone() {
                update_state(|st| { st.items.insert(id.to_string(), StateItem { remote: u, at: unix_now(), at_ms: 0, proxy: String::new(), fmt: SYNC_FMT }); });
            }
            continue;
        }

        if !local.contains(id) {
            // Synced before but gone here: deleted on this machine — do not bring it
            // back, unless the server has a newer version than the deletion we saw
            // (it was restored elsewhere).
            if let Some(k) = known {
                if r.updated_at.as_deref().map_or(true, |u| u == k.remote) {
                    // The report of that deletion may never have reached the server (it was
                    // unreachable, or this machine was offline): say it again, once per run.
                    let first = deletion_reported().lock().map(|mut set| set.insert(id.to_string())).unwrap_or(false);
                    if first {
                        report_deleted(id.to_string()).await;
                    }
                    continue;
                }
            }
            slog!("round: {id} is not on this machine yet — pulling it");
            match pull_profile(&base, &token, id).await {
                Ok(true) => { changed += 1; touched.push(id.to_string()); }
                Ok(false) => {}
                Err(e) => slog!("pull {id}: {e:#}"),
            }
            continue;
        }

        let Some(known) = known else {
            // Existed before auto-sync: adopt the server's current version as the
            // baseline rather than overwriting anything.
            if let Some(u) = r.updated_at.clone() {
                update_state(|st| { st.items.insert(id.to_string(), StateItem { remote: u, at: unix_now(), at_ms: 0, proxy: proxy_signature(id), fmt: SYNC_FMT }); });
            }
            continue;
        };

        // A profile whose last close never reached the server holds the newer
        // copy (a fresh login): send it up rather than pulling the older one over it.
        let unsaved = state.pending.contains(id);
        if !unsaved && r.updated_at.as_deref().is_some_and(|u| u != known.remote) {
            slog!("round: {id} changed on the server (by {}, {} → {}) — taking it", r.updated_by.as_deref().unwrap_or("?"), known.remote, r.updated_at.as_deref().unwrap_or("?"));
            match pull_profile(&base, &token, id).await {
                Ok(true) => { changed += 1; touched.push(id.to_string()); }
                Ok(false) => {}
                Err(e) => slog!("update {id}: {e:#}"),
            }
        } else if unsaved || local_edit_time(id) > known.at || proxy_signature(id) != known.proxy {
            let why = if unsaved { "its last close was not saved" } else if proxy_signature(id) != known.proxy { "its proxy changed here" } else { "its settings were edited here" };
            slog!("round: sending {id} to the server — {why}");
            match push_profile(&base, &token, &holder, id).await {
                Ok(true) => {
                    touched.push(id.to_string());
                    if unsaved {
                        update_state(|st| { st.pending.remove(id); });
                    }
                }
                Ok(false) => {}
                Err(e) => slog!("push {id}: {e:#}"),
            }
        }
    }

    // Profiles the server has never seen.
    let remote_ids: std::collections::HashSet<&str> = remote.iter().map(|r| r.id.as_str()).collect();
    for id in local.iter().filter(|id| !remote_ids.contains(id.as_str())) {
        if crate::process::Tracker::shared().is_running(id) {
            continue;
        }
        let Some(_guard) = try_begin(id, "push") else { continue };
        match push_profile(&base, &token, &holder, id).await {
            Ok(true) => touched.push(id.clone()),
            Ok(false) => {}
            Err(e) => slog!("first upload {id}: {e:#}"),
        }
    }

    for id in &touched {
        mark_synced(id).await;
    }
    changed += sync_library(&base, &token).await;
    if changed > 0 {
        GENERATION.fetch_add(1, Ordering::Relaxed);
    }
    Ok(changed)
}

/// Asks the server to say when something changes. Returns the new position and
/// the ids uploaded since `after` (None = just report where the server is now).
async fn wait_for_events(base: &str, token: &str, after: Option<u64>) -> Result<(u64, Vec<String>)> {
    let url = match after {
        Some(n) => format!("{base}/events/wait?after={n}"),
        None => format!("{base}/events/wait"),
    };
    let resp = client()
        .get(url)
        .bearer_auth(token)
        .timeout(Duration::from_secs(40))
        .send()
        .await
        .context("contact sync server")?;
    if resp.status().as_u16() == 401 { kicked_out(); }
    if !resp.status().is_success() {
        anyhow::bail!("events rejected: {}", resp.status());
    }
    #[derive(serde::Deserialize)]
    struct R {
        seq: u64,
        #[serde(default)]
        ids: Vec<String>,
    }
    let r: R = resp.json().await.context("parse events")?;
    Ok((r.seq, r.ids))
}

/// Trigger for "something changed here" (a profile was saved or created): the
/// next pass sends it up now rather than waiting for the safety-net timer.
static KICK: OnceLock<tokio::sync::Notify> = OnceLock::new();
fn kick_cell() -> &'static tokio::sync::Notify {
    KICK.get_or_init(tokio::sync::Notify::new)
}
pub fn kick() {
    kick_cell().notify_one();
}

/// Keeps this machine in step with the team server for as long as the app
/// lives. Other machines' uploads arrive as events and are pulled at once; a
/// slow safety-net pass (and `kick`) covers local edits and dropped connections.
pub async fn run_forever() {
    tokio::time::sleep(Duration::from_secs(6)).await;
    loop {
        let Ok(Some((_cfg, base, token))) = active_config() else {
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        };
        // Catch up on anything missed while offline, then listen from "now".
        if let Err(e) = sync_round().await {
            slog!("round failed: {e:#}");
        }
        let mut pos = match wait_for_events(&base, &token, None).await {
            Ok((seq, _)) => seq,
            Err(e) => {
                slog!("cannot listen: {e:#}");
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };
        let mut last_full = std::time::Instant::now();
        loop {
            // Config changed (turned off, new server)? Start over.
            match active_config() {
                Ok(Some((_, b, t))) if b == base && t == token => {}
                _ => break,
            }
            tokio::select! {
                res = wait_for_events(&base, &token, Some(pos)) => match res {
                    Ok((seq, ids)) => {
                        pos = seq;
                        if !ids.is_empty() {
                            if let Err(e) = sync_round().await {
                                slog!("round failed: {e:#}");
                            }
                            last_full = std::time::Instant::now();
                        }
                    }
                    Err(e) => {
                        slog!("listen dropped: {e:#}");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        break; // reconnect and catch up
                    }
                },
                _ = kick_cell().notified() => {
                    if let Err(e) = sync_round().await {
                        slog!("round failed: {e:#}");
                    }
                    last_full = std::time::Instant::now();
                }
            }
            if last_full.elapsed() > Duration::from_secs(300) {
                let _ = sync_round().await;
                last_full = std::time::Instant::now();
            }
        }
    }
}

#[cfg(test)]
pub(crate) static TEST_ROOT_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    /// A login sealed on one machine must open on the next. Cookies are sealed with a key
    /// that belongs to the machine (a DPAPI key on Windows), so the raw database from the
    /// first one is noise on the second — a profile arrived logged out, its saved passwords
    /// gone, and `Local State` (the key itself) would have replaced the receiving machine's.
    /// The bundle carries a portable copy instead; the receiver seals it again.
    #[test]
    fn logins_are_resealed_for_the_receiving_machine() {
        let _g = TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let a = std::env::temp_dir().join(format!("hir-logina-{}", uuid::Uuid::new_v4()));
        let b = std::env::temp_dir().join(format!("hir-loginb-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let id = "login-1";

        // Machine A: a profile with a Facebook login cookie, sealed with A's key.
        store::set_data_root(Some(a.clone()));
        let mut stored = crate::profile::StoredProfile::default();
        stored.meta.id = id.to_string();
        stored.config.insert("name".into(), serde_json::json!("Login test"));
        crate::profile::save_raw(&mut stored).unwrap();
        let cookie: cookies::Cookie = serde_json::from_value(serde_json::json!({
            "domain": ".facebook.com", "name": "c_user", "value": "100012345", "path": "/",
            "expires": 1893456000.0, "secure": true, "httpOnly": false
        })).unwrap();
        cookies::import(id, &[cookie]).expect("seed a cookie");
        let udd_a = store::user_data_root().unwrap().join(id);
        let sealed_on_a = std::fs::read(cookies::cookie_db(&udd_a)).unwrap();
        let bytes = build_bundle(id).unwrap();

        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes.clone())).unwrap();
        let names: Vec<String> = (0..z.len()).map(|i| z.by_index(i).unwrap().name().to_string()).collect();
        let has = |n: &str| names.iter().any(|x| x == n);
        assert!(has("portable/Cookies"), "the login must travel as a portable copy: {names:?}");
        assert!(!has("user-data/Default/Network/Cookies"), "the sealed database must not travel raw");
        assert!(!has("user-data/Local State"), "Local State holds this machine's key and must not travel");
        let mut portable = Vec::new();
        z.by_name("portable/Cookies").unwrap().read_to_end(&mut portable).unwrap();
        assert!(portable.windows(9).any(|w| w == b"100012345"), "the copy holds the plain value");
        assert!(!sealed_on_a.windows(9).any(|w| w == b"100012345"), "while the original is sealed");

        // Machine B: its own, empty data root (so its own key).
        store::set_data_root(Some(b.clone()));
        let mut stored_b = crate::profile::StoredProfile::default();
        stored_b.meta.id = id.to_string();
        crate::profile::save_raw(&mut stored_b).unwrap();
        apply_bundle(id, &bytes).unwrap();
        let got = cookies::export(id).expect("read the cookies back with B's own key");
        let found = got.iter().find(|c| c.name == "c_user").map(|c| c.value.clone());

        store::set_data_root(None);
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
        assert_eq!(found.as_deref(), Some("100012345"), "the login must still be there on the second machine");
    }

    /// A profile can hold two cookie databases: the one the browser reads (Network/Cookies) and
    /// an old leftover (Default/Cookies) — a Mac had 76 rows in the first and 5 stale ones in
    /// the second. The restored login used to go into whichever existed first, the old file, and
    /// the profile came up logged out. It must land in the live one and leave the old one alone.
    #[test]
    fn a_restored_login_lands_in_the_cookie_file_the_browser_reads_not_a_stale_old_one() {
        let _g = TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let a = std::env::temp_dir().join(format!("hir-stale-a-{}", uuid::Uuid::new_v4()));
        let b = std::env::temp_dir().join(format!("hir-stale-b-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let id = "stale-1";
        let cookie = |name: &str, value: &str| -> cookies::Cookie {
            serde_json::from_value(serde_json::json!({
                "domain": ".facebook.com", "name": name, "value": value, "path": "/", "expires": 1893456000.0, "secure": true
            })).unwrap()
        };
        let count = |db: &std::path::Path, name: &str| -> i64 {
            let conn = rusqlite::Connection::open(db).unwrap();
            conn.query_row("SELECT count(1) FROM cookies WHERE name = ?1", [name], |r| r.get(0)).unwrap()
        };

        // The machine that is logged in.
        store::set_data_root(Some(a.clone()));
        let mut stored = crate::profile::StoredProfile::default();
        stored.meta.id = id.to_string();
        crate::profile::save_raw(&mut stored).unwrap();
        cookies::import(id, &[cookie("c_user", "100012345")]).unwrap();
        let bytes = build_bundle(id).unwrap();

        // The other one: it has opened the profile (its own live database, a guest cookie in it)
        // and an old file lies beside it.
        store::set_data_root(Some(b.clone()));
        let mut stored_b = crate::profile::StoredProfile::default();
        stored_b.meta.id = id.to_string();
        crate::profile::save_raw(&mut stored_b).unwrap();
        cookies::import(id, &[cookie("guest", "1")]).unwrap();
        let udd_b = store::user_data_root().unwrap().join(id);
        let live = udd_b.join("Default/Network/Cookies");
        let old = udd_b.join("Default/Cookies");
        assert!(live.is_file(), "the import went to the live place");
        std::fs::write(&old, b"stale leftover").unwrap();

        // Which file the profile means by "its cookies" with both there: the live one.
        assert_eq!(cookies::cookie_db(&udd_b), live);
        apply_bundle(id, &bytes).unwrap();
        let in_live = count(&live, "c_user");
        let old_after = std::fs::read(&old).unwrap();

        // And with only the old file there (a profile the new engine has not opened yet): the
        // login still goes to the live place, which the old file does not stop.
        let only_old = udd_b.parent().unwrap().join("only-old");
        std::fs::create_dir_all(only_old.join("Default")).unwrap();
        std::fs::write(only_old.join("Default/Cookies"), b"stale leftover").unwrap();
        let mut stored_c = crate::profile::StoredProfile::default();
        stored_c.meta.id = "only-old".to_string();
        crate::profile::save_raw(&mut stored_c).unwrap();
        apply_bundle("only-old", &bytes).unwrap();
        let live_c = only_old.join("Default/Network/Cookies");
        let in_live_c = if live_c.is_file() { count(&live_c, "c_user") } else { -1 };
        let old_c_after = std::fs::read(only_old.join("Default/Cookies")).unwrap();

        store::set_data_root(None);
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
        assert_eq!(in_live, 1, "the login must be in the file the browser reads");
        assert_eq!(old_after, b"stale leftover", "the old file is left exactly as it was");
        assert_eq!(in_live_c, 1, "with only an old file around, the login still goes to the live place");
        assert_eq!(old_c_after, b"stale leftover");
    }

    /// A login that arrives empty must not wipe one that is there: that is what a machine sends
    /// when it could not read its own cookies, and taking it logs the receiver out too.
    #[test]
    fn an_empty_incoming_login_does_not_replace_the_one_already_here() {
        let _g = TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let a = std::env::temp_dir().join(format!("hir-empty-a-{}", uuid::Uuid::new_v4()));
        let b = std::env::temp_dir().join(format!("hir-empty-b-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let id = "empty-1";
        let cookie: cookies::Cookie = serde_json::from_value(serde_json::json!({
            "domain": ".facebook.com", "name": "c_user", "value": "100077", "path": "/", "expires": 1893456000.0, "secure": true
        })).unwrap();

        // A machine whose cookie database is there but empty.
        store::set_data_root(Some(a.clone()));
        let mut stored = crate::profile::StoredProfile::default();
        stored.meta.id = id.to_string();
        crate::profile::save_raw(&mut stored).unwrap();
        cookies::import(id, &[]).unwrap();
        let empty_bundle = build_bundle(id).unwrap();

        // One that is logged in.
        store::set_data_root(Some(b.clone()));
        let mut stored_b = crate::profile::StoredProfile::default();
        stored_b.meta.id = id.to_string();
        crate::profile::save_raw(&mut stored_b).unwrap();
        cookies::import(id, &[cookie]).unwrap();
        apply_bundle(id, &empty_bundle).unwrap();
        let kept = cookies::export(id).unwrap();
        let log = log_tail(20);

        store::set_data_root(None);
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
        assert_eq!(kept.iter().find(|c| c.name == "c_user").map(|c| c.value.as_str()), Some("100077"), "the login must survive an empty one arriving");
        assert!(log.contains("merged with this machine's own"), "and the log says what was done: {log}");
    }

    /// The login that arrives is merged with the one that is here: for each cookie the more recent
    /// one stays, a cookie only one side has is kept, and nothing is lost. Before, the incoming
    /// database replaced the local one, so a machine that had gone logged out and pushed that state
    /// took the login away from every other machine on its next background pull.
    #[test]
    fn an_arriving_login_is_merged_and_the_newer_cookie_wins_in_both_directions() {
        let _g = TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let a = std::env::temp_dir().join(format!("hir-merge-a-{}", uuid::Uuid::new_v4()));
        let b = std::env::temp_dir().join(format!("hir-merge-b-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let id = "merge-1";
        let ck = |name: &str, value: &str| -> cookies::Cookie {
            serde_json::from_value(serde_json::json!({
                "domain": ".facebook.com", "name": name, "value": value, "path": "/", "expires": 1893456000.0, "secure": true
            })).unwrap()
        };
        let make = |root: &std::path::Path| {
            store::set_data_root(Some(root.to_path_buf()));
            let mut st = crate::profile::StoredProfile::default();
            st.meta.id = id.to_string();
            crate::profile::save_raw(&mut st).unwrap();
        };
        let value_of = |name: &str| cookies::export(id).unwrap().into_iter().find(|c| c.name == name).map(|c| c.value);
        let pause = || std::thread::sleep(Duration::from_millis(30));

        // 1. This machine (B) has the newer c_user; the incoming copy (A) has an older one and a
        //    cookie B lacks.
        make(&a);
        cookies::import(id, &[ck("c_user", "from-A-older"), ck("only_a", "a")]).unwrap();
        let from_a = build_bundle(id).unwrap();
        pause();
        make(&b);
        cookies::import(id, &[ck("c_user", "from-B-newer"), ck("only_b", "b")]).unwrap();
        apply_bundle(id, &from_a).unwrap();
        let newer_here_wins = (value_of("c_user"), value_of("only_a"), value_of("only_b"));
        let key_check = log_tail(30);

        // 2. The other way round: the incoming c_user is the newer one.
        make(&b);
        let _ = std::fs::remove_dir_all(store::user_data_root().unwrap().join(id));
        cookies::import(id, &[ck("c_user", "from-B-older")]).unwrap();
        pause();
        make(&a);
        let _ = std::fs::remove_dir_all(store::user_data_root().unwrap().join(id));
        cookies::import(id, &[ck("c_user", "from-A-newer")]).unwrap();
        let from_a2 = build_bundle(id).unwrap();
        make(&b);
        apply_bundle(id, &from_a2).unwrap();
        let newer_incoming_wins = value_of("c_user");

        // 3. An empty incoming login takes nothing away.
        make(&a);
        let _ = std::fs::remove_dir_all(store::user_data_root().unwrap().join(id));
        cookies::import(id, &[]).unwrap();
        let empty = build_bundle(id).unwrap();
        make(&b);
        apply_bundle(id, &empty).unwrap();
        let after_empty = value_of("c_user");

        store::set_data_root(None);
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
        assert!(
            key_check.contains("of this machine's own 2 sealed values, 2 can be opened with the key assumed here") && !key_check.contains("NONE of them"),
            "the log says the assumed key opens this machine's own cookies: {key_check}"
        );
        assert_eq!(newer_here_wins.0.as_deref(), Some("from-B-newer"), "an older incoming cookie must not replace a newer one");
        assert_eq!(newer_here_wins.1.as_deref(), Some("a"), "a cookie only the incoming copy has is added");
        assert_eq!(newer_here_wins.2.as_deref(), Some("b"), "a cookie only this machine has is kept");
        assert_eq!(newer_incoming_wins.as_deref(), Some("from-A-newer"), "a newer incoming cookie does replace an older one");
        assert_eq!(after_empty.as_deref(), Some("from-A-newer"), "an empty login takes nothing away");
    }

    /// A row this machine's key cannot open (sealed by another key: what a Mac browser writes with
    /// its Keychain key, before it is told to use the fixed one) used to win the merge whenever it
    /// was the newer, and the cookie that could be opened never got in. Such rows are dropped first.
    /// (Only meaningful where keys differ from one data folder to the next: Windows.)
    #[cfg(windows)]
    #[test]
    fn a_newer_row_this_machine_cannot_open_does_not_block_the_one_that_arrives() {
        let _g = TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let a = std::env::temp_dir().join(format!("hir-purge-a-{}", uuid::Uuid::new_v4()));
        let b = std::env::temp_dir().join(format!("hir-purge-b-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let id = "purge-1";
        let cookie: cookies::Cookie = serde_json::from_value(serde_json::json!({
            "domain": ".facebook.com", "name": "c_user", "value": "100555", "path": "/", "expires": 1893456000.0, "secure": true
        })).unwrap();

        // The machine that is logged in; its raw (sealed with ITS key) database is kept aside.
        store::set_data_root(Some(a.clone()));
        let mut st = crate::profile::StoredProfile::default();
        st.meta.id = id.to_string();
        crate::profile::save_raw(&mut st).unwrap();
        cookies::import(id, &[cookie]).unwrap();
        let udd_a = store::user_data_root().unwrap().join(id);
        let foreign_db = std::fs::read(cookies::cookie_db(&udd_a)).unwrap();
        let bundle = build_bundle(id).unwrap();

        // This machine: the same cookie, same age, but sealed with a key that is not this machine's.
        store::set_data_root(Some(b.clone()));
        let mut st_b = crate::profile::StoredProfile::default();
        st_b.meta.id = id.to_string();
        crate::profile::save_raw(&mut st_b).unwrap();
        let live = store::user_data_root().unwrap().join(id).join("Default/Network/Cookies");
        std::fs::create_dir_all(live.parent().unwrap()).unwrap();
        std::fs::write(&live, &foreign_db).unwrap();

        apply_bundle(id, &bundle).unwrap();
        let got = cookies::export(id).unwrap().into_iter().find(|c| c.name == "c_user").map(|c| c.value);
        let log = log_tail(30);

        store::set_data_root(None);
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
        assert_eq!(got.as_deref(), Some("100555"), "the cookie that arrives must be the one that can be read: {log}");
        assert!(log.contains("could not be opened with the key used here"), "and the log says what was dropped: {log}");
    }

    /// A copy that was restored by an earlier build may have its login in the wrong place or sealed
    /// with the wrong key. "Nothing new on the server" must not keep it that way: the first open after
    /// the upgrade takes the full copy once.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_copy_restored_by_an_older_build_is_pulled_in_full_once_before_it_may_be_skipped() {
        with_synced_profile(|_base, _token, cookies| async move {
            checkout("sess-1").await.expect("open");
            std::fs::write(&cookies, "logged-in").unwrap();
            checkin("sess-1").await.expect("close");
            tokio::time::sleep(Duration::from_millis(30)).await;

            // As written by a build before SYNC_FMT existed.
            update_state(|st| { st.items.get_mut("sess-1").unwrap().fmt = 0; });
            checkout("sess-1").await.expect("reopen");
            let last = log_tail(1);
            assert!(last.contains("download") && !last.contains("already up to date"), "{last}");
            checkin("sess-1").await.expect("close");
            tokio::time::sleep(Duration::from_millis(30)).await;

            // Recorded the current way, it may skip again.
            checkout("sess-1").await.expect("reopen again");
            assert!(log_tail(1).contains("already up to date"), "{}", log_tail(1));
        }).await;
    }

    /// The admin removes a member. Before, the member's machine merely switched sync off and every
    /// profile it had downloaded stayed fully usable. Now the first call that comes back 401 deletes
    /// the team's profiles from that machine — browser data, trash copy, the proxies that came with
    /// them — and leaves what was only ever local.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn removing_a_member_takes_the_team_profiles_off_their_machine() {
        with_synced_profile(|base, admin_token, _cookies| async move {
            // A staff member, and this machine becomes theirs.
            let c = reqwest::Client::new();
            let made: serde_json::Value = c.put(format!("{base}/admin/members")).bearer_auth(&admin_token)
                .json(&serde_json::json!({"name": "staff", "role": "admin"})).send().await.unwrap().json().await.unwrap();
            let (staff_token, staff_id) = (made["token"].as_str().unwrap().to_string(), made["id"].as_str().unwrap().to_string());
            let mut s = settings::load().unwrap();
            s.sync.token = Some(staff_token);
            settings::save(&s).unwrap();

            // A team profile on this machine (it syncs), a local-only one, and a team proxy.
            checkout("sess-1").await.expect("the member opens a team profile");
            checkin("sess-1").await.expect("and closes it");
            let mut local_only = crate::profile::StoredProfile::default();
            local_only.meta.id = "mine-only".into();
            crate::profile::save_raw(&mut local_only).unwrap();
            let team_proxy = crate::proxy::upsert(crate::proxy::ProxyEntry {
                id: String::new(), name: "from the team".into(), kind: crate::proxy::ProxyKind::Http,
                host: "203.0.113.5".into(), port: 3128, username: "u".into(), password: "p".into(),
                country: String::new(), notes: String::new(),
            }).unwrap();
            update_state(|st| { st.team_proxies.insert(team_proxy.id.clone()); });
            assert!(crate::profile::load_raw("sess-1").is_ok());

            // The admin removes the member.
            let gone = c.post(format!("{base}/admin/members/{staff_id}/delete")).bearer_auth(&admin_token).send().await.unwrap();
            assert!(gone.status().is_success(), "{}", gone.status());

            // The next thing the machine does is refused, and that is enough.
            assert!(checkout("sess-1").await.is_err(), "the removed member's token is refused");
            let started = std::time::Instant::now();
            while crate::profile::load_raw("sess-1").is_ok() && started.elapsed() < Duration::from_secs(10) {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            let udd = store::user_data_root().unwrap().join("sess-1");
            assert!(crate::profile::load_raw("sess-1").is_err(), "the team profile is gone from this machine");
            assert!(!udd.exists(), "and so is its browser data");
            assert!(crate::profile::load_raw("mine-only").is_ok(), "a profile that was only ever local stays");
            assert!(crate::proxy::get(&team_proxy.id).unwrap().is_none(), "the proxy that came with the team is removed");
            let st = load_state();
            assert!(st.items.is_empty() && st.pending.is_empty() && st.team_proxies.is_empty(), "no trace of the team's profiles in the sync state");
            let s = settings::load().unwrap();
            assert!(!s.sync.enabled && s.sync.token.is_none(), "sync is off and the token gone");
            assert!(log_tail(10).contains("removed from the team"), "{}", log_tail(10));
        }).await;
    }

    /// The same wipe when the member leaves on their own (the Disconnect button): team profiles
    /// deleted, sync state forgotten.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn wiping_the_team_deletes_its_profiles_and_forgets_the_sync_state() {
        with_synced_profile(|_base, _token, _cookies| async move {
            checkout("sess-1").await.expect("open");
            checkin("sess-1").await.expect("close");
            assert!(crate::profile::load_raw("sess-1").is_ok());
            let n = wipe_team_data().await;
            assert_eq!(n, 1);
            assert!(crate::profile::load_raw("sess-1").is_err());
            assert!(load_state().items.is_empty());
        }).await;
    }

    /// A team profile is opened only while this machine is in the team (or hosts it). A machine that
    /// has left, was removed, or just switched sync off in the settings keeps the files but not the
    /// right to open them from the app.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_team_profile_is_not_opened_by_a_machine_that_is_out_of_the_team() {
        let _g = TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = std::env::temp_dir().join(format!("hir-access-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        store::set_data_root(Some(tmp.clone()));
        update_state(|st| {
            st.items.insert("team-1".into(), StateItem { remote: "v".into(), at: 1, at_ms: 1, proxy: String::new(), fmt: SYNC_FMT });
        });
        let mut s = settings::load().unwrap();
        s.sync.enabled = false;
        s.server_host.token = None;
        settings::save(&s).unwrap();

        let out_of_team = ensure_access("team-1").is_err();
        let local_is_free = ensure_access("never-synced").is_ok();
        let launch = crate::launch::launch_profile_synced("team-1", false, false, None, 0, "").await;

        let mut in_team = settings::load().unwrap();
        in_team.sync.enabled = true;
        settings::save(&in_team).unwrap();
        let member_ok = ensure_access("team-1").is_ok();

        let mut host = settings::load().unwrap();
        host.sync.enabled = false;
        host.server_host.token = Some("the-servers-own-token".into());
        settings::save(&host).unwrap();
        let host_ok = ensure_access("team-1").is_ok();

        store::set_data_root(None);
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(out_of_team, "out of the team: refused");
        assert!(local_is_free, "a profile that never came from a team is nobody's business");
        let refused = launch.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(refused.contains("thuộc nhóm"), "the launch is refused with a reason: {refused:?}");
        assert!(member_ok, "in the team: allowed");
        assert!(host_ok, "the machine that hosts the team: allowed");
    }

    // ---- a long day across several machines ----

    fn at(root: &std::path::Path) {
        store::set_data_root(Some(root.to_path_buf()));
    }

    fn sim_cookie(name: &str, value: &str) -> cookies::Cookie {
        serde_json::from_value(serde_json::json!({
            "domain": ".facebook.com", "name": name, "value": value, "path": "/", "expires": 1893456000.0, "secure": true
        })).unwrap()
    }

    /// The login this machine's profile holds: the number inside its `c_user` cookie ("u-7" → 7).
    fn sim_login(id: &str) -> Option<u32> {
        let all = cookies::export(id).ok()?;
        all.iter().find(|c| c.name == "c_user").and_then(|c| c.value.strip_prefix("u-")).and_then(|v| v.parse().ok())
    }

    /// The site's own storage (a stand-in for Local Storage): "tok-7" → 7.
    fn sim_token(id: &str) -> Option<u32> {
        let f = store::user_data_root().ok()?.join(id).join("Default/Local Storage/leveldb/000003.log");
        std::fs::read_to_string(f).ok()?.strip_prefix("tok-")?.trim().parse().ok()
    }

    async fn sim_login_on(root: &std::path::Path, id: &str, k: u32) {
        at(root);
        checkout(id).await.expect("open");
        cookies::import(id, &[sim_cookie("c_user", &format!("u-{k}")), sim_cookie("xs", &format!("x-{k}"))]).unwrap();
        let tok = store::user_data_root().unwrap().join(id).join("Default/Local Storage/leveldb/000003.log");
        std::fs::create_dir_all(tok.parent().unwrap()).unwrap();
        std::fs::write(tok, format!("tok-{k}")).unwrap();
        checkin(id).await.expect("close");
    }

    async fn sim_visit_on(root: &std::path::Path, id: &str) {
        at(root);
        checkout(id).await.expect("open");
        checkin(id).await.expect("close");
    }

    /// A machine whose login cannot be restored (what a Mac whose browser seals with another key
    /// looks like to the launcher): it opens the profile, finds no cookies it can use, and closes.
    async fn sim_broken_visit_on(root: &std::path::Path, id: &str) {
        at(root);
        checkout(id).await.expect("open");
        let udd = store::user_data_root().unwrap().join(id);
        let _ = std::fs::remove_file(udd.join("Default/Network/Cookies"));
        let _ = std::fs::remove_file(udd.join("Default/Cookies"));
        checkin(id).await.expect("close");
    }

    /// Several machines share one profile through the team server for a long, random stretch: logins
    /// on two healthy machines, visits, background rounds, and a third machine that cannot restore
    /// the login and keeps pushing what it has. A healthy machine must never lose a login it had
    /// (or see an older one come back over a newer), and a machine added at the end must start from
    /// the latest login — the server's copy must not have been emptied by the broken machine.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_long_day_of_logins_and_syncs_never_costs_a_healthy_machine_its_login() {
        let _g = TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = std::env::temp_dir().join(format!("hir-sim-{}", uuid::Uuid::new_v4()));
        for d in ["srv", "A", "B", "C", "W"] {
            std::fs::create_dir_all(tmp.join(d)).unwrap();
        }
        // Puts everything back even if an assertion below stops the test: left in place, the
        // server and its data folder would bleed into every test that runs after this one.
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = crate::team_server::stop();
                crate::team_server::set_server_dir_for_tests(None);
                store::set_data_root(None);
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(tmp.clone());
        crate::team_server::set_server_dir_for_tests(Some(tmp.join("srv")));
        at(&tmp.join("srv"));
        let token = "sim-token-123456789";
        let port = crate::team_server::start(0, token.to_string()).await.expect("start");
        let base = format!("http://127.0.0.1:{port}");
        let id = "sim-1";
        let setup = |name: &str| {
            at(&tmp.join(name));
            let mut s = settings::load().unwrap();
            s.sync.enabled = true;
            s.sync.server_url = Some(base.clone());
            s.sync.token = Some(token.to_string());
            s.sync.device_name = Some(format!("machine-{name}"));
            s.sync.slim_local = false;
            settings::save(&s).unwrap();
        };
        for m in ["A", "B", "C"] {
            setup(m);
        }
        at(&tmp.join("A"));
        let mut stored = crate::profile::StoredProfile::default();
        stored.meta.id = id.to_string();
        stored.config.insert("name".into(), serde_json::json!("Shared account"));
        crate::profile::save_raw(&mut stored).unwrap();

        let mut x: u64 = 0x5eed_1234_abcd_0001;
        let mut next = move |n: u32| {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((x >> 33) as u32) % n
        };
        let (a, b, c) = (tmp.join("A"), tmp.join("B"), tmp.join("C"));
        let mut latest = 1u32;
        sim_login_on(&a, id, latest).await;
        // What each healthy machine has held, so a step backwards is caught.
        let mut seen: std::collections::HashMap<&str, (u32, u32)> = std::collections::HashMap::new();
        let mut trail: Vec<String> = vec!["A logs in (1)".into()];

        for step in 0..120 {
            let who = ["A", "B", "C"][next(3) as usize];
            let root = tmp.join(who);
            let what = match next(10) {
                0 | 1 if who != "B" => {
                    latest += 1;
                    sim_login_on(&root, id, latest).await;
                    format!("{who} logs in ({latest})")
                }
                2..=5 => {
                    if who == "B" { sim_broken_visit_on(&root, id).await } else { sim_visit_on(&root, id).await }
                    format!("{who} opens and closes")
                }
                _ => {
                    at(&root);
                    let _ = sync_round().await;
                    format!("{who} background round")
                }
            };
            trail.push(format!("{step}: {what}"));
            tokio::time::sleep(Duration::from_millis(3)).await;

            // Healthy machines only.
            for (m, root) in [("A", &a), ("C", &c)] {
                at(root);
                let (login, token) = (sim_login(id), sim_token(id));
                if let Some((had_login, had_token)) = seen.get(m).copied() {
                    assert!(login.is_some_and(|k| k >= had_login), "{m} lost or went back on its login (had {had_login}, now {login:?}) after: {}", trail.join(" | "));
                    assert!(token.is_some_and(|k| k >= had_token), "{m}'s site storage went back (had {had_token}, now {token:?}) after: {}", trail.join(" | "));
                }
                if let (Some(l), Some(t)) = (login, token) {
                    let e = seen.entry(m).or_insert((0, 0));
                    *e = (e.0.max(l), e.1.max(t));
                }
            }
        }

        // The broken machine pushes last, and then a machine that has never seen the profile joins.
        sim_login_on(&a, id, latest + 1).await;
        latest += 1;
        sim_broken_visit_on(&b, id).await;
        sim_visit_on(&c, id).await;
        at(&c);
        let c_after = (sim_login(id), sim_token(id));
        setup("W");
        at(&tmp.join("W"));
        checkout(id).await.expect("a new machine opens the profile");
        let w_login = sim_login(id);
        let w_token = sim_token(id);
        let _ = checkin(id).await;

        assert_eq!(c_after.0, Some(latest), "a healthy machine ends on the latest login");
        assert_eq!(c_after.1, Some(latest), "and the latest site storage");
        assert_eq!(w_login, Some(latest), "a machine added at the end starts logged in: the broken machine's pushes did not empty the server's copy");
        assert_eq!(w_token, Some(latest));
    }

    /// A computer switched off under the browser can leave its cookie file damaged, missing, or
    /// without the login. The copy kept beside it puts the login back — and does nothing while the
    /// live file still has it, or when there is no copy to put back.
    #[test]
    fn the_logins_lost_with_the_computer_are_put_back_from_the_saved_copy() {
        let _g = TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = std::env::temp_dir().join(format!("hir-keep-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        store::set_data_root(Some(tmp.clone()));
        struct Reset(std::path::PathBuf);
        impl Drop for Reset {
            fn drop(&mut self) {
                store::set_data_root(None);
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _reset = Reset(tmp.clone());

        let id = "keep-1";
        let mut stored = crate::profile::StoredProfile::default();
        stored.meta.id = id.to_string();
        crate::profile::save_raw(&mut stored).unwrap();
        let udd = store::user_data_root().unwrap().join(id);
        let live = cookies::cookie_db(&udd);
        let sites = |n: u32| -> Vec<cookies::Cookie> {
            let mut v = vec![sim_cookie("c_user", "u-5"), sim_cookie("xs", "x-5")];
            for i in 0..n {
                let mut c = sim_cookie(&format!("SID{i}"), "g");
                c.domain = ".google.com".into();
                v.push(c);
            }
            v
        };

        assert!(cookies::restore_if_lost(id).is_none(), "no copy, nothing to put back");
        cookies::import(id, &sites(6)).unwrap();
        assert!(cookies::snapshot(id).unwrap(), "a profile with logins is copied");
        assert!(cookies::restore_if_lost(id).is_none(), "the live file still has them");

        // The file is damaged.
        std::fs::write(&live, b"this is not a database").unwrap();
        assert!(cookies::snapshot(id).is_ok_and(|made| !made), "a damaged file never replaces the good copy");
        let note = cookies::restore_if_lost(id).expect("restored");
        assert!(note.contains("unreadable"), "{note}");
        assert_eq!(sim_login(id), Some(5));
        assert_eq!(cookies::export(id).unwrap().len(), 8, "Gmail's cookies came back with Facebook's");

        // The file is gone.
        std::fs::remove_file(&live).unwrap();
        assert!(cookies::restore_if_lost(id).expect("restored").contains("missing"));
        assert_eq!(cookies::export(id).unwrap().len(), 8);

        // The file is there but emptied down to a stray cookie or two.
        std::fs::remove_file(&live).unwrap();
        cookies::import(id, &[sim_cookie("fr", "other")]).unwrap();
        assert!(cookies::snapshot(id).is_ok_and(|made| !made), "an emptied file never replaces the good copy");
        assert!(cookies::restore_if_lost(id).expect("restored").contains("down to"));
        assert_eq!(cookies::export(id).unwrap().len(), 8);

        // Logging out of one site changes nothing: most of the file is still there.
        cookies::import(id, &sites(6)).unwrap();
        assert!(cookies::restore_if_lost(id).is_none());
        let line = cookies::census(id);
        assert!(line.contains("c_user=yes") && line.contains("xs=yes") && line.contains("saved copy"), "{line}");

    }

    /// "Why am I logged out on this machine?" has an answer in the sync log: how many cookies a
    /// bundle was made with, how many were restored here, and — the case that left a Mac logged
    /// out with nothing to show for it — when a bundle carried no login at all.
    #[test]
    fn the_sync_log_says_what_travelled_and_what_was_missing() {
        let _g = TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let a = std::env::temp_dir().join(format!("hir-loga-{}", uuid::Uuid::new_v4()));
        let b = std::env::temp_dir().join(format!("hir-logb-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let id = "trail-1";

        store::set_data_root(Some(a.clone()));
        let mut stored = crate::profile::StoredProfile::default();
        stored.meta.id = id.to_string();
        crate::profile::save_raw(&mut stored).unwrap();
        let cookie: cookies::Cookie = serde_json::from_value(serde_json::json!({
            "domain": ".facebook.com", "name": "c_user", "value": "100099", "path": "/", "expires": 1893456000.0, "secure": true
        })).unwrap();
        cookies::import(id, &[cookie]).unwrap();
        let bytes = build_bundle(id).unwrap();
        let made = log_tail(50);

        // Another machine takes it: the login is restored, and the log counts it.
        store::set_data_root(Some(b.clone()));
        let mut stored_b = crate::profile::StoredProfile::default();
        stored_b.meta.id = id.to_string();
        crate::profile::save_raw(&mut stored_b).unwrap();
        apply_bundle(id, &bytes).unwrap();
        let restored = log_tail(50);

        // A bundle from before logins could travel: nothing to restore, and the log says so.
        let mut old = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut old);
            zip.start_file("user-data/Default/Bookmarks", zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(b"x").unwrap();
            zip.finish().unwrap();
        }
        apply_bundle(id, &old.into_inner()).unwrap();
        let after_old = log_tail(50);

        store::set_data_root(None);
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);

        assert!(made.contains("portable/Cookies: 1 rows, 1 made portable, 0 dropped"), "{made}");
        assert!(restored.contains("restored Default/Network/Cookies: 1 values sealed for this machine"), "{restored}");
        assert!(!restored.contains("carries no portable cookies"), "a bundle with a login must not say it has none: {restored}");
        assert!(after_old.contains("carries no portable cookies") && after_old.contains("NOT restored"), "{after_old}");
    }

    /// An older build sent `Local State` and the sealed databases raw. Taking them would put
    /// the other machine's key over this one's, so a bundle's copies of them are ignored.
    #[test]
    fn an_older_bundles_raw_sealed_files_are_not_applied() {
        let _g = TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let a = std::env::temp_dir().join(format!("hir-oldb-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&a).unwrap();
        store::set_data_root(Some(a.clone()));
        let id = "old-1";
        let mut stored = crate::profile::StoredProfile::default();
        stored.meta.id = id.to_string();
        crate::profile::save_raw(&mut stored).unwrap();
        let udd = store::user_data_root().unwrap().join(id);
        std::fs::create_dir_all(udd.join("Default")).unwrap();
        std::fs::write(udd.join("Local State"), "MY-OWN-KEY").unwrap();
        std::fs::write(udd.join("Default/Bookmarks"), "old").unwrap();

        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("user-data/Local State", opts).unwrap();
            zip.write_all(b"THEIR-KEY").unwrap();
            zip.start_file("user-data/Default/Bookmarks", opts).unwrap();
            zip.write_all(b"new").unwrap();
            zip.finish().unwrap();
        }
        apply_bundle(id, &buf.into_inner()).unwrap();
        let key = std::fs::read_to_string(udd.join("Local State")).unwrap();
        let marks = std::fs::read_to_string(udd.join("Default/Bookmarks")).unwrap();

        store::set_data_root(None);
        let _ = std::fs::remove_dir_all(&a);
        assert_eq!(key, "MY-OWN-KEY", "this machine's key must survive a pull");
        assert_eq!(marks, "new", "ordinary files still come across");
    }

    /// Extensions ride inside the profile's own bundle: a machine that never had
    /// them installs them on pull, leaves identical ones alone, and replaces changed ones.
    #[test]
    fn extensions_travel_inside_the_profile_bundle() {
        let _g = TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let a = std::env::temp_dir().join(format!("hir-exta-{}", uuid::Uuid::new_v4()));
        let b = std::env::temp_dir().join(format!("hir-extb-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let id = "profile-ext-1";

        // Machine A: a profile that uses one extension (stored under its canonical id).
        store::set_data_root(Some(a.clone()));
        let staging = store::extensions_dir().unwrap().join("staging");
        std::fs::create_dir_all(staging.join("_locales/vi")).unwrap();
        std::fs::write(staging.join("manifest.json"), "{\"name\":\"x\",\"version\":\"1\"}").unwrap();
        std::fs::write(staging.join("_locales/vi/messages.json"), "{}").unwrap();
        let ext = crate::extensions::canonical_id(&staging).unwrap();
        let ext = ext.as_str();
        let ext_dir = store::extensions_dir().unwrap().join(ext);
        std::fs::rename(&staging, &ext_dir).unwrap();
        let mut stored = crate::profile::StoredProfile::default();
        stored.meta.id = id.to_string();
        stored.meta.extensions = vec![ext.to_string()];
        stored.config.insert("name".into(), serde_json::json!("P"));
        crate::profile::save_raw(&mut stored).unwrap();
        let bytes = build_bundle(id).unwrap();

        // Machine B: nothing installed yet.
        store::set_data_root(Some(b.clone()));
        apply_bundle(id, &bytes).unwrap();
        let dst = store::extensions_dir().unwrap().join(ext);
        let installed = dst.join("manifest.json").exists() && dst.join("_locales/vi/messages.json").exists();
        let uses = crate::profile::load_raw(id).map(|p| p.meta.extensions).unwrap_or_default();

        // Identical bundle again: untouched (the marker survives). Changed manifest: replaced.
        let stamp = || std::fs::metadata(dst.join("manifest.json")).unwrap().modified().unwrap();
        let before = stamp();
        std::thread::sleep(Duration::from_millis(1100));
        apply_bundle(id, &bytes).unwrap();
        let untouched = stamp() == before;
        store::set_data_root(Some(a.clone()));
        std::fs::write(ext_dir.join("manifest.json"), "{\"name\":\"x\",\"version\":\"2.0\"}").unwrap();
        let bytes2 = build_bundle(id).unwrap();
        store::set_data_root(Some(b.clone()));
        apply_bundle(id, &bytes2).unwrap();
        let replaced = std::fs::read_to_string(dst.join("manifest.json")).unwrap().contains("\"2.0\"");

        store::set_data_root(None);
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
        assert!(installed, "extension was not installed from the bundle");
        assert_eq!(uses, vec![ext.to_string()], "profile should keep using the extension");
        assert!(untouched, "an identical extension was needlessly rewritten");
        assert!(replaced, "a changed extension was not replaced");
    }

    /// A pulled bundle must replace whole databases, not blend into the local
    /// files, must bring the open tabs along, and must leave caches alone.
    #[test]
    fn apply_bundle_replaces_databases_and_carries_tabs() {
        let _g = TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = std::env::temp_dir().join(format!("hir-sync-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        store::set_data_root(Some(tmp.clone()));
        let id = "test-profile-1";

        // Stale local state from an earlier session on this machine.
        let udd = store::user_data_root().unwrap().join(id);
        for (rel, body) in [
            ("Default/Sessions/Tabs_OLD", "old tabs"),
            ("Default/Local Storage/leveldb/000003.ldb", "stale table"),
            ("Default/Cache/keep-me", "cache"),
        ] {
            let p = udd.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        }

        // The other machine's bundle: new tabs, a new database generation.
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::SimpleFileOptions::default();
            let mut put = |name: &str, body: &str| {
                zip.start_file(name, opts).unwrap();
                zip.write_all(body.as_bytes()).unwrap();
            };
            put("proxy.json", "null");
            put("user-data/Default/Sessions/Tabs_NEW", "youtube tab");
            put("user-data/Default/Session Storage/000010.ldb", "tab storage");
            put("user-data/Default/Local Storage/leveldb/000007.ldb", "fresh table");
            put("user-data/Default/Top Sites", "top");
            zip.finish().unwrap();
        }
        apply_bundle(id, &buf.into_inner()).expect("apply");

        let exists = |rel: &str| udd.join(rel).exists();
        let ok = exists("Default/Sessions/Tabs_NEW")
            && exists("Default/Session Storage/000010.ldb")
            && exists("Default/Local Storage/leveldb/000007.ldb")
            && exists("Default/Top Sites")
            && !exists("Default/Sessions/Tabs_OLD")
            && !exists("Default/Local Storage/leveldb/000003.ldb")
            && exists("Default/Cache/keep-me");
        store::set_data_root(None);
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(ok, "bundle was blended into stale local files or dropped the open tabs");
    }

    /// The real server on a random port, driven through the client functions:
    /// shared library, deletion tombstone, and the change-notification channel.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn server_library_tombstone_and_events() {
        let _g = TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = std::env::temp_dir().join(format!("hir-srv-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        store::set_data_root(Some(tmp.clone()));
        let token = "test-token-1234";
        let port = crate::team_server::start(0, token.to_string()).await.expect("start");
        let base = format!("http://127.0.0.1:{port}");

        // Library: upload, list, download.
        library_put(&base, token, "fingerprints", "fp1", b"zipbytes".to_vec()).await.unwrap();
        library_put(&base, token, "fingerprints", "fp0", b"{}".to_vec()).await.unwrap();
        let fp_ids = library_ids(&base, token, "fingerprints").await.unwrap();
        let got = library_get(&base, token, "fingerprints", "fp1").await.unwrap();
        // Extensions are not a server-side library any more: they ride with the profile.
        let bad_kind = library_ids(&base, token, "extensions").await.is_err();
        let bad_token = library_ids(&base, "wrong-token-000", "fingerprints").await.is_err();

        // Events: baseline, then an upload wakes a waiting client.
        let (pos, _) = wait_for_events(&base, token, None).await.unwrap();
        let waiter = {
            let (b, t) = (base.clone(), token.to_string());
            tokio::spawn(async move { wait_for_events(&b, &t, Some(pos)).await })
        };
        tokio::time::sleep(Duration::from_millis(300)).await;
        library_put(&base, token, "fingerprints", "fp2", b"more".to_vec()).await.unwrap();
        let (new_pos, ids) = tokio::time::timeout(Duration::from_secs(5), waiter).await
            .expect("waiter woke up in time").unwrap().unwrap();

        // Deletion leaves a tombstone the listing reports.
        let c = client();
        let del = c.post(format!("{base}/profiles/p-1/delete")).bearer_auth(token)
            .json(&serde_json::json!({"holder": "machine-a"})).send().await.unwrap();
        let list: serde_json::Value = c.get(format!("{base}/profiles")).bearer_auth(token)
            .send().await.unwrap().json().await.unwrap();
        let row = list["profiles"].as_array().unwrap().iter().find(|r| r["id"] == "p-1").cloned();

        let _ = crate::team_server::stop();
        store::set_data_root(None);
        let _ = std::fs::remove_dir_all(&tmp);

        assert!(fp_ids.contains("fp1") && fp_ids.contains("fp0"));
        assert_eq!(got, b"zipbytes");
        assert!(bad_kind && bad_token, "bad kind / bad token must be refused");
        assert!(new_pos > pos && ids.iter().any(|i| i == "lib:fingerprints/fp2"), "{ids:?}");
        assert!(del.status().is_success());
        let row = row.expect("tombstone listed");
        assert_eq!(row["deleted"], true);
        assert_eq!(row["updatedBy"], "machine-a");
    }

    /// A member disabled or deleted from above leaves this machine holding a
    /// token the server no longer recognises. The very next call that notices
    /// (here, `list_remote`, one of a few entry points instrumented for this)
    /// must switch sync off locally right away — not just report an error and
    /// keep quietly retrying with the same dead token forever.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_401_from_the_team_server_disables_sync_locally() {
        let _g = TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = std::env::temp_dir().join(format!("hir-kick-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        store::set_data_root(Some(tmp.clone()));

        let real_token = "server-admin-token-1234";
        let port = crate::team_server::start(0, real_token.to_string()).await.expect("start");
        let base = format!("http://127.0.0.1:{port}");

        let mut s = settings::load().unwrap();
        s.sync.enabled = true;
        s.sync.server_url = Some(base.clone());
        // A token the server has never heard of, exactly what a disabled or
        // deleted member's now-stale token looks like from this machine.
        s.sync.token = Some("a-token-the-server-does-not-know".to_string());
        settings::save(&s).unwrap();

        let err = list_remote().await;

        let after = settings::load().unwrap();

        let _ = crate::team_server::stop();
        store::set_data_root(None);
        let _ = std::fs::remove_dir_all(&tmp);

        assert!(err.is_err(), "an unrecognised token must be refused");
        assert!(!after.sync.enabled, "sync should be switched off locally once the token is rejected");
        assert_eq!(after.sync.server_url, None);
        assert_eq!(after.sync.token, None);
    }

    /// Runs `body` against a live team server, with this machine set up as
    /// "machine-a" and one profile ("sess-1") holding a login cookie.
    async fn with_synced_profile<F, Fut>(body: F)
    where
        F: FnOnce(String, String, std::path::PathBuf) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let _g = TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = std::env::temp_dir().join(format!("hir-sess-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        store::set_data_root(Some(tmp.clone()));
        // A body that stops on a failed assertion must still stop the server and put the data
        // root back, or the tests that run after it start against a server that is already up.
        struct Teardown(std::path::PathBuf);
        impl Drop for Teardown {
            fn drop(&mut self) {
                let _ = crate::team_server::stop();
                store::set_data_root(None);
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _teardown = Teardown(tmp.clone());
        let token = "session-test-token-1234".to_string();
        let port = crate::team_server::start(0, token.clone()).await.expect("start");
        let base = format!("http://127.0.0.1:{port}");
        let mut s = settings::load().unwrap();
        s.sync.enabled = true;
        s.sync.server_url = Some(base.clone());
        s.sync.token = Some(token.clone());
        s.sync.device_name = Some("machine-a".into());
        s.sync.slim_local = false;
        settings::save(&s).unwrap();
        let mut stored = crate::profile::StoredProfile::default();
        stored.meta.id = "sess-1".into();
        stored.config.insert("name".into(), serde_json::json!("Session test"));
        crate::profile::save_raw(&mut stored).unwrap();
        // Stand-in for "the login": a file that travels as it is. The real cookie and
        // password databases are sealed per machine and travel as portable copies, which
        // have their own tests (`logins_are_resealed_for_the_receiving_machine`).
        let cookies = store::user_data_root().unwrap().join("sess-1/Default/Bookmarks");
        std::fs::create_dir_all(cookies.parent().unwrap()).unwrap();
        std::fs::write(&cookies, "logged-out").unwrap();

        body(base, token, cookies).await;
    }

    /// The cookie file inside the bundle the server currently holds.
    async fn server_cookie(base: &str, token: &str) -> String {
        let bytes = client().get(format!("{base}/profiles/sess-1/bundle")).bearer_auth(token)
            .send().await.unwrap().bytes().await.unwrap();
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).unwrap();
        let mut f = z.by_name("user-data/Default/Bookmarks").unwrap();
        let mut out = String::new();
        f.read_to_string(&mut out).unwrap();
        out
    }

    /// A profile that has been logged into carries megabytes of state (local
    /// storage, IndexedDB, cookies). axum caps a request body at 2 MB unless told
    /// otherwise, so the server refused every such close with 413 — a fresh,
    /// empty profile saved fine, a logged-in one never did, and the next open
    /// pulled the old empty copy back over it: "log in, close, reopen, logged out".
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_logged_in_profile_bigger_than_two_megabytes_saves_and_comes_back() {
        with_synced_profile(|base, token, cookies| async move {
            checkout("sess-1").await.expect("open");
            // Incompressible, so the zipped bundle really is ~6 MB.
            let big: Vec<u8> = (0..400_000).flat_map(|_| *uuid::Uuid::new_v4().as_bytes()).collect();
            let idb = cookies.parent().unwrap().join("IndexedDB/https_mail.google.com_0.indexeddb.leveldb/000005.ldb");
            std::fs::create_dir_all(idb.parent().unwrap()).unwrap();
            std::fs::write(&idb, &big).unwrap();
            std::fs::write(&cookies, "logged-in").unwrap();
            assert!(build_bundle("sess-1").unwrap().len() > 3_000_000, "the bundle must really exceed axum's 2 MB default");

            checkin("sess-1").await.expect("a big logged-in profile must save on close");
            assert_eq!(server_cookie(&base, &token).await, "logged-in");

            // Wipe the local copy the way a fresh machine (or a pull) would find it,
            // then open again: everything comes back from the server.
            std::fs::remove_file(&idb).unwrap();
            std::fs::write(&cookies, "logged-out").unwrap();
            checkout("sess-1").await.expect("reopen");
            assert_eq!(std::fs::read_to_string(&cookies).unwrap(), "logged-in");
            assert_eq!(std::fs::read(&idb).unwrap(), big);
        }).await;
    }

    /// The lock a profile opens under lasts 6 hours and nothing renewed it, so a
    /// profile left open past that could not be saved on close — silently — and
    /// the next open pulled the older server copy over the fresh login. Closing
    /// now renews the lock first, so a lapsed (or forgotten) lock no longer
    /// costs the session.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn closing_saves_the_session_even_after_the_lock_lapsed() {
        with_synced_profile(|base, token, cookies| async move {
            checkout("sess-1").await.expect("open");
            std::fs::write(&cookies, "logged-in").unwrap();
            // The lock lapses / the server forgets it while the profile is open.
            unlock(&base, &token, "sess-1", "machine-a").await.unwrap();

            checkin("sess-1").await.expect("close saves despite the lost lock");

            assert_eq!(server_cookie(&base, &token).await, "logged-in");
            assert!(!load_state().pending.contains("sess-1"));
        }).await;
    }

    /// If a close cannot be saved, the next open must not replace this
    /// machine's newer copy (the login) with the older one on the server; the
    /// following successful close then saves it.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_failed_close_is_not_overwritten_by_the_next_open() {
        with_synced_profile(|base, token, cookies| async move {
            checkout("sess-1").await.expect("open");
            checkin("sess-1").await.expect("first close");
            assert_eq!(server_cookie(&base, &token).await, "logged-out");

            // Second session: log in, but another device grabs the lock before the close.
            checkout("sess-1").await.expect("reopen");
            std::fs::write(&cookies, "logged-in").unwrap();
            unlock(&base, &token, "sess-1", "machine-a").await.unwrap();
            let steal = client().post(format!("{base}/profiles/sess-1/lock")).bearer_auth(&token)
                .json(&serde_json::json!({"holder": "machine-b"})).send().await.unwrap();
            assert!(steal.status().is_success());

            assert!(checkin("sess-1").await.is_err(), "close cannot be saved while another device holds it");
            assert!(load_state().pending.contains("sess-1"));
            assert_eq!(server_cookie(&base, &token).await, "logged-out", "server still has the old copy");

            // Meanwhile someone with edit rights changes the profile's configuration on
            // the server (machine-b holds the lock, so its upload is accepted).
            let original = crate::profile::load_raw("sess-1").unwrap();
            let mut edited = original.clone();
            edited.config.insert("name".into(), serde_json::json!("Renamed by admin"));
            crate::profile::save_raw(&mut edited).unwrap();
            let edited_bundle = build_bundle("sess-1").unwrap();
            let mut restore = original.clone();
            crate::profile::save_raw(&mut restore).unwrap();
            let put = client().put(format!("{base}/profiles/sess-1/bundle")).bearer_auth(&token)
                .header("X-Sync-Holder", "machine-b").body(edited_bundle).send().await.unwrap();
            assert!(put.status().is_success(), "{}", put.status());

            // The other device lets go; this machine opens the profile again.
            unlock(&base, &token, "sess-1", "machine-b").await.unwrap();
            checkout("sess-1").await.expect("reopen again");
            assert_eq!(std::fs::read_to_string(&cookies).unwrap(), "logged-in", "the login survived the reopen");
            let cfg_name = crate::profile::load_raw("sess-1").unwrap().config.get("name").and_then(|v| v.as_str()).map(String::from);
            assert_eq!(cfg_name.as_deref(), Some("Renamed by admin"), "the configuration still follows the server");

            checkin("sess-1").await.expect("close saves now");
            assert_eq!(server_cookie(&base, &token).await, "logged-in");
            assert!(!load_state().pending.contains("sess-1"));
        }).await;
    }

    /// Opening a profile used to download the whole bundle every time, even on the machine that
    /// closed it a minute ago — seconds spent fetching what was already there. A machine whose
    /// copy is untouched since its last sync now gets "304, nothing new" and goes straight on; a
    /// copy that changed here since is still replaced by the server's, as before.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn reopening_where_it_was_closed_downloads_nothing_and_a_local_change_pulls_again() {
        with_synced_profile(|_base, _token, cookies| async move {
            checkout("sess-1").await.expect("open");
            std::fs::write(&cookies, "logged-in").unwrap();
            checkin("sess-1").await.expect("close");
            tokio::time::sleep(Duration::from_millis(30)).await;

            // Nothing changed on this machine since the close: nothing is downloaded or applied.
            checkout("sess-1").await.expect("reopen");
            let log = log_tail(40);
            assert!(log.contains("already up to date"), "{log}");
            assert_eq!(std::fs::read_to_string(&cookies).unwrap(), "logged-in");
            checkin("sess-1").await.expect("close again");
            tokio::time::sleep(Duration::from_millis(30)).await;

            // A change made here and never saved: the next open still takes the server's copy.
            std::fs::write(&cookies, "edited-and-never-saved").unwrap();
            checkout("sess-1").await.expect("reopen after a local change");
            assert_eq!(std::fs::read_to_string(&cookies).unwrap(), "logged-in", "the server's copy came back");
            let log = log_tail(40);
            assert!(log.contains("download") && log.contains("apply"), "{log}");
        }).await;
    }

    /// A server older than this build knows nothing of versions: its lock names none and its bundle
    /// carries no ETag. The client must open the profile exactly as before — whole bundle, applied.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_missing_version_means_the_whole_bundle_is_taken_as_before() {
        with_synced_profile(|base, token, cookies| async move {
            checkout("sess-1").await.expect("open");
            std::fs::write(&cookies, "logged-in").unwrap();
            checkin("sess-1").await.expect("close");
            // This machine has no record of a version it could name (what an older build's state
            // looks like): the bundle is fetched in full, with no If-None-Match.
            update_state(|st| { st.items.remove("sess-1"); });
            std::fs::write(&cookies, "logged-out").unwrap();
            checkout("sess-1").await.expect("reopen");
            assert_eq!(std::fs::read_to_string(&cookies).unwrap(), "logged-in");
            assert_eq!(server_cookie(&base, &token).await, "logged-in");
        }).await;
    }

    fn write_ext(dir: &std::path::Path, name: &str, version: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("manifest.json"), format!("{{\"name\":\"{name}\",\"version\":\"{version}\"}}")).unwrap();
    }

    fn ext_dirs() -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(store::extensions_dir().unwrap()).unwrap()
            .flatten().filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().to_string()).collect();
        v.sort();
        v
    }

    /// The same extension must exist once per machine: importing it twice,
    /// old random-id copies, and a bundle from another machine all collapse to
    /// one folder, and profiles point at that one id.
    #[test]
    fn an_extension_never_exists_twice() {
        let _g = TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = std::env::temp_dir().join(format!("hir-dup-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        store::set_data_root(Some(tmp.clone()));

        // 1. Importing the same extension twice (different source folders).
        let (s1, s2, s3) = (tmp.join("src1"), tmp.join("src2"), tmp.join("src3"));
        write_ext(&s1, "Cool Tool", "1.0");
        write_ext(&s2, "Cool Tool", "1.2");
        write_ext(&s3, "Other Tool", "3.0");
        let e1 = crate::extensions::import(&s1).unwrap();
        let e2 = crate::extensions::import(&s2).unwrap();
        let e3 = crate::extensions::import(&s3).unwrap();
        let after_imports = ext_dirs();
        let version_kept = crate::extensions::version_of(&store::extensions_dir().unwrap().join(&e2.id));

        // 2. Old-style duplicates (random ids) used by two profiles.
        let legacy_a = store::extensions_dir().unwrap().join("aaaaaaaa11111111aaaaaaaa11111111");
        let legacy_b = store::extensions_dir().unwrap().join("bbbbbbbb22222222bbbbbbbb22222222");
        write_ext(&legacy_a, "Legacy Tool", "1.0");
        write_ext(&legacy_b, "Legacy Tool", "2.0");
        for (pid, exts) in [("p-a", vec!["aaaaaaaa11111111aaaaaaaa11111111"]), ("p-b", vec!["bbbbbbbb22222222bbbbbbbb22222222", "aaaaaaaa11111111aaaaaaaa11111111"])] {
            let mut st = crate::profile::StoredProfile::default();
            st.meta.id = pid.to_string();
            st.meta.extensions = exts.into_iter().map(String::from).collect();
            st.config.insert("name".into(), serde_json::json!(pid));
            crate::profile::save_raw(&mut st).unwrap();
        }
        let merged = crate::extensions::canonicalize_all();
        let dirs = ext_dirs();
        let pa = crate::profile::load_raw("p-a").unwrap().meta.extensions;
        let pb = crate::profile::load_raw("p-b").unwrap().meta.extensions;
        let legacy_version = crate::extensions::version_of(&store::extensions_dir().unwrap().join(&pa[0]));

        // 3. A bundle from another machine that already uses the canonical id of
        //    "Legacy Tool" while this machine has another copy under a random id.
        let canon = pa[0].clone();
        let other = tmp.join("other");
        std::fs::create_dir_all(&other).unwrap();
        let mut bundle_stored = crate::profile::StoredProfile::default();
        bundle_stored.meta.id = "p-remote".into();
        bundle_stored.meta.extensions = vec![canon.clone()];
        bundle_stored.config.insert("name".into(), serde_json::json!("R"));
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("profile.json", opts).unwrap();
            zip.write_all(serde_json::to_string(&bundle_stored).unwrap().as_bytes()).unwrap();
            zip.start_file(format!("extensions/{canon}/manifest.json"), opts).unwrap();
            zip.write_all(b"{\"name\":\"Legacy Tool\",\"version\":\"1.5\"}").unwrap(); // OLDER than the 2.0 here
            zip.finish().unwrap();
        }
        apply_bundle("p-remote", &buf.into_inner()).unwrap();
        let after_bundle = ext_dirs();
        let downgrade_blocked = crate::extensions::version_of(&store::extensions_dir().unwrap().join(&canon)) == "2.0";
        let remote_uses = crate::profile::load_raw("p-remote").unwrap().meta.extensions;

        store::set_data_root(None);
        let _ = std::fs::remove_dir_all(&tmp);

        assert_eq!(e1.id, e2.id, "same extension imported twice got two ids");
        assert_ne!(e1.id, e3.id);
        assert_eq!(after_imports.len(), 2, "{after_imports:?}");
        assert_eq!(version_kept, "1.2", "the newer import should win");
        assert_eq!(merged, 2, "both legacy copies get renamed/merged");
        assert_eq!(dirs.len(), 3, "two imports + one merged Legacy Tool: {dirs:?}");
        assert_eq!(pa.len(), 1);
        assert_eq!(pb, pa, "p-b listed the extension twice under old ids; now once, same id");
        assert_eq!(legacy_version, "2.0", "the newer legacy copy wins");
        assert_eq!(after_bundle.len(), 3, "the bundle must not add a copy: {after_bundle:?}");
        assert!(downgrade_blocked, "an older bundled copy replaced a newer local one");
        assert_eq!(remote_uses, vec![canon]);
    }
}
