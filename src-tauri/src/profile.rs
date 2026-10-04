use crate::store;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

/// Serialises every read-modify-write of a profile file: two interleaving — a
/// launch touching last_launched_at while the editor saves — lose one change.
fn file_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Temp file plus rename: `fs::write` truncates first, and a truncated profile
/// is unparseable, so `list_all` drops it. The retry is for Windows scanners.
fn write_atomic(path: &Path, body: &[u8]) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    {
        let mut f = fs::File::create(&tmp)
            .with_context(|| format!("create {}", tmp.display()))?;
        f.write_all(body)?;
        // Durable before the rename, or the rename can publish an empty file.
        f.sync_all()?;
    }
    let mut last = None;
    for attempt in 0..4 {
        match fs::rename(&tmp, path) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = Some(e);
                if attempt < 3 {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        }
    }
    let _ = fs::remove_file(&tmp);
    Err(anyhow::anyhow!(
        "rename {} -> {}: {}",
        tmp.display(),
        path.display(),
        last.map(|e| e.to_string()).unwrap_or_default()
    ))
}

/// Launcher-side view of a profile (wraps raw FingerprintConfig JSON).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileMeta {
    pub id: String,
    pub name: String,
    pub notes: String,
    pub proxy_id: Option<String>,
    pub last_launched_at: Option<String>,
    pub created_at: Option<String>,
    pub pinned: bool,
    pub folder: String,
    /// Accumulated runtime across every launch; UI shows this plus the
    /// current-session uptime when the profile is running.
    #[serde(default)]
    pub total_runtime_ms: u64,
    /// Icon accent, `#rrggbb`. None = derived from the name, which is what the
    /// browser does on its own.
    #[serde(default)]
    pub color: Option<String>,
    /// Extension ids from the library, loaded at launch.
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Phone/tablet fingerprint, by the core's own rule. Sync groups must not
    /// mix classes, so the bulk bar reads it.
    #[serde(default)]
    pub mobile: bool,
    /// Whether this profile answers media questions the Android way.
    #[serde(default)]
    pub android_media: bool,
}

/// On-disk `<profiles_dir>/<id>.json`: FingerprintConfig + `_meta` envelope.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoredProfile {
    #[serde(rename = "_meta", default)]
    pub meta: StoredMeta,
    /// Verbatim FingerprintConfig payload (round-trip, not parsed).
    #[serde(flatten)]
    pub config: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoredMeta {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub proxy_id: Option<String>,
    #[serde(default)]
    pub last_launched_at: Option<String>,
    /// "@<unix_secs>" creation marker.
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub pinned: bool,
    /// Empty = unfiled (All tab).
    #[serde(default)]
    pub folder: String,
    /// Cumulative engine uptime in milliseconds; bumped by the Tracker
    /// when the child exits.  Persists across launcher restarts.
    #[serde(default)]
    pub total_runtime_ms: u64,
    /// Source library fingerprint id; MUST round-trip — drives the editor GPU select.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_preset_id: Option<String>,
    /// Inline proxy from temporary profile API; not in proxy store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline_proxy: Option<crate::proxy::ProxyEntry>,
    /// Hidden from listings; auto-deleted on close.
    #[serde(default, skip_serializing_if = "is_false")]
    pub temporary: bool,
    /// Icon accent, `#rrggbb`; absent = derived from the name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Extension ids from the library.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    /// Answer media questions like a real Android device; phone profiles only.
    /// Blending in and playing video pull apart here, so the operator chooses.
    #[serde(default, skip_serializing_if = "is_false")]
    pub android_media: bool,
    /// Bumped by every write. An editor sends back the number it opened, so a
    /// save landing on top of someone else's is refused instead of silent.
    #[serde(default)]
    pub rev: u64,
}

/// The screen the profile claims, as (width, height).
pub fn claimed_screen(
    config: &serde_json::Map<String, serde_json::Value>,
) -> Option<(i64, i64)> {
    let screen = config.get("screen")?;
    let n = |k: &str| screen.get(k).and_then(|v| v.as_i64()).filter(|v| *v > 0);
    Some((n("width")?, n("height")?))
}

/// Mirrors the core's fingerprint::ProfileClaimsMobile() — same three signals
/// in the same order, so launcher and engine agree on what a profile is.
pub fn claims_mobile(config: &serde_json::Map<String, serde_json::Value>) -> bool {
    let ch = config.get("client_hints");
    if ch.and_then(|c| c.get("mobile")).and_then(|v| v.as_bool()).unwrap_or(false) {
        return true;
    }
    if ch
        .and_then(|c| c.get("platform"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .starts_with("android")
    {
        return true;
    }
    config
        .get("navigator")
        .and_then(|n| n.get("user_agent"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .contains("android")
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// The engine saves the window's last size/position into the profile's own
/// `Default/Preferences` (`browser.window_placement`) and restores it on every
/// launch — including a narrow, cornered one that got saved for whatever
/// reason (most commonly: the profile ran as a phone-shaped Android window at
/// some point, and "Đổi sang máy tính" only swaps the fingerprint, never this
/// leftover window state). Nothing else ever clears it, so the operator sees
/// the same tiny window every single launch until they notice and manually
/// resize it once Chromium does start remembering the new size — "tắt đi bật
/// lại" alone was never going to fix it, because the file never changed.
///
/// Called right before every launch; a no-op (and harmless) if the saved
/// placement is already a reasonable desktop size, or absent, or the profile
/// is genuinely a phone (where narrow is correct, not a bug).
pub fn sanitize_window_placement(user_data_dir: &std::path::Path, is_mobile: bool) {
    if is_mobile {
        return;
    }
    let path = user_data_dir.join("Default").join("Preferences");
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let Ok(mut root) = serde_json::from_str::<serde_json::Value>(&text) else { return };
    let Some(browser) = root.get_mut("browser").and_then(|b| b.as_object_mut()) else { return };
    let Some(wp) = browser.get("window_placement").and_then(|w| w.as_object()) else { return };
    let get = |k: &str| wp.get(k).and_then(|v| v.as_i64());
    let (Some(left), Some(top), Some(right), Some(bottom)) = (get("left"), get("top"), get("right"), get("bottom")) else { return };
    const MIN_DESKTOP_WIDTH: i64 = 600;
    const MIN_DESKTOP_HEIGHT: i64 = 400;
    if right - left >= MIN_DESKTOP_WIDTH && bottom - top >= MIN_DESKTOP_HEIGHT {
        return; // already a sane size — leave the operator's own resize alone
    }
    browser.remove("window_placement");
    if let Ok(out) = serde_json::to_string(&root) {
        let _ = std::fs::write(&path, out);
    }
}

fn path_for(id: &str) -> Result<PathBuf> {
    if id.contains(['/', '\\', '.']) {
        anyhow::bail!("invalid profile id");
    }
    Ok(store::profiles_dir()?.join(format!("{id}.json")))
}

pub fn list_all() -> Result<Vec<ProfileMeta>> {
    let dir = store::profiles_dir()?;
    let mut out = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.path().extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let path = entry.path();
        // One unreadable file must not fail the whole listing: `?` here made a
        // single momentarily-locked profile (Windows, mid-write by another
        // thread) return an error and the whole table read as empty. Retry
        // once, then skip just that profile.
        let body = match fs::read_to_string(&path) {
            Ok(b) => b,
            Err(_) => {
                std::thread::sleep(std::time::Duration::from_millis(60));
                match fs::read_to_string(&path) {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("[launcher] profile {} unreadable, skipped: {e}", path.display());
                        continue;
                    }
                }
            }
        };
        let Ok(mut stored): std::result::Result<StoredProfile, _> = serde_json::from_str(&body) else {
            eprintln!("[launcher] profile {} is not valid JSON, skipped", path.display());
            continue;
        };
        // Hide ephemeral profiles.
        if stored.meta.temporary {
            continue;
        }
        // Backfill legacy profiles' created_at from file mtime, then persist.
        if stored.meta.created_at.is_none() {
            let mtime = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| format!("@{}", d.as_secs()));
            if let Some(ts) = mtime {
                stored.meta.created_at = Some(ts);
                if let Ok(body) = serde_json::to_string_pretty(&stored) {
                    let _ = write_atomic(&path, body.as_bytes());
                }
            }
        }
        let name = stored
            .config
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("(unnamed)")
            .to_string();
        let notes = stored
            .config
            .get("notes")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        out.push(ProfileMeta {
            id: stored.meta.id,
            name,
            notes,
            proxy_id: stored.meta.proxy_id,
            last_launched_at: stored.meta.last_launched_at,
            created_at: stored.meta.created_at,
            pinned: stored.meta.pinned,
            folder: stored.meta.folder,
            total_runtime_ms: stored.meta.total_runtime_ms,
            color: stored.meta.color,
            extensions: stored.meta.extensions,
            mobile: claims_mobile(&stored.config),
            android_media: stored.meta.android_media,
        });
    }
    // Pinned first, then newest-first by created_at; name fallback for same-second ties.
    out.sort_by(|a, b| {
        match (a.pinned, b.pinned) {
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            _ => {}
        }
        match (&b.created_at, &a.created_at) {
            (Some(bv), Some(av)) => bv.cmp(av),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.name.cmp(&b.name),
        }
    });
    Ok(out)
}

/// Delete leftover temporary profiles after a crash; returns count.
pub fn purge_temporary() -> Result<usize> {
    let dir = store::profiles_dir()?;
    let mut n = 0;
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.path().extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let Ok(body) = fs::read_to_string(entry.path()) else { continue; };
        let Ok(stored): std::result::Result<StoredProfile, _> = serde_json::from_str(&body) else {
            continue;
        };
        if stored.meta.temporary && !stored.meta.id.is_empty() {
            let _ = delete(&stored.meta.id);
            n += 1;
        }
    }
    Ok(n)
}

pub fn load_raw(id: &str) -> Result<StoredProfile> {
    let path = path_for(id)?;
    let body = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let stored: StoredProfile = serde_json::from_str(&body)?;
    Ok(stored)
}

/// Deterministic non-zero 32-bit seed from the profile id + noise slot (FNV-1a).
/// Same id + slot always yields the same seed (stable fingerprint across
/// launches/edits); different ids yield different seeds (unique per profile).
fn derive_noise_seed(id: &str, slot: &str) -> u32 {
    let s = format!("{id}::{slot}");
    let mut h: u32 = 2166136261;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    // 0 is the "derive automatically" sentinel — never hand it back as a value.
    if h == 0 {
        1
    } else {
        h
    }
}

/// Replace every auto-sentinel noise seed (`seed == 0` or absent) with a
/// stable per-profile value derived from the final profile id.  The UI can't
/// know the id at create time, so it sends `seed: 0` for every vector; without
/// this every freshly-created profile would otherwise share one placeholder
/// seed and produce an identical canvas/audio/WebGL fingerprint.
fn fill_noise_seeds(config: &mut serde_json::Map<String, serde_json::Value>, id: &str) {
    let Some(noise) = config.get_mut("noise").and_then(|n| n.as_object_mut()) else {
        return;
    };
    for (slot, block) in noise.iter_mut() {
        let Some(obj) = block.as_object_mut() else {
            continue;
        };
        let needs = obj
            .get("seed")
            .and_then(|v| v.as_u64())
            .map(|n| n == 0)
            .unwrap_or(true);
        if needs {
            obj.insert("seed".into(), serde_json::Value::from(derive_noise_seed(id, slot)));
        }
    }
}

/// Reset every noise seed back to the auto sentinel so the next `save_raw`
/// re-derives them from a fresh id.  Used when cloning so the copy doesn't
/// inherit the source's canvas/audio/WebGL fingerprint.
fn clear_noise_seeds(config: &mut serde_json::Map<String, serde_json::Value>) {
    let Some(noise) = config.get_mut("noise").and_then(|n| n.as_object_mut()) else {
        return;
    };
    for (_, block) in noise.iter_mut() {
        if let Some(obj) = block.as_object_mut() {
            obj.insert("seed".into(), serde_json::Value::from(0u32));
        }
    }
}

/// The `rev` currently on disk, or 0 when there is no such profile yet.
pub fn current_rev(id: &str) -> u64 {
    load_raw(id).map(|p| p.meta.rev).unwrap_or(0)
}

pub fn save_raw(stored: &mut StoredProfile) -> Result<()> {
    let _guard = file_lock();
    save_raw_locked(stored)
}

/// Body of `save_raw` for callers already holding the lock. It reloads the file
/// itself, so a caller that loaded first must hold the lock across both halves.
fn save_raw_locked(stored: &mut StoredProfile) -> Result<()> {
    let is_new = stored.meta.id.is_empty();
    if is_new {
        stored.meta.id = uuid::Uuid::new_v4().to_string();
    }
    // Carry created_at/pinned/folder/last_launched_at through edits.
    // pinned and folder are owned by set_pin/set_folder respectively.
    if !is_new {
        if let Ok(existing) = load_raw(&stored.meta.id) {
            if stored.meta.created_at.is_none() {
                stored.meta.created_at = existing.meta.created_at;
            }
            stored.meta.pinned = existing.meta.pinned;
            if stored.meta.folder.is_empty() {
                stored.meta.folder = existing.meta.folder;
            }
            if stored.meta.last_launched_at.is_none() {
                stored.meta.last_launched_at = existing.meta.last_launched_at;
            }
            // total_runtime_ms is owned by the Tracker — every save (edit /
            // proxy bind / folder move) carries the existing counter through.
            if stored.meta.total_runtime_ms == 0 {
                stored.meta.total_runtime_ms = existing.meta.total_runtime_ms;
            }
            // Only a change to the profile's own content moves the counter.
            // Launching a profile writes last_launched_at, and closing it
            // writes the runtime total — neither is something an open editor
            // is in conflict with, and bumping for them refused the operator's
            // own save with a message blaming an API nobody had called.
            stored.meta.rev = if stored.config == existing.config {
                existing.meta.rev
            } else {
                existing.meta.rev.wrapping_add(1)
            };
        }
    }
    if is_new {
        stored.meta.rev = 1;
    }
    if stored.meta.created_at.is_none() {
        stored.meta.created_at = Some(chrono_now_iso());
    }
    // The id is now final (freshly minted for new profiles, carried through for
    // edits) — derive per-profile noise seeds from it so each profile gets a
    // unique-but-stable fingerprint instead of sharing the UI's placeholder.
    fill_noise_seeds(&mut stored.config, &stored.meta.id);
    let path = path_for(&stored.meta.id)?;
    let body = serde_json::to_string_pretty(stored)?;
    write_atomic(&path, body.as_bytes())?;
    // A change made here (edit, pin, folder, colour) should reach the team now.
    crate::cloud_sync::kick();
    Ok(())
}

/// Copies one file, tolerating what a *running* browser does to it: Windows'
/// `CopyFileEx` refuses a file another process holds open, while a plain read
/// (std opens with every share flag) usually gets through.
fn copy_file_tolerant(src: &Path, dst: &Path) -> Result<()> {
    if fs::copy(src, dst).is_ok() {
        return Ok(());
    }
    let mut from = fs::File::open(src).with_context(|| format!("read {}", src.display()))?;
    let mut to = fs::File::create(dst).with_context(|| format!("write {}", dst.display()))?;
    std::io::copy(&mut from, &mut to).with_context(|| format!("copy {}", src.display()))?;
    Ok(())
}

/// Recursive copy of regular files and folders only. A Chromium user-data dir
/// also holds sockets and symlinks (`SingletonLock`/`SingletonSocket`/
/// `SingletonCookie`, left dangling by a crash or force-quit) that cannot be
/// copied and carry nothing worth keeping — they are skipped, not fatal, and so
/// are the `LOCK` files LevelDB holds while a profile runs (re-created on
/// open). Anything else that fails *is* an error: a bundle silently missing
/// half a database is worse than one reported as failed.
fn copy_tree_tolerant(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ft.is_dir() {
            copy_tree_tolerant(&entry.path(), &to)?;
        } else if ft.is_file() {
            if entry.file_name() == "LOCK" {
                continue;
            }
            copy_file_tolerant(&entry.path(), &to)?;
        }
    }
    Ok(())
}

/// Folder name for one exported profile: the profile's own name where the
/// filesystem allows it (Vietnamese letters stay readable instead of turning
/// into underscores), made safe for Windows, capped in length (a long name
/// plus Chromium's deep cache paths is how a copy hits the 260-character limit)
/// and deduped against what this batch already used.
fn dedup_folder_name(name: &str, used: &mut std::collections::HashSet<String>) -> String {
    let base: String = name
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') { '_' } else { c }
        })
        .take(60)
        .collect();
    // Windows silently drops trailing dots and spaces, which would make two
    // names collide on disk while looking distinct here.
    let base = base.trim().trim_end_matches('.').trim().to_string();
    let reserved = ["CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "LPT1", "LPT2", "LPT3"];
    let base = if base.is_empty() || reserved.contains(&base.to_ascii_uppercase().as_str()) {
        "profile".to_string()
    } else {
        base
    };
    let mut candidate = base.clone();
    let mut n = 2;
    while used.contains(&candidate.to_lowercase()) {
        candidate = format!("{base} ({n})");
        n += 1;
    }
    used.insert(candidate.to_lowercase());
    candidate
}

/// One profile a bundle operation could not do, and why.
#[derive(Debug, Clone, Serialize)]
pub struct BundleFailure {
    pub name: String,
    pub error: String,
}

#[derive(Debug, Default, Serialize)]
pub struct ExportReport {
    pub exported: usize,
    pub failed: Vec<BundleFailure>,
    /// Exported, but worth a look (e.g. the profile was running at the time).
    pub warnings: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct ImportReport {
    pub imported: usize,
    pub failed: Vec<BundleFailure>,
    pub warnings: Vec<String>,
}

/// Export one profile into `out_dir`: `profile.json`, `proxy.json` (the bound
/// proxy travels with it, or the profile would arrive on another machine
/// pointing at a proxy id that machine has never heard of) and `userdata/`
/// holding only the account-carrying files — the same set the trash and team
/// sync keep, never the cache.
fn export_one(id: &str, stored: &StoredProfile, out_dir: &Path) -> Result<()> {
    fs::create_dir_all(out_dir)?;
    fs::write(out_dir.join("profile.json"), serde_json::to_string_pretty(stored)?)?;
    let proxy = stored
        .meta
        .proxy_id
        .as_deref()
        .and_then(|pid| crate::proxy::get(pid).ok().flatten());
    fs::write(out_dir.join("proxy.json"), serde_json::to_string(&proxy)?)?;
    let udd = store::user_data_root()?.join(id);
    if udd.exists() {
        let dst = out_dir.join("userdata");
        for rel in crate::cloud_sync::synced_paths() {
            let src = udd.join(rel);
            let to = dst.join(rel);
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)?;
            }
            if src.is_dir() {
                copy_tree_tolerant(&src, &to)?;
            } else if src.is_file() {
                copy_file_tolerant(&src, &to)?;
            }
        }
    }
    Ok(())
}

/// Export profiles as a portable bundle: one subfolder per profile under
/// `dest`, each holding what `import_bundle` expects back, so a folder of
/// these can be carried to another machine and re-imported whole.
///
/// One profile that cannot be exported — a broken file, something locked by a
/// running browser — is reported and skipped, never allowed to stop the rest:
/// exporting 72 used to end at the first bad one with a bare error.
pub fn export_bundle(ids: &[String], dest: &Path) -> Result<ExportReport> {
    fs::create_dir_all(dest)?;
    let mut used = std::collections::HashSet::new();
    let mut report = ExportReport::default();
    for id in ids {
        let stored = match load_raw(id) {
            Ok(s) => s,
            Err(e) => {
                report.failed.push(BundleFailure { name: id.clone(), error: format!("{e:#}") });
                continue;
            }
        };
        let name = stored
            .config
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(id.as_str())
            .to_string();
        let out_dir = dest.join(dedup_folder_name(&name, &mut used));
        match export_one(id, &stored, &out_dir) {
            Ok(()) => {
                report.exported += 1;
                if crate::process::Tracker::shared().is_running(id) {
                    report.warnings.push(format!(
                        "{name}: đang chạy lúc xuất — phiên đăng nhập có thể chưa lưu hết, nên tắt profile rồi xuất lại"
                    ));
                }
            }
            Err(e) => {
                // No half-written folder left behind for an import to trip over.
                let _ = fs::remove_dir_all(&out_dir);
                report.failed.push(BundleFailure { name, error: format!("{e:#}") });
            }
        }
    }
    Ok(report)
}

/// True if `path` is itself the top of one Chromium profile's data (a
/// `user-data-dir`, or a single profile folder inside one) rather than a
/// folder that merely holds several such folders as children. Without this
/// check, pointing the picker straight at one real Chrome profile would make
/// its own internal folders (`Default`, `Profile 1`, `Local State`'s
/// sibling dirs) each get imported as if they were separate profiles.
fn looks_like_browser_profile_dir(path: &Path) -> bool {
    path.join("Local State").is_file()
        || path.join("Default").is_dir()
        || path.join("Preferences").is_file()
}

/// Import one profile from `dir`: `dir/profile.json` (written by
/// `export_bundle`) round-trips its config and `dir/userdata/` verbatim; a
/// bare browser-profile folder is adopted as the user-data itself and given
/// a random fingerprint from the library, named after the folder.
///
/// All or nothing: if the browser data cannot be copied the profile that was
/// just created for it is removed again, instead of leaving a profile that
/// opens empty and logged out.
fn import_one_profile_dir(dir: &Path, warnings: &mut Vec<String>) -> Result<()> {
    let folder_name = dir
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "profile".into());
    let profile_json = dir.join("profile.json");

    let mut stored = if profile_json.exists() {
        let body = fs::read_to_string(&profile_json)?;
        serde_json::from_str::<StoredProfile>(&body)
            .with_context(|| format!("{}: invalid profile.json", dir.display()))?
    } else {
        let library = crate::fingerprints::list_all()?;
        // Desktop systems only: a phone fingerprint opens as a narrow phone-sized
        // window that cannot be widened (falls back to all if there is no desktop one).
        let desktop: Vec<_> = library.iter().filter(|f| matches!(f.platform.as_str(), "Windows" | "macOS" | "Linux")).collect();
        let pool: Vec<_> = if desktop.is_empty() { library.iter().collect() } else { desktop };
        let base = if pool.is_empty() {
            serde_json::json!({})
        } else {
            let pick = uuid::Uuid::new_v4().as_bytes()[0] as usize % pool.len();
            crate::fingerprints::get(&pool[pick].id)?
                .map(|e| e.payload)
                .unwrap_or_else(|| serde_json::json!({}))
        };
        let mut config = base.as_object().cloned().unwrap_or_default();
        config.insert("name".into(), serde_json::Value::String(folder_name.clone()));
        StoredProfile { meta: StoredMeta::default(), config }
    };
    let name = stored
        .config
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(folder_name.as_str())
        .to_string();

    // The proxy: bring it across when the bundle carries it (deduped, so 72
    // profiles sharing one proxy make one entry, not 72); with none in the
    // bundle (older export), a binding to an id this machine does not know
    // would launch the profile *direct* without a word — better shown as
    // "no proxy" so it is noticed.
    let bundled = fs::read_to_string(dir.join("proxy.json"))
        .ok()
        .and_then(|b| serde_json::from_str::<Option<crate::proxy::ProxyEntry>>(&b).ok());
    match bundled {
        Some(Some(entry)) => {
            stored.meta.proxy_id = Some(crate::proxy::upsert_dedup(entry)?.id);
        }
        Some(None) => stored.meta.proxy_id = None,
        None => {
            if let Some(pid) = stored.meta.proxy_id.clone() {
                if crate::proxy::get(&pid).ok().flatten().is_none() {
                    stored.meta.proxy_id = None;
                    warnings.push(format!("{name}: dùng proxy chưa có trên máy này — hãy gán lại proxy trước khi mở"));
                }
            }
        }
    }

    let _guard = file_lock();
    let new_id = uuid::Uuid::new_v4().to_string();
    stored.meta.id = new_id.clone();
    stored.meta.last_launched_at = None;
    stored.meta.created_at = None;
    stored.meta.pinned = false;
    stored.meta.rev = 0;
    save_raw_locked(&mut stored)?;
    drop(_guard);

    let src_userdata = if profile_json.exists() { dir.join("userdata") } else { dir.to_path_buf() };
    if src_userdata.exists() {
        let copied = user_data_dir(&new_id).and_then(|dst| copy_tree_tolerant(&src_userdata, &dst));
        if let Err(e) = copied {
            let _ = delete(&new_id);
            return Err(e);
        }
    }
    Ok(())
}

/// Import a portable bundle directory. If `src` is itself one profile's data
/// (a bundle with `profile.json`, or a real Chromium profile folder), it is
/// imported as that single profile. Otherwise every immediate subfolder of
/// `src` is imported as one profile each — see `import_one_profile_dir`.
/// One that fails is reported and skipped; the rest still come in.
pub fn import_bundle(src: &Path) -> Result<ImportReport> {
    let mut report = ImportReport::default();
    if src.join("profile.json").exists() || looks_like_browser_profile_dir(src) {
        let name = src.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        match import_one_profile_dir(src, &mut report.warnings) {
            Ok(()) => report.imported = 1,
            Err(e) => report.failed.push(BundleFailure { name, error: format!("{e:#}") }),
        }
        return Ok(report);
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            dirs.push(entry.path());
        }
    }
    dirs.sort();
    for subdir in dirs {
        if !subdir.join("profile.json").exists() && !looks_like_browser_profile_dir(&subdir) {
            continue;
        }
        let name = subdir.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        match import_one_profile_dir(&subdir, &mut report.warnings) {
            Ok(()) => report.imported += 1,
            Err(e) => report.failed.push(BundleFailure { name, error: format!("{e:#}") }),
        }
    }
    Ok(report)
}

pub fn delete(id: &str) -> Result<()> {
    let path = path_for(id)?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    // Also wipe per-profile user-data-dir.
    let udd = store::user_data_root()?.join(id);
    if udd.exists() {
        let _ = fs::remove_dir_all(udd);
    }
    Ok(())
}

/// Add `ms` to the persisted total_runtime_ms counter.  Called by the
/// process Tracker when the engine exits — totals survive launcher restarts.
pub fn add_runtime(id: &str, ms: u64) -> Result<()> {
    let _guard = file_lock();
    let mut p = load_raw(id)?;
    p.meta.total_runtime_ms = p.meta.total_runtime_ms.saturating_add(ms);
    save_raw_locked(&mut p)?;
    Ok(())
}

/// Touch last_launched_at; optionally switch bound proxy.
pub fn touch_launched(id: &str, proxy_id: Option<String>) -> Result<()> {
    let _guard = file_lock();
    let mut p = load_raw(id)?;
    p.meta.last_launched_at = Some(chrono_now_iso());
    if proxy_id.is_some() {
        p.meta.proxy_id = proxy_id;
    }
    save_raw_locked(&mut p)?;
    Ok(())
}

pub fn clone_profile(id: &str) -> Result<ProfileMeta> {
    let _guard = file_lock();
    let mut src = load_raw(id)?;
    let new_id = uuid::Uuid::new_v4().to_string();
    let old_name = src
        .config
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("profile")
        .to_string();
    src.meta.id = new_id.clone();
    src.meta.last_launched_at = None;
    src.meta.created_at = None;
    src.meta.pinned = false;
    src.config
        .insert("name".into(), serde_json::Value::String(format!("{old_name} (copy)")));
    // Re-randomize CPU/RAM/platform_version so the copy doesn't collide on those axes.
    crate::randomize_platform_version(&mut src.config);
    crate::randomize_hardware(&mut src.config);
    // Same reasoning for the fingerprint noise: drop the source's seeds so
    // save_raw re-derives fresh ones from new_id, giving the copy its own
    // canvas/audio/WebGL fingerprint instead of a clone of the original's.
    clear_noise_seeds(&mut src.config);
    save_raw_locked(&mut src)?;
    Ok(ProfileMeta {
        id: src.meta.id,
        name: format!("{old_name} (copy)"),
        notes: src
            .config
            .get("notes")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        proxy_id: src.meta.proxy_id,
        last_launched_at: None,
        created_at: src.meta.created_at,
        pinned: false,
        folder: src.meta.folder,
        total_runtime_ms: 0,
        color: src.meta.color,
        extensions: src.meta.extensions,
        mobile: claims_mobile(&src.config),
        android_media: src.meta.android_media,
    })
}

/// Flip pin flag.
pub fn set_pin(id: &str, pinned: bool) -> Result<()> {
    let _guard = file_lock();
    let mut p = load_raw(id)?;
    p.meta.pinned = pinned;
    let path = path_for(&p.meta.id)?;
    let body = serde_json::to_string_pretty(&p)?;
    write_atomic(&path, body.as_bytes())?;
    Ok(())
}

/// Assign folder tag (empty string clears).
pub fn set_folder(id: &str, folder: &str) -> Result<()> {
    let _guard = file_lock();
    let mut p = load_raw(id)?;
    p.meta.folder = folder.trim().to_string();
    let path = path_for(&p.meta.id)?;
    let body = serde_json::to_string_pretty(&p)?;
    write_atomic(&path, body.as_bytes())?;
    Ok(())
}

/// Turns one library extension on for every profile that does not have it yet
/// (temporary profiles are left alone — they live for one run). Returns how
/// many profiles changed. Like pin and folder this is the machine's own
/// choice, so it is written without bumping the sync revision.
pub fn add_extension_to_all(ext_id: &str) -> Result<usize> {
    let _guard = file_lock();
    let dir = store::profiles_dir()?;
    let mut n = 0;
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.path().extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let Ok(body) = fs::read_to_string(entry.path()) else { continue };
        let Ok(mut stored): std::result::Result<StoredProfile, _> = serde_json::from_str(&body)
        else {
            continue;
        };
        if stored.meta.temporary || stored.meta.extensions.iter().any(|e| e == ext_id) {
            continue;
        }
        stored.meta.extensions.push(ext_id.to_string());
        let out = serde_json::to_string_pretty(&stored)?;
        write_atomic(&entry.path(), out.as_bytes())?;
        n += 1;
    }
    Ok(n)
}

/// Retag profiles from folder `old` to `new`; returns count.
pub fn rename_folder(old: &str, new: &str) -> Result<usize> {
    let _guard = file_lock();
    let dir = store::profiles_dir()?;
    let new = new.trim();
    let mut n = 0;
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.path().extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let Ok(body) = fs::read_to_string(entry.path()) else { continue; };
        let Ok(mut stored): std::result::Result<StoredProfile, _> = serde_json::from_str(&body)
        else {
            continue;
        };
        if stored.meta.folder == old {
            stored.meta.folder = new.to_string();
            if let Ok(out) = serde_json::to_string_pretty(&stored) {
                let _ = write_atomic(&entry.path(), out.as_bytes());
            }
            n += 1;
        }
    }
    Ok(n)
}

/// Delete folder; `delete_profiles` true removes, false unfiles. Returns count.
pub fn delete_folder(name: &str, delete_profiles: bool) -> Result<usize> {
    let _guard = file_lock();
    let dir = store::profiles_dir()?;
    let mut n = 0;
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.path().extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let Ok(body) = fs::read_to_string(entry.path()) else { continue; };
        let Ok(mut stored): std::result::Result<StoredProfile, _> = serde_json::from_str(&body)
        else {
            continue;
        };
        if stored.meta.folder == name {
            if delete_profiles {
                // Through the trash, like every other delete — a folder wiped
                // by mistake is exactly the case the week is there for.
                if crate::trash::move_to_trash(&stored.meta.id).is_err() {
                    let _ = delete(&stored.meta.id);
                }
            } else {
                stored.meta.folder = String::new();
                if let Ok(out) = serde_json::to_string_pretty(&stored) {
                    let _ = write_atomic(&entry.path(), out.as_bytes());
                }
            }
            n += 1;
        }
    }
    Ok(n)
}

/// Per-profile user-data-dir; created on first call.
pub fn user_data_dir(id: &str) -> Result<PathBuf> {
    if id.contains(['/', '\\', '.']) {
        anyhow::bail!("invalid profile id");
    }
    let p = store::user_data_root()?.join(id);
    std::fs::create_dir_all(&p)?;
    Ok(p)
}

fn chrono_now_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let s = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("@{s}")
}

#[cfg(test)]
mod window_placement_tests {
    use super::*;
    use std::io::Write;

    fn prefs_with(window_placement: serde_json::Value) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("hir-wp-{}", uuid::Uuid::new_v4()));
        let default_dir = dir.join("Default");
        std::fs::create_dir_all(&default_dir).unwrap();
        let path = default_dir.join("Preferences");
        let body = serde_json::json!({"browser": {"window_placement": window_placement, "other_setting": true}});
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(body.to_string().as_bytes()).unwrap();
        (dir, path)
    }

    fn placement_of(path: &std::path::Path) -> Option<serde_json::Value> {
        let text = std::fs::read_to_string(path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        v["browser"].get("window_placement").cloned()
    }

    /// A narrow, phone-shaped window (the exact Android dimensions) left over
    /// on a desktop profile is cleared, so the engine falls back to its own
    /// sane default on the next launch instead of restoring the old one again.
    #[test]
    fn a_narrow_leftover_window_is_cleared_on_a_desktop_profile() {
        let (dir, path) = prefs_with(serde_json::json!({"left": 609, "top": 44, "right": 969, "bottom": 850, "maximized": false}));
        sanitize_window_placement(&dir, false);
        assert_eq!(placement_of(&path), None, "the narrow placement must be gone");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("other_setting"), "nothing else in Preferences should be touched");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A reasonably sized window — the operator's own deliberate resize — is
    /// never second-guessed.
    #[test]
    fn a_normal_sized_window_is_left_alone() {
        let (dir, path) = prefs_with(serde_json::json!({"left": 100, "top": 100, "right": 1380, "bottom": 900, "maximized": false}));
        sanitize_window_placement(&dir, false);
        assert!(placement_of(&path).is_some(), "a normal-sized window must survive");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The same narrow size is correct, not a bug, on a profile that really is
    /// a phone — never touched there.
    #[test]
    fn a_narrow_window_is_left_alone_on_a_mobile_profile() {
        let (dir, path) = prefs_with(serde_json::json!({"left": 609, "top": 44, "right": 969, "bottom": 850, "maximized": false}));
        sanitize_window_placement(&dir, true);
        assert!(placement_of(&path).is_some(), "an Android profile's phone-sized window is intentional");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// No Preferences file yet (profile never launched) — nothing to crash on.
    #[test]
    fn a_missing_preferences_file_is_a_quiet_no_op() {
        let dir = std::env::temp_dir().join(format!("hir-wp-missing-{}", uuid::Uuid::new_v4()));
        sanitize_window_placement(&dir, false); // must not panic
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Absent from the file when off, so old profiles do not grow a line.
    #[test]
    fn android_media_round_trips_and_stays_absent_when_off() {
        let mut meta = StoredMeta::default();
        assert!(!meta.android_media);
        let off = serde_json::to_string(&meta).unwrap();
        assert!(!off.contains("android_media"), "{off}");

        meta.android_media = true;
        let on = serde_json::to_string(&meta).unwrap();
        assert!(on.contains("\"android_media\":true"), "{on}");

        let back: StoredMeta = serde_json::from_str(&on).unwrap();
        assert!(back.android_media);

        // A profile written before the setting existed reads as off.
        let old: StoredMeta = serde_json::from_str("{}").unwrap();
        assert!(!old.android_media);
    }

    /// The launcher's rule and the core's rule must agree about what a phone is.
    #[test]
    fn claims_mobile_matches_the_cores_three_signals() {
        let m = |json: &str| -> bool {
            let v: serde_json::Value = serde_json::from_str(json).unwrap();
            claims_mobile(v.as_object().unwrap())
        };
        assert!(m(r#"{"client_hints":{"mobile":true}}"#));
        assert!(m(r#"{"client_hints":{"platform":"Android"}}"#));
        assert!(m(r#"{"navigator":{"user_agent":"Mozilla/5.0 (Linux; Android 15) Chrome"}}"#));
        assert!(!m(r#"{"navigator":{"user_agent":"Mozilla/5.0 (Macintosh) Chrome"}}"#));
        // navigator.platform is the field that lies: Chrome for Android says
        // "Linux armv8l" there, and a desktop Linux profile says "Linux x86_64".
        assert!(!m(r#"{"navigator":{"platform":"Linux armv8l"}}"#));
    }
}

#[cfg(test)]
mod bundle_tests {
    use super::*;

    /// A throwaway data root for one test (and the lock that keeps tests that
    /// share the process-wide root from interleaving).
    struct Root {
        dir: PathBuf,
        _g: MutexGuard<'static, ()>,
    }
    impl Root {
        fn new(tag: &str) -> Root {
            let g = crate::cloud_sync::TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let dir = std::env::temp_dir().join(format!("hir-{tag}-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&dir).unwrap();
            crate::store::set_data_root(Some(dir.clone()));
            Root { dir, _g: g }
        }
        /// Point at a second machine's data (the bundle stays where it is).
        fn switch_to(&self, other: &str) {
            let d = self.dir.join(other);
            fs::create_dir_all(&d).unwrap();
            crate::store::set_data_root(Some(d));
        }
    }
    impl Drop for Root {
        fn drop(&mut self) {
            crate::store::set_data_root(None);
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn one_extension_can_be_turned_on_for_every_profile() {
        let _root = Root::new("ext-all");
        let a = make("a", None);
        let b = make("b", None);
        let mut sb = load_raw(&b).unwrap();
        sb.meta.extensions = vec!["ext1".into()];
        save_raw(&mut sb).unwrap();
        let mut tmp = StoredProfile { meta: StoredMeta::default(), config: serde_json::Map::new() };
        tmp.meta.temporary = true;
        save_raw(&mut tmp).unwrap();

        assert_eq!(add_extension_to_all("ext1").unwrap(), 1, "b already had it");
        assert_eq!(load_raw(&a).unwrap().meta.extensions, vec!["ext1".to_string()]);
        assert_eq!(load_raw(&b).unwrap().meta.extensions, vec!["ext1".to_string()], "not doubled");
        assert!(load_raw(&tmp.meta.id).unwrap().meta.extensions.is_empty(), "temporary profile untouched");
        assert_eq!(add_extension_to_all("ext1").unwrap(), 0, "second call changes nothing");
        assert_eq!(add_extension_to_all("ext2").unwrap(), 2);
        assert_eq!(load_raw(&a).unwrap().meta.extensions, vec!["ext1".to_string(), "ext2".to_string()]);
    }

    /// A profile with the usual account files plus the cache a real browser
    /// leaves, returning its id.
    fn make(name: &str, proxy: Option<&crate::proxy::ProxyEntry>) -> String {
        let mut cfg = serde_json::Map::new();
        cfg.insert("name".into(), serde_json::Value::String(name.into()));
        let mut sp = StoredProfile { meta: StoredMeta::default(), config: cfg };
        sp.meta.proxy_id = proxy.map(|p| p.id.clone());
        save_raw(&mut sp).unwrap();
        let udd = user_data_dir(&sp.meta.id).unwrap();
        fs::create_dir_all(udd.join("Default/Local Storage/leveldb")).unwrap();
        fs::write(udd.join("Default/Cookies"), format!("cookies-of-{name}")).unwrap();
        fs::write(udd.join("Local State"), b"{}").unwrap();
        fs::write(udd.join("Default/Local Storage/leveldb/000003.log"), b"ls").unwrap();
        fs::write(udd.join("Default/Local Storage/leveldb/LOCK"), b"").unwrap();
        // The bulk of a real profile — must not travel.
        fs::create_dir_all(udd.join("Default/Cache/Cache_Data")).unwrap();
        fs::write(udd.join("Default/Cache/Cache_Data/f_000001"), vec![0u8; 4096]).unwrap();
        fs::create_dir_all(udd.join("GrShaderCache")).unwrap();
        fs::write(udd.join("GrShaderCache/data_0"), vec![0u8; 4096]).unwrap();
        sp.meta.id.clone()
    }

    fn a_proxy() -> crate::proxy::ProxyEntry {
        let e = crate::proxy::parse_single_with_kind("1.2.3.4:1080:user01:c2VjcmV0", crate::proxy::ProxyKind::Socks5).unwrap();
        crate::proxy::upsert_dedup(e).unwrap()
    }

    fn name_of(id: &str) -> String {
        load_raw(id).unwrap().config.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string()
    }

    /// The reported failure: 72 selected, 2 exported. One profile whose browser
    /// folder holds something that cannot be copied (here the dangling
    /// `SingletonLock` a crashed Chromium leaves) used to end the whole export
    /// with an error at that point. Now that entry is not even looked at
    /// (it is cache), and a profile that genuinely cannot be read is reported
    /// by name while the rest still come out.
    #[cfg(unix)]
    #[test]
    fn export_carries_on_past_a_bad_profile_and_says_which() {
        let root = Root::new("exp-bad");
        let mut ids = Vec::new();
        for i in 0..6 {
            let id = make(&format!("P{i}"), None);
            if i == 2 {
                let udd = user_data_dir(&id).unwrap();
                std::os::unix::fs::symlink("nowhere-1234", udd.join("SingletonLock")).unwrap();
            }
            ids.push(id);
        }
        // Profile #4's file is gone entirely (listed, but unreadable).
        fs::remove_file(path_for(&ids[3]).unwrap()).unwrap();

        let dest = root.dir.join("out");
        let rep = export_bundle(&ids, &dest).unwrap();
        assert_eq!(rep.exported, 5, "{rep:?}");
        assert_eq!(rep.failed.len(), 1, "{rep:?}");
        assert_eq!(rep.failed[0].name, ids[3]);
        assert_eq!(fs::read_dir(&dest).unwrap().count(), 5, "no half-written folder for the failure");
    }

    /// What goes into a bundle is the account, not the cache.
    #[test]
    fn a_bundle_holds_the_login_and_none_of_the_cache() {
        let root = Root::new("exp-slim");
        let id = make("Slim", None);
        let dest = root.dir.join("out");
        let rep = export_bundle(&[id], &dest).unwrap();
        assert_eq!(rep.exported, 1);
        let b = dest.join("Slim");
        assert!(b.join("profile.json").is_file());
        assert!(b.join("proxy.json").is_file());
        assert_eq!(fs::read_to_string(b.join("userdata/Default/Cookies")).unwrap(), "cookies-of-Slim");
        assert!(b.join("userdata/Local State").is_file());
        assert!(b.join("userdata/Default/Local Storage/leveldb/000003.log").is_file());
        assert!(!b.join("userdata/Default/Cache").exists(), "cache must not travel");
        assert!(!b.join("userdata/GrShaderCache").exists(), "cache must not travel");
        assert!(!b.join("userdata/Default/Local Storage/leveldb/LOCK").exists(), "a held lock file is skipped");
    }

    /// Machine to machine, the real use: the profile arrives with its login and
    /// with its proxy, and 40 profiles sharing one proxy do not make 40 proxies.
    #[test]
    fn a_bundle_round_trips_to_another_machine_with_login_and_proxy() {
        let root = Root::new("round");
        let px = a_proxy();
        let ids: Vec<String> = (0..40).map(|i| make(&format!("Shop {i}"), Some(&px))).collect();
        let dest = root.dir.join("bundle");
        let rep = export_bundle(&ids, &dest).unwrap();
        assert_eq!((rep.exported, rep.failed.len()), (40, 0), "{rep:?}");

        root.switch_to("machine-b");
        assert!(crate::proxy::load().unwrap().proxies.is_empty(), "machine B starts with no proxies");
        let imp = import_bundle(&dest).unwrap();
        assert_eq!((imp.imported, imp.failed.len()), (40, 0), "{imp:?}");
        assert!(imp.warnings.is_empty(), "{imp:?}");

        let listed = list_all().unwrap();
        assert_eq!(listed.len(), 40);
        let proxies = crate::proxy::load().unwrap().proxies;
        assert_eq!(proxies.len(), 1, "one shared proxy, not one per profile");
        for p in &listed {
            assert_eq!(p.proxy_id.as_deref(), Some(proxies[0].id.as_str()), "{} lost its proxy", p.name);
            let udd = user_data_dir(&p.id).unwrap();
            assert_eq!(fs::read_to_string(udd.join("Default/Cookies")).unwrap(), format!("cookies-of-{}", p.name), "{} lost its login", p.name);
        }
    }

    /// "Số lượng lớn": a few hundred profiles in one go, none lost, and quick —
    /// the cache that used to be copied is where the time and the failures were.
    #[test]
    fn a_few_hundred_profiles_export_and_import_in_one_go() {
        let root = Root::new("bulk");
        let ids: Vec<String> = (0..300).map(|i| make(&format!("Acc {i:03}"), None)).collect();
        let dest = root.dir.join("bundle");
        let t = std::time::Instant::now();
        let rep = export_bundle(&ids, &dest).unwrap();
        assert_eq!((rep.exported, rep.failed.len()), (300, 0), "{rep:?}");
        root.switch_to("machine-b");
        let imp = import_bundle(&dest).unwrap();
        assert_eq!((imp.imported, imp.failed.len()), (300, 0), "{imp:?}");
        assert_eq!(list_all().unwrap().len(), 300);
        println!("300 profiles out and back in: {:?}", t.elapsed());
    }

    /// One unreadable bundle among good ones: the others still import, the bad
    /// one is named, and nothing half-made is left in the list.
    #[test]
    fn import_carries_on_past_a_bad_bundle() {
        let root = Root::new("imp-bad");
        let ids: Vec<String> = (0..5).map(|i| make(&format!("G{i}"), None)).collect();
        let dest = root.dir.join("bundle");
        export_bundle(&ids, &dest).unwrap();
        fs::write(dest.join("G2/profile.json"), b"{ not json").unwrap();

        root.switch_to("machine-b");
        let imp = import_bundle(&dest).unwrap();
        assert_eq!(imp.imported, 4, "{imp:?}");
        assert_eq!(imp.failed.len(), 1, "{imp:?}");
        assert_eq!(imp.failed[0].name, "G2");
        assert_eq!(list_all().unwrap().len(), 4);
    }

    /// If a profile's browser data cannot be copied in, the profile made for it
    /// is taken out again, not left behind to open empty and logged out.
    #[cfg(unix)]
    #[test]
    fn a_profile_whose_data_will_not_copy_is_not_left_half_imported() {
        use std::os::unix::fs::PermissionsExt;
        let root = Root::new("imp-half");
        let ids: Vec<String> = (0..3).map(|i| make(&format!("H{i}"), None)).collect();
        let dest = root.dir.join("bundle");
        export_bundle(&ids, &dest).unwrap();
        let locked = dest.join("H1/userdata/Default/Cookies");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        // Running as root would read it anyway; nothing to prove then.
        if fs::read(&locked).is_ok() {
            return;
        }

        root.switch_to("machine-b");
        let imp = import_bundle(&dest).unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(imp.imported, 2, "{imp:?}");
        assert_eq!(imp.failed.len(), 1, "{imp:?}");
        assert_eq!(imp.failed[0].name, "H1");
        let names: Vec<String> = list_all().unwrap().into_iter().map(|p| p.name).collect();
        assert!(!names.contains(&"H1".to_string()), "{names:?}");
        assert_eq!(list_all().unwrap().len(), 2);
    }

    /// A bundle from before proxies travelled in it: the profile names a proxy
    /// this machine has never seen. Left bound it would launch direct without a
    /// word; it is cleared and the person is told.
    #[test]
    fn an_old_bundle_pointing_at_an_unknown_proxy_is_unbound_and_flagged() {
        let root = Root::new("imp-legacy");
        let px = a_proxy();
        let id = make("Legacy", Some(&px));
        let dest = root.dir.join("bundle");
        export_bundle(&[id], &dest).unwrap();
        fs::remove_file(dest.join("Legacy/proxy.json")).unwrap();

        root.switch_to("machine-b");
        let imp = import_bundle(&dest).unwrap();
        assert_eq!(imp.imported, 1, "{imp:?}");
        assert_eq!(imp.warnings.len(), 1, "{imp:?}");
        assert!(imp.warnings[0].contains("Legacy"));
        let p = &list_all().unwrap()[0];
        assert_eq!(p.proxy_id, None);
    }

    #[test]
    fn folder_names_keep_vietnamese_stay_safe_and_never_collide() {
        let mut used = std::collections::HashSet::new();
        assert_eq!(dedup_folder_name("Nguyễn Văn Cường", &mut used), "Nguyễn Văn Cường");
        assert_eq!(dedup_folder_name("nguyễn văn cường", &mut used), "nguyễn văn cường (2)", "case-insensitive disks collide on this");
        assert_eq!(dedup_folder_name("a/b:c*d?", &mut used), "a_b_c_d_");
        assert_eq!(dedup_folder_name("CON", &mut used), "profile", "reserved on Windows");
        assert_eq!(dedup_folder_name("   ", &mut used), "profile (2)");
        assert_eq!(dedup_folder_name("name...", &mut used), "name");
        let long = "x".repeat(200);
        assert_eq!(dedup_folder_name(&long, &mut used).chars().count(), 60);
    }

    /// Real engine, real running profile: exported while open, it still comes
    /// out with its login files, and is flagged so the person knows to close it
    /// first. Opt-in (needs the browser installed):
    /// `HIR_RUN_BUNDLE_E2E=1 cargo test --no-default-features --features automation --lib export_of_a_running -- --ignored --nocapture`
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore]
    async fn export_of_a_running_profile_works_and_is_flagged() {
        if std::env::var("HIR_RUN_BUNDLE_E2E").as_deref() != Ok("1") {
            return;
        }
        let root = Root::new("exp-live");
        let fps = crate::fingerprints::list_all().unwrap();
        let tpl = fps.iter().find(|f| f.platform == "macOS" || f.platform == "Windows").unwrap().id.clone();
        let mut merged = crate::merge_library_fingerprint(&tpl).unwrap();
        merged.insert("name".into(), serde_json::Value::String("Live".into()));
        crate::enrich_new_config(None, &mut merged);
        let meta = crate::save_profile_core(None, serde_json::Value::Object(merged), false).unwrap();
        crate::launch::launch_profile(&meta.id, true, false).await.expect("launch");
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;

        let dest = root.dir.join("out");
        let rep = export_bundle(&[meta.id.clone()], &dest).unwrap();
        println!("report: {rep:?}");
        let files: Vec<String> = walk(&dest.join("Live/userdata"));
        println!("exported files: {files:?}");
        let _ = crate::process::Tracker::shared().kill(&meta.id).await;
        assert_eq!((rep.exported, rep.failed.len()), (1, 0), "{rep:?}");
        assert_eq!(rep.warnings.len(), 1, "a running profile must be flagged: {rep:?}");
        assert!(files.iter().any(|f| f.ends_with("Local State")), "{files:?}");
    }

    fn walk(dir: &Path) -> Vec<String> {
        let mut out = Vec::new();
        if let Ok(rd) = fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() { out.extend(walk(&p)); } else { out.push(p.display().to_string()); }
            }
        }
        out
    }

    #[test]
    fn names_with_vietnamese_survive_an_export() {
        let root = Root::new("exp-vi");
        let a = make("Lan - Sale 1 🚀", None);
        let b = make("Lan - Sale 1 🚀", None);
        let dest = root.dir.join("out");
        let rep = export_bundle(&[a.clone(), b.clone()], &dest).unwrap();
        assert_eq!(rep.exported, 2, "{rep:?}");
        let n = fs::read_dir(&dest).unwrap().count();
        assert_eq!(n, 2, "same name twice still gives two folders");
        assert_eq!(name_of(&a), "Lan - Sale 1 🚀");
    }
}
