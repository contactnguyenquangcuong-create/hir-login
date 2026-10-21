//! The partner tools that may borrow this app's profiles — HirS (the Shopee video tool) and Hir-F
//! (the Flow tool) — and which profile folders the owner has shared with each, and how far. Shared
//! like with a person in the folder's Chia sẻ dialog.
//!
//! A partner drives profiles through the local API and writes its own proxies into them. On top of
//! its "Cho phép … kết nối" switch, it only sees profiles in folders shared with it one by one, so
//! it can never put a proxy into a folder that has proxies of its own (see `api::partner_scope`):
//! - `use` ("Được dùng"): see, open and close the profiles, like a team member;
//! - `manage` ("Quản lý thư mục"): also add, edit (proxy included), move and delete them, and
//!   rename or delete the folder.
//! A profile with no folder is never shared. Each partner has its own list: sharing with one says
//! nothing about the other.
//!
//! Kept in files of their own under the config root — `hirs-folders.json`, `hirf-folders.json` —
//! not in settings.json, which the Settings page writes back whole. 0.2.9 kept HirS's list as plain
//! names: those read back as `use`, the smaller right.

use crate::store;
use anyhow::Result;
use std::collections::BTreeMap;
use std::sync::Mutex;

/// A tool that says who it is with `X-Hir-Client`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Partner {
    HirS,
    HirF,
}

impl Partner {
    pub const ALL: [Partner; 2] = [Partner::HirS, Partner::HirF];

    /// The id used in file names, JSON keys and the app's commands.
    pub fn key(self) -> &'static str {
        match self {
            Partner::HirS => "hirs",
            Partner::HirF => "hirf",
        }
    }

    /// How people read its name.
    pub fn name(self) -> &'static str {
        match self {
            Partner::HirS => "HirS",
            Partner::HirF => "Hir-F",
        }
    }

    pub fn from_key(s: &str) -> Option<Partner> {
        Partner::ALL.into_iter().find(|p| p.key() == s)
    }

    /// From an `X-Hir-Client` value: `HirS`, `HirF` or `Hir-F`, any case.
    pub fn from_header(v: &str) -> Option<Partner> {
        let flat: String = v.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
        Partner::from_key(&flat)
    }

    /// Whether its "Cho phép … kết nối" switch is on.
    pub fn allowed(self) -> bool {
        crate::settings::load()
            .map(|s| match self {
                Partner::HirS => s.hirs_allowed,
                Partner::HirF => s.hirf_allowed,
            })
            .unwrap_or(false)
    }

    fn file(self) -> &'static str {
        match self {
            Partner::HirS => "hirs-folders.json",
            Partner::HirF => "hirf-folders.json",
        }
    }
}

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

fn path(p: Partner) -> Result<std::path::PathBuf> {
    Ok(store::config_root()?.join(p.file()))
}

fn lock() -> std::sync::MutexGuard<'static, ()> {
    static L: Mutex<()> = Mutex::new(());
    L.lock().unwrap_or_else(|e| e.into_inner())
}

fn read(p: Partner) -> BTreeMap<String, Level> {
    let v = path(p)
        .ok()
        .and_then(|f| std::fs::read_to_string(f).ok())
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

fn write(p: Partner, map: &BTreeMap<String, Level>) -> Result<()> {
    let folders: serde_json::Map<String, serde_json::Value> =
        map.iter().map(|(k, l)| (k.clone(), l.as_str().into())).collect();
    let body = serde_json::to_string_pretty(&serde_json::json!({ "folders": folders }))?;
    std::fs::write(path(p)?, body)?;
    Ok(())
}

/// Every folder shared with `p`, with its level.
pub fn access(p: Partner) -> BTreeMap<String, Level> {
    read(p)
}

/// The folders shared with `p`, sorted.
pub fn list(p: Partner) -> Vec<String> {
    read(p).into_keys().collect()
}

pub fn level(p: Partner, folder: &str) -> Option<Level> {
    if folder.trim().is_empty() {
        return None;
    }
    read(p).get(folder).copied()
}

/// Shares one folder with `p` at `level`, or unshares it (`None`); returns the new map.
pub fn set(p: Partner, folder: &str, level: Option<Level>) -> Result<BTreeMap<String, Level>> {
    let folder = folder.trim();
    if folder.is_empty() {
        anyhow::bail!("Chỉ chia sẻ được một thư mục có tên.");
    }
    let _g = lock();
    let mut m = read(p);
    match level {
        Some(l) => { m.insert(folder.to_string(), l); }
        None => { m.remove(folder); }
    }
    write(p, &m)?;
    Ok(m)
}

/// Parses "none" | "use" | "manage" from the app.
pub fn parse_level(s: &str) -> Result<Option<Level>> {
    match s {
        "none" | "" => Ok(None),
        other => Level::parse(other).map(Some).ok_or_else(|| anyhow::anyhow!("Quyền không hợp lệ: {other}")),
    }
}

/// A folder renamed (from the app or the API) keeps its shares under the new name. Merged into a
/// folder that already had a share, the smaller of the two wins.
pub fn renamed(old: &str, new: &str) {
    let _g = lock();
    for p in Partner::ALL {
        let mut m = read(p);
        if let Some(l) = m.remove(old) {
            let new = new.trim();
            if !new.is_empty() {
                let merged = m.get(new).map_or(l, |&had| had.min(l));
                m.insert(new.to_string(), merged);
            }
            let _ = write(p, &m);
        }
    }
}

/// A deleted folder is no longer shared (a new folder of the same name starts unshared).
pub fn removed(folder: &str) {
    let _g = lock();
    for p in Partner::ALL {
        let mut m = read(p);
        if m.remove(folder).is_some() {
            let _ = write(p, &m);
        }
    }
}
