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
    for i in 0..zip.len() {
        let mut f = zip.by_index(i)?;
        let Some(rel) = f.enclosed_name() else { continue };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if f.is_dir() {
            continue;
        }
        let mut buf = Vec::with_capacity(f.size() as usize);
        f.read_to_end(&mut buf)?;
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
    Ok(())
}

/// Call before spawning the browser. Locks the profile on the server (fails
/// loudly if another device holds it) and pulls its latest bundle down. A
/// no-op when sync isn't configured.
pub async fn checkout(profile_id: &str) -> Result<()> {
    let Some((cfg, base, token)) = active_config()? else { return Ok(()) };
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

    Ok(())
}

/// Call after the browser exits. Pushes the current bundle up and releases
/// the lock. A no-op when sync isn't configured. Best-effort: logged, never
/// fatal — the operator still has their local copy either way.
pub async fn checkin(profile_id: &str) -> Result<()> {
    let Some((cfg, base, token)) = active_config()? else { return Ok(()) };
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
