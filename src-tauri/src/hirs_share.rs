//! Which profile folders the owner has shared with HirS, and how far — shared like with a person
//! in the folder's Chia sẻ dialog.
//!
//! HirS (the owner's video tool) drives profiles through the local API and writes its own
//! proxies into them. On top of the "Cho phép HirS kết nối" switch, it only sees profiles in
//! folders shared here one by one, so it can never put a proxy into a folder that has proxies of
//! its own (see `api::hirs_scope`):
//! - `use` ("Được dùng"): see, open and close the profiles, like a team member;
//! - `manage` ("Quản lý thư mục"): also add, edit (proxy included), move and delete them, and
//!   rename or delete the folder.
//! A profile with no folder is never shared.
//!
//! Kept in its own file, `hirs-folders.json` under the config root — not in settings.json,
//! which the Settings page writes back whole. 0.2.9 kept a plain list of names: those read back
//! as `use`, the smaller right.

use crate::store;
use anyhow::Result;
use std::collections::BTreeMap;
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Use,
    Manage,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Use => "use",
            Level::Manage => "manage",
        }
    }
    fn parse(s: &str) -> Option<Level> {
        match s {
            "use" => Some(Level::Use),
            "manage" => Some(Level::Manage),
            _ => None,
        }
    }
}

fn path() -> Result<std::path::PathBuf> {
    Ok(store::config_root()?.join("hirs-folders.json"))
}

fn lock() -> std::sync::MutexGuard<'static, ()> {
    static L: Mutex<()> = Mutex::new(());
    L.lock().unwrap_or_else(|e| e.into_inner())
}

fn read() -> BTreeMap<String, Level> {
    let v = path()
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("folders").cloned())
        .unwrap_or(serde_json::Value::Null);
    let mut out = BTreeMap::new();
    match v {
        serde_json::Value::Object(m) => {
            for (k, l) in m {
                if let Some(l) = l.as_str().and_then(Level::parse) {
                    out.insert(k, l);
                }
            }
        }
        serde_json::Value::Array(a) => {
            for k in a.into_iter().filter_map(|k| k.as_str().map(str::to_string)) {
                out.insert(k, Level::Use);
            }
        }
        _ => {}
    }
    out.retain(|k, _| !k.trim().is_empty());
    out
}

fn write(map: &BTreeMap<String, Level>) -> Result<()> {
    let folders: serde_json::Map<String, serde_json::Value> =
        map.iter().map(|(k, l)| (k.clone(), l.as_str().into())).collect();
    let body = serde_json::to_string_pretty(&serde_json::json!({ "folders": folders }))?;
    std::fs::write(path()?, body)?;
    Ok(())
}

/// Every shared folder with its level.
pub fn access() -> BTreeMap<String, Level> {
    read()
}

/// The shared folders, sorted.
pub fn list() -> Vec<String> {
    read().into_keys().collect()
}

pub fn level(folder: &str) -> Option<Level> {
    if folder.trim().is_empty() {
        return None;
    }
    read().get(folder).copied()
}

/// Shares one folder at `level`, or unshares it (`None`); returns the new map.
pub fn set(folder: &str, level: Option<Level>) -> Result<BTreeMap<String, Level>> {
    let folder = folder.trim();
    if folder.is_empty() {
        anyhow::bail!("Chỉ chia sẻ được một thư mục có tên.");
    }
    let _g = lock();
    let mut m = read();
    match level {
        Some(l) => { m.insert(folder.to_string(), l); }
        None => { m.remove(folder); }
    }
    write(&m)?;
    Ok(m)
}

/// Parses "none" | "use" | "manage" from the app.
pub fn parse_level(s: &str) -> Result<Option<Level>> {
    match s {
        "none" | "" => Ok(None),
        other => Level::parse(other).map(Some).ok_or_else(|| anyhow::anyhow!("Quyền không hợp lệ: {other}")),
    }
}

/// A folder renamed (from the app or the API) keeps its share under the new name. Merged into a
/// folder that already had a share, the smaller of the two wins.
pub fn renamed(old: &str, new: &str) {
    let _g = lock();
    let mut m = read();
    if let Some(l) = m.remove(old) {
        let new = new.trim();
        if !new.is_empty() {
            let merged = m.get(new).map_or(l, |&had| had.min(l));
            m.insert(new.to_string(), merged);
        }
    }
    let _ = write(&m);
}

/// A deleted folder is no longer shared (a new folder of the same name starts unshared).
pub fn removed(folder: &str) {
    let _g = lock();
    let mut m = read();
    if m.remove(folder).is_some() {
        let _ = write(&m);
    }
}
