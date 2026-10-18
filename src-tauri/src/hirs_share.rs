//! Which profile folders the owner has shared with HirS ("Chia sẻ cho HirS").
//!
//! HirS (the owner's video tool) drives profiles through the local API and writes its own
//! proxies into them. On top of the "Cho phép HirS kết nối" switch, it only sees and touches
//! profiles in folders shared here one by one, so it can never put a proxy into a folder that
//! has proxies of its own (see `api::hirs_scope`). A profile with no folder is never shared.
//!
//! Kept in its own file, `hirs-folders.json` under the config root — not in settings.json,
//! which the Settings page writes back whole.

use crate::store;
use anyhow::Result;
use std::collections::BTreeSet;
use std::sync::Mutex;

fn path() -> Result<std::path::PathBuf> {
    Ok(store::config_root()?.join("hirs-folders.json"))
}

fn lock() -> std::sync::MutexGuard<'static, ()> {
    static L: Mutex<()> = Mutex::new(());
    L.lock().unwrap_or_else(|e| e.into_inner())
}

fn read() -> BTreeSet<String> {
    path()
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("folders").cloned())
        .and_then(|f| serde_json::from_value::<Vec<String>>(f).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|f| !f.trim().is_empty())
        .collect()
}

fn write(set: &BTreeSet<String>) -> Result<()> {
    let body = serde_json::to_string_pretty(&serde_json::json!({ "folders": set }))?;
    std::fs::write(path()?, body)?;
    Ok(())
}

/// The shared folders, sorted.
pub fn list() -> Vec<String> {
    read().into_iter().collect()
}

pub fn is_shared(folder: &str) -> bool {
    !folder.trim().is_empty() && read().contains(folder)
}

/// Share or unshare one folder; returns the new list.
pub fn set(folder: &str, shared: bool) -> Result<Vec<String>> {
    let folder = folder.trim();
    if folder.is_empty() {
        anyhow::bail!("Chỉ chia sẻ được một thư mục có tên.");
    }
    let _g = lock();
    let mut s = read();
    if shared { s.insert(folder.to_string()); } else { s.remove(folder); }
    write(&s)?;
    Ok(s.into_iter().collect())
}

/// A folder renamed (from the app or the API) keeps its share under the new name.
pub fn renamed(old: &str, new: &str) {
    let _g = lock();
    let mut s = read();
    if s.remove(old) && !new.trim().is_empty() {
        s.insert(new.trim().to_string());
    }
    let _ = write(&s);
}

/// A deleted folder is no longer shared (a new folder of the same name starts unshared).
pub fn removed(folder: &str) {
    let _g = lock();
    let mut s = read();
    if s.remove(folder) {
        let _ = write(&s);
    }
}
