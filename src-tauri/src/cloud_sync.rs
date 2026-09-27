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
            for rel in trash::KEEP {
                let src = udd.join(rel);
                if src.is_dir() {
                    add_dir(&mut zip, &src, &format!("user-data/{rel}"), opts)?;
                } else if src.is_file() {
                    add_file(&mut zip, &src, &format!("user-data/{rel}"), opts)?;
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
    // Some(None) = the bundle says "no proxy"; None = an older bundle that says nothing.
    let mut proxy_in_bundle: Option<Option<crate::proxy::ProxyEntry>> = None;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i)?;
        let Some(rel) = f.enclosed_name() else { continue };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if f.is_dir() {
            continue;
        }
        let mut buf = Vec::with_capacity(f.size() as usize);
        f.read_to_end(&mut buf)?;
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
                let mut local = crate::profile::load_raw(id).unwrap_or(remote.clone());
                local.config = remote.config;
                let _ = crate::profile::save_raw(&mut local);
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
        anyhow::bail!("sync server rejected the lock request: {}", resp.status());
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
        anyhow::bail!("sync server rejected the download: {}", resp.status());
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
        })
        .collect())
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
        anyhow::bail!("lock rejected: {}", resp.status());
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
            anyhow::bail!("upload rejected: {}", put.status());
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

        if !local.contains(id) {
            // Synced before but gone here: deleted on this machine — do not bring it back.
            if known.is_some() {
                continue;
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
