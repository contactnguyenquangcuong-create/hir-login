// User-managed Fingerprint Library.
//
// Each entry is a full FingerprintConfig JSON stored under
// `$CONFIG/shardx-launcher/fingerprints/<id>.json`.  The GPU select in
// the profile editor pulls its options from here — i.e. the user can
// curate which GPUs/devices show up by importing their own JSON files
// (or by deleting bundled ones).
//
// On first run we seed the directory with the bundled gpu presets so
// the launcher is usable out of the box.

use crate::store;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// One row in the library UI; also what the profile editor uses to
/// populate the GPU select.  `payload` is the verbatim FingerprintConfig
/// (without the `_meta` envelope launcher profiles use).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryEntry {
    pub id: String,
    pub label: String,
    pub platform: String,
    pub chrome: String,
    pub gpu: String,
    pub tag_color: String,
    /// True for the bundled starter set — UI marks them so the user
    /// knows they can re-seed by deleting/reimporting.
    #[serde(default)]
    pub builtin: bool,
    /// Full FingerprintConfig JSON.  Stored inline so the frontend can
    /// preview it and the profile editor can read derived fields
    /// (screen, platform, webgl) without a second round-trip.
    pub payload: Value,
}

fn safe_id(id: &str) -> Result<String> {
    if id.is_empty() || id.contains(['/', '\\']) || id.contains("..") {
        anyhow::bail!("invalid fingerprint id");
    }
    Ok(id.to_string())
}

fn path_for(id: &str) -> Result<PathBuf> {
    let id = safe_id(id)?;
    Ok(store::fingerprints_dir()?.join(format!("{id}.json")))
}

/// Seed the library with the bundled GPU presets on the first call
fn tag_color_for(platform: &str) -> String {
    match platform {
        "macOS" => "#8b5cf6".into(),
        "Windows" => "#5dade2".into(),
        "Linux" => "#4ade80".into(),
        // navigator.platform on a phone is "Linux armv8l", so a handset is
        // matched on the prefix rather than on equality.
        p if p.starts_with("Android") || p.starts_with("Linux arm") => "#fb923c".into(),
        _ => "#a78bfa".into(),
    }
}

/// Parse one fingerprint JSON file into its full `LibraryEntry` (payload
/// included).  Shared by `list_all` (which then strips the payload back
/// out for the bulk response) and `get` (which reads a single file
/// directly instead of scanning the whole directory).
fn read_entry(path: &PathBuf) -> Result<Option<LibraryEntry>> {
    if path.extension().and_then(|s| s.to_str()) != Some("json") {
        return Ok(None);
    }
    let body = fs::read_to_string(path)?;
    if let Ok(e) = serde_json::from_str::<LibraryEntry>(&body) {
        return Ok(Some(e));
    }
    if let Ok(payload) = serde_json::from_str::<Value>(&body) {
        // Bare FingerprintConfig (no LibraryEntry wrapper) — wrap on the
        // fly so user-imported files that came straight from ShardX
        // still show up.
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("imported")
            .to_string();
        return Ok(Some(wrap_payload(&id, &payload)));
    }
    Ok(None)
}

pub fn list_all() -> Result<Vec<LibraryEntry>> {
    // Pure filesystem read.  Everything in
    //   $CONFIG/shardx-launcher/fingerprints/*.json
    // becomes a library entry, no matter how it got there — UI
    // imports, drag-and-drop, or the user dumping files in by hand.
    // No bundled set, no compile-time tables, no "builtin" concept.
    //
    // The payload is dropped from each entry here: with a large library
    // (user-imported sets can run into the thousands) shipping every
    // full FingerprintConfig over IPC on every list call makes the UI
    // visibly lag. Callers that need the payload for one entry (GPU
    // pick, profile save) fetch it with `get(id)` instead.
    let dir = store::fingerprints_dir()?;
    let mut out = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        if let Some(mut e) = read_entry(&entry.path())? {
            e.payload = Value::Null;
            out.push(e);
        }
    }
    out.sort_by(|a, b| a.label.cmp(&b.label));
    Ok(out)
}

/// Pull a label / platform / chrome / GPU description out of a raw
/// FingerprintConfig.  Used both at import time and when listing
/// bare-JSON files in the library dir.
fn wrap_payload(id: &str, p: &Value) -> LibraryEntry {
    let label = p
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(id)
        .to_string();
    let platform = p
        .get("navigator")
        .and_then(|n| n.get("platform"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let chrome = p
        .get("client_hints")
        .and_then(|c| c.get("brand_version"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let gpu = p
        .get("webgl")
        .and_then(|w| w.get("renderer"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    LibraryEntry {
        id: id.to_string(),
        label,
        platform: platform.clone(),
        chrome,
        gpu,
        tag_color: tag_color_for(&platform),
        builtin: false,
        payload: p.clone(),
    }
}

pub fn get(id: &str) -> Result<Option<LibraryEntry>> {
    // Read the one target file directly rather than scanning/parsing the
    // whole library — the bulk `list_all` path already strips payloads,
    // so this is the only place a full FingerprintConfig gets loaded.
    let path = path_for(id)?;
    if !path.exists() {
        return Ok(None);
    }
    read_entry(&path)
}

/// Import a raw FingerprintConfig JSON.  Accepts the user's text and
/// returns the saved entry.  If `id` is empty a slug is derived from
/// `payload.name` (or a UUID if no name).
pub fn import(json_text: &str, id_hint: Option<String>) -> Result<LibraryEntry> {
    let payload: Value =
        serde_json::from_str(json_text).context("not a valid JSON FingerprintConfig")?;
    let raw_id = id_hint
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            payload
                .get("name")
                .and_then(|v| v.as_str())
                .map(slugify)
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
        });
    let id = ensure_unique_id(&raw_id)?;
    let entry = wrap_payload(&id, &payload);
    let path = path_for(&id)?;
    fs::write(path, serde_json::to_string_pretty(&entry)?)?;
    note_custom(&id);
    crate::cloud_sync::kick();
    Ok(entry)
}

fn custom_marker() -> Option<PathBuf> {
    store::config_root().ok().map(|d| d.join("custom-fingerprints.txt"))
}

/// Ids the operator added themselves (imports, or pulled from the team) — the
/// set worth sharing, since the shipped fingerprints are already on every machine.
pub fn custom_ids() -> Vec<String> {
    custom_marker()
        .and_then(|p| fs::read_to_string(p).ok())
        .map(|t| t.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect())
        .unwrap_or_default()
}

pub fn note_custom(id: &str) {
    if custom_ids().iter().any(|x| x == id) {
        return;
    }
    if let Some(p) = custom_marker() {
        use std::io::Write;
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(p) {
            let _ = writeln!(f, "{id}");
        }
    }
}

/// Import every `.json` file in `dir` as a library entry, e.g. to carry a
/// custom-generated set (or another machine's library) over in one go
/// instead of pasting files in one at a time. Returns the count imported;
/// a file that fails to parse is skipped rather than aborting the batch.
pub fn import_folder(dir: &Path) -> Result<usize> {
    let mut n = 0;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.path().extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = fs::read_to_string(entry.path()) else { continue };
        if import(&text, None).is_ok() {
            n += 1;
        }
    }
    Ok(n)
}

/// Copy the fingerprint set shipped inside the app (`resources/fingerprints`)
/// into the library. Runs once per app version (marker file) and never
/// overwrites an existing file, so edits and deletions between updates hold.
pub fn seed_bundled(resource_dir: &Path, app_version: &str) -> Result<usize> {
    let src = resource_dir.join("resources").join("fingerprints");
    let src = if src.is_dir() { src } else { resource_dir.join("fingerprints") };
    if !src.is_dir() {
        return Ok(0);
    }
    let dir = store::fingerprints_dir()?;
    let marker = dir.join(".hirlogin-seeded");
    if fs::read_to_string(&marker).ok().as_deref() == Some(app_version) {
        return Ok(0);
    }
    let mut n = 0;
    for entry in fs::read_dir(&src)? {
        let entry = entry?;
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let dst = dir.join(entry.file_name());
        if !dst.exists() && fs::copy(&p, &dst).is_ok() {
            n += 1;
        }
    }
    let _ = fs::write(&marker, app_version);
    Ok(n)
}

fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if c == ' ' || c == '_' || c == '-' || c == '.' {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() { uuid::Uuid::new_v4().to_string() } else { trimmed }
}

fn ensure_unique_id(base: &str) -> Result<String> {
    let dir = store::fingerprints_dir()?;
    if !dir.join(format!("{base}.json")).exists() {
        return Ok(base.into());
    }
    for n in 2..1000 {
        let cand = format!("{base}-{n}");
        if !dir.join(format!("{cand}.json")).exists() {
            return Ok(cand);
        }
    }
    Ok(format!("{base}-{}", uuid::Uuid::new_v4()))
}

pub fn delete(id: &str) -> Result<()> {
    let path = path_for(id)?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}
