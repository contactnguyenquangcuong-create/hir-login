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

use crate::{settings, store, trash};
use anyhow::{Context, Result};
use settings::SyncConfig;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

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
    /// Signature of the bound proxy then; a different one now is an unsynced change.
    #[serde(default)]
    proxy: String,
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct SyncState {
    #[serde(default)]
    items: HashMap<String, StateItem>,
}

fn state_lock() -> &'static Mutex<()> {
    static M: OnceLock<Mutex<()>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(()))
}

fn state_path() -> Result<std::path::PathBuf> {
    Ok(store::config_root()?.join("sync-state.json"))
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
            st.items.insert(id.to_string(), StateItem { remote: updated, at: unix_now(), proxy: proxy_signature(id) });
        });
    }
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
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
    let reason = resp.json::<serde_json::Value>().await.ok()
        .and_then(|v| v.get("reason").and_then(|r| r.as_str().map(String::from)))
        .unwrap_or_default();
    match (code, reason.as_str()) {
        (404, _) => anyhow::anyhow!("permission denied: no access to this profile"),
        (403, "edit") => anyhow::anyhow!("permission denied: you may use this profile but not change its settings"),
        (403, "add") => anyhow::anyhow!("permission denied: only an admin or manager can add"),
        (403, "move") => anyhow::anyhow!("permission denied: you may not move this profile to another folder"),
        (403, "delete") => anyhow::anyhow!("permission denied: you may not delete this profile"),
        (403, _) => anyhow::anyhow!("permission denied: not allowed"),
        _ => anyhow::anyhow!("sync server rejected the {what}: {code}"),
    }
}

/// A call to the team server's member/permission API with this machine's token.
pub async fn admin_call(method: &str, path: &str, body: Option<serde_json::Value>) -> Result<serde_json::Value> {
    let Some((_cfg, base, token)) = active_config()? else { anyhow::bail!("sync is not enabled") };
    let c = client();
    let url = format!("{base}{path}");
    let mut req = match method { "PUT" => c.put(url), "POST" => c.post(url), _ => c.get(url) }.bearer_auth(&token);
    if let Some(b) = body { req = req.json(&b); }
    let resp = req.send().await.context("contact sync server")?;
    if resp.status().as_u16() == 401 { anyhow::bail!("sync server rejected the request: 401"); }
    if resp.status().as_u16() == 403 {
        let reason = resp.json::<serde_json::Value>().await.ok()
            .and_then(|v| v.get("reason").and_then(|r| r.as_str().map(String::from))).unwrap_or_default();
        if reason == "folder-owner" { anyhow::bail!("permission denied: folder made by someone above"); }
        anyhow::bail!("permission denied: only the admin can manage members");
    }
    if !resp.status().is_success() { anyhow::bail!("sync server rejected the request: {}", resp.status()); }
    Ok(resp.json().await.unwrap_or(serde_json::Value::Null))
}

fn device_name(cfg: &SyncConfig) -> String {
    if let Some(n) = cfg.device_name.as_deref().filter(|s| !s.trim().is_empty()) {
        return n.to_string();
    }
    std::process::Command::new("hostname")
        .output()
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

fn synced_paths() -> impl Iterator<Item = &'static str> {
    trash::KEEP.iter().copied().chain(SYNC_EXTRA.iter().copied())
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
            for rel in synced_paths() {
                let src = udd.join(rel);
                if src.is_dir() {
                    add_dir(&mut zip, &src, &format!("user-data/{rel}"), opts)?;
                } else if src.is_file() {
                    add_file(&mut zip, &src, &format!("user-data/{rel}"), opts)?;
                }
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
        if let Some(root) = synced_paths().find(|r| sub == *r || sub.starts_with(&format!("{r}/"))) {
            roots.insert(root);
        }
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
        let Some(sub) = rel_str.strip_prefix("user-data/") else { continue };
        let out = udd.join(sub);
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(out, buf)?;
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

/// Call before spawning the browser. Locks the profile on the server (fails
/// loudly if another device holds it) and pulls its latest bundle down. A
/// no-op when sync isn't configured.
pub async fn checkout(profile_id: &str) -> Result<()> {
    let Some((cfg, base, token)) = active_config()? else { return Ok(()) };
    let _busy = begin_wait(profile_id, "pull").await;
    let holder = device_name(&cfg);
    let c = client();

    let resp = c
        .post(format!("{base}/profiles/{profile_id}/lock"))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "holder": holder }))
        .send()
        .await
        .context("contact sync server")?;

    if resp.status().as_u16() == 409 {
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        let other = body.get("holder").and_then(|v| v.as_str()).unwrap_or("another device");
        anyhow::bail!("this profile is in use by {other} — try again once they close it");
    }
    if !resp.status().is_success() {
        return Err(denied(resp, "lock request").await);
    }

    let resp = c
        .get(format!("{base}/profiles/{profile_id}/bundle"))
        .bearer_auth(&token)
        .send()
        .await
        .context("download profile bundle")?;

    if resp.status().is_success() {
        let bytes = resp.bytes().await.context("read bundle body")?;
        if let Err(e) = apply_bundle(profile_id, &bytes) {
            // Best effort: don't strand the lock on a corrupt bundle.
            let _ = unlock(&base, &token, profile_id, &holder).await;
            return Err(e.context("apply downloaded bundle"));
        }
    } else if resp.status().as_u16() != 404 {
        let _ = unlock(&base, &token, profile_id, &holder).await;
        return Err(denied(resp, "download").await);
    }
    // 404 = no remote copy yet (first time this profile syncs) — fine, the
    // local copy becomes the first version on checkin.
    mark_synced(profile_id).await;

    Ok(())
}

/// Call after the browser exits. Pushes the current bundle up and releases
/// the lock. A no-op when sync isn't configured. Best-effort: logged, never
/// fatal — the operator still has their local copy either way.
pub async fn checkin(profile_id: &str) -> Result<()> {
    let Some((cfg, base, token)) = active_config()? else { return Ok(()) };
    let _busy = begin_wait(profile_id, "push").await;
    let holder = device_name(&cfg);
    let c = client();

    let bytes = build_bundle(profile_id).context("zip profile for upload")?;
    let resp = c
        .put(format!("{base}/profiles/{profile_id}/bundle"))
        .bearer_auth(&token)
        .header("X-Sync-Holder", &holder)
        .body(bytes)
        .send()
        .await
        .context("upload profile bundle")?;
    if !resp.status().is_success() {
        anyhow::bail!("sync server rejected the upload: {}", resp.status());
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
            eprintln!("[sync] pull {} failed: {}", r.id, resp.status());
            continue;
        }
        let bytes = resp.bytes().await.context("read bundle")?;
        if let Err(e) = apply_bundle(&r.id, &bytes) {
            eprintln!("[sync] apply {} failed: {e}", r.id);
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
        .send()
        .await;
    match res {
        Ok(r) if r.status().is_success() => mark_synced(&id).await,
        Ok(r) => eprintln!("[sync] delete report {id}: {}", r.status()),
        Err(e) => eprintln!("[sync] delete report {id}: {e}"),
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
            Err(e) => eprintln!("[sync] republish {id}: {e:#}"),
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
                    eprintln!("[sync] fingerprint {id} up: {e:#}");
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
                Err(e) => eprintln!("[sync] fingerprint {id} down: {e:#}"),
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
                    Err(e) => eprintln!("[sync] trash {id}: {e:#}"),
                }
            }
            if let Some(u) = r.updated_at.clone() {
                update_state(|st| { st.items.insert(id.to_string(), StateItem { remote: u, at: unix_now(), proxy: String::new() }); });
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
            match pull_profile(&base, &token, id).await {
                Ok(true) => { changed += 1; touched.push(id.to_string()); }
                Ok(false) => {}
                Err(e) => eprintln!("[sync] pull {id}: {e:#}"),
            }
            continue;
        }

        let Some(known) = known else {
            // Existed before auto-sync: adopt the server's current version as the
            // baseline rather than overwriting anything.
            if let Some(u) = r.updated_at.clone() {
                update_state(|st| { st.items.insert(id.to_string(), StateItem { remote: u, at: unix_now(), proxy: proxy_signature(id) }); });
            }
            continue;
        };

        if r.updated_at.as_deref().is_some_and(|u| u != known.remote) {
            match pull_profile(&base, &token, id).await {
                Ok(true) => { changed += 1; touched.push(id.to_string()); }
                Ok(false) => {}
                Err(e) => eprintln!("[sync] update {id}: {e:#}"),
            }
        } else if local_edit_time(id) > known.at || proxy_signature(id) != known.proxy {
            match push_profile(&base, &token, &holder, id).await {
                Ok(true) => touched.push(id.to_string()),
                Ok(false) => {}
                Err(e) => eprintln!("[sync] push {id}: {e:#}"),
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
            Err(e) => eprintln!("[sync] first upload {id}: {e:#}"),
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
            eprintln!("[sync] round failed: {e:#}");
        }
        let mut pos = match wait_for_events(&base, &token, None).await {
            Ok((seq, _)) => seq,
            Err(e) => {
                eprintln!("[sync] cannot listen: {e:#}");
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
                                eprintln!("[sync] round failed: {e:#}");
                            }
                            last_full = std::time::Instant::now();
                        }
                    }
                    Err(e) => {
                        eprintln!("[sync] listen dropped: {e:#}");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        break; // reconnect and catch up
                    }
                },
                _ = kick_cell().notified() => {
                    if let Err(e) = sync_round().await {
                        eprintln!("[sync] round failed: {e:#}");
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
