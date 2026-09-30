// $CONFIG/hir-login/: settings.json, proxies.json, bookmarks.json, and under
// data_root() — profiles/, user-data/, extensions/, trash/.
//
// data_root() is movable to another disk from Settings; the config files stay
// put, since that is where the new location is recorded.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, RwLock};

/// Move `<base>/<old_leaf>` to `<base>/<new_leaf>` the first time this runs, so an
/// update from before the app's rename keeps every existing profile, setting and
/// cookie exactly where it already was — nothing here is copied or recreated.
/// Safe to call from more than one place (config dir and data dir can be the
/// same folder on macOS/Windows): the rename is attempted once per `new` path
/// per run, and if it can't complete (the two live on different volumes, or
/// something is still holding the old folder open) that old folder is left
/// untouched rather than risking data split across both.
pub fn migrate_legacy_dir(base: &Path, old_leaf: &str, new_leaf: &str) -> PathBuf {
    static ATTEMPTED: OnceLock<Mutex<std::collections::HashSet<PathBuf>>> = OnceLock::new();
    let new = base.join(new_leaf);
    let mut attempted = ATTEMPTED.get_or_init(|| Mutex::new(Default::default()))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if attempted.insert(new.clone()) && !new.exists() {
        let old = base.join(old_leaf);
        if old.exists() {
            let _ = std::fs::rename(&old, &new);
        }
    }
    new
}

pub fn config_root() -> Result<PathBuf> {
    let base = dirs::config_dir().context("OS config dir unavailable")?;
    let root = migrate_legacy_dir(&base, "shardx-launcher", "hir-login");
    std::fs::create_dir_all(&root)?;
    Ok(root)
}

fn data_root_cell() -> &'static RwLock<Option<PathBuf>> {
    static CELL: OnceLock<RwLock<Option<PathBuf>>> = OnceLock::new();
    CELL.get_or_init(|| RwLock::new(None))
}

/// Point the heavy directories at `root` (None = back to the config dir).
pub fn set_data_root(root: Option<PathBuf>) {
    if let Ok(mut g) = data_root_cell().write() {
        *g = root;
    }
}

/// Where profiles, user-data, extensions and the trash live.
pub fn data_root() -> Result<PathBuf> {
    let over = data_root_cell().read().ok().and_then(|g| g.clone());
    match over {
        Some(p) => {
            std::fs::create_dir_all(&p)?;
            Ok(p)
        }
        None => config_root(),
    }
}

fn data_sub(name: &str) -> Result<PathBuf> {
    let p = data_root()?.join(name);
    std::fs::create_dir_all(&p)?;
    Ok(p)
}

pub fn profiles_dir() -> Result<PathBuf> {
    data_sub("profiles")
}

pub fn user_data_root() -> Result<PathBuf> {
    data_sub("user-data")
}

/// Unpacked extensions, one directory per id; `--load-extension` points here.
pub fn extensions_dir() -> Result<PathBuf> {
    data_sub("extensions")
}

/// Deleted profiles, one `<id>.zip` + `<id>.json` manifest each.
pub fn trash_dir() -> Result<PathBuf> {
    data_sub("trash")
}

pub fn fingerprints_dir() -> Result<PathBuf> {
    let p = config_root()?.join("fingerprints");
    std::fs::create_dir_all(&p)?;
    Ok(p)
}

/// Cached Widevine CDM, seeded from a host Chrome install (or downloaded from
/// the project's git LFS bucket).  Every freshly-created profile gets a
/// pre-warmed copy so a DRM page doesn't stall on the component updater.
pub fn widevine_cache_dir() -> Result<PathBuf> {
    Ok(config_root()?.join("widevine-cdm"))
}

pub fn proxies_path() -> Result<PathBuf> {
    Ok(user_files_root()?.join("proxies.json"))
}

pub fn settings_path() -> Result<PathBuf> {
    Ok(user_files_root()?.join("settings.json"))
}

/// Where the user's own small files live (settings, proxies, sync state).
/// Production: the config dir.  Test builds: never the real one — a test that
/// enables sync or saves a proxy must not leave that behind in the developer's
/// installed app — so they follow the data-root override, or a throw-away
/// per-process folder.
#[cfg(not(test))]
pub fn user_files_root() -> Result<PathBuf> {
    config_root()
}

#[cfg(test)]
pub fn user_files_root() -> Result<PathBuf> {
    let over = data_root_cell().read().ok().and_then(|g| g.clone());
    let p = over.unwrap_or_else(|| {
        std::env::temp_dir().join(format!("hir-login-test-files-{}", std::process::id()))
    });
    std::fs::create_dir_all(&p)?;
    Ok(p)
}

/// Folder-scoped bookmarks, merged into each profile's Bookmarks on launch.
pub fn bookmarks_path() -> Result<PathBuf> {
    Ok(user_files_root()?.join("bookmarks.json"))
}

/// Automation projects: one JSON file holding every project's blocks.
pub fn automation_path() -> Result<PathBuf> {
    Ok(config_root()?.join("automation.json"))
}

/// ProxyShard billing-API config (Bearer key). Kept in its own file so the
/// Settings page (which round-trips the whole Settings struct) can never
/// clobber the saved key.
pub fn psapi_path() -> Result<PathBuf> {
    Ok(config_root()?.join("psapi.json"))
}

#[cfg(test)]
mod migrate_tests {
    use super::*;

    /// The old folder's whole tree — a settings file plus a subfolder like
    /// `runtime/` — must land intact under the new name, and a second call must
    /// be a no-op rather than trying (and failing) to move an already-empty
    /// source again.
    #[test]
    fn legacy_folder_is_moved_once_with_everything_inside_it() {
        let base = std::env::temp_dir().join(format!("hir-migrate-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(base.join("shardx-launcher").join("runtime")).unwrap();
        std::fs::write(base.join("shardx-launcher").join("settings.json"), b"{\"x\":1}").unwrap();
        std::fs::write(base.join("shardx-launcher").join("runtime").join("chrome"), b"bin").unwrap();

        let root = migrate_legacy_dir(&base, "shardx-launcher", "hir-login");

        assert_eq!(root, base.join("hir-login"));
        assert!(!base.join("shardx-launcher").exists(), "old folder is gone, not copied alongside");
        assert_eq!(std::fs::read(root.join("settings.json")).unwrap(), b"{\"x\":1}");
        assert_eq!(std::fs::read(root.join("runtime").join("chrome")).unwrap(), b"bin");

        // A later run — no old folder any more — must not touch the new one.
        std::fs::write(root.join("settings.json"), b"{\"x\":2}").unwrap();
        let root2 = migrate_legacy_dir(&base, "shardx-launcher", "hir-login");
        assert_eq!(root2, root);
        assert_eq!(std::fs::read(root.join("settings.json")).unwrap(), b"{\"x\":2}", "untouched by the second call");

        let _ = std::fs::remove_dir_all(&base);
    }

    /// A fresh machine with neither folder gets the new, empty one created by the caller
    /// (`migrate_legacy_dir` itself only ever renames — it never creates a directory).
    #[test]
    fn no_legacy_folder_is_a_plain_no_op() {
        let base = std::env::temp_dir().join(format!("hir-migrate-fresh-{}", uuid::Uuid::new_v4()));
        let root = migrate_legacy_dir(&base, "shardx-launcher", "hir-login");
        assert_eq!(root, base.join("hir-login"));
        assert!(!root.exists());
        let _ = std::fs::remove_dir_all(&base);
    }
}
