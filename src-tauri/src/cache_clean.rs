//! A closed profile's browser caches, removed.
//!
//! What a browser keeps only to load pages faster — downloaded pages and media, compiled scripts,
//! GPU shaders, the offline copies sites put in Cache Storage — is most of a profile's folder: a
//! profile that makes videos grew to 370 MB, of which logins, extensions and site data were a
//! few tens. None of it goes to the team, and none of it is needed: the next open fetches what it
//! needs again. So it is removed once the profile has closed (and, at startup, from every profile
//! that is not open). Logins, extensions, site data (Local Storage, IndexedDB), history and
//! settings are not touched.

use std::path::Path;

/// Folders the browser rebuilds on its own, relative to the profile's user-data folder.
const CACHES: &[&str] = &[
    "Default/Cache",
    "Default/Code Cache",
    "Default/GPUCache",
    "Default/DawnGraphiteCache",
    "Default/DawnWebGPUCache",
    "Default/Service Worker/CacheStorage",
    "GrShaderCache",
    "GraphiteDawnCache",
    "ShaderCache",
    "component_crx_cache",
];

/// Removes the caches of the profile in `udd`, unless a browser has it open. Returns the bytes freed.
pub fn clean(udd: &Path) -> u64 {
    if !udd.is_dir() || crate::process::Tracker::user_data_in_use(udd) {
        return 0;
    }
    let mut freed = 0;
    for rel in CACHES {
        let dir = udd.join(rel);
        if !dir.is_dir() {
            continue;
        }
        let size = dir_size(&dir);
        // A file still held by a process of the browser that is ending stays; the rest goes.
        if crate::winfs::remove_dir_all(&dir).is_ok() || !dir.exists() {
            freed += size;
        } else {
            freed += size.saturating_sub(dir_size(&dir));
        }
    }
    freed
}

/// Cleans the caches of every profile that is not open, one at a time, gently. Run once, a while
/// after startup: profiles closed by an earlier version kept theirs.
pub async fn sweep() {
    #[allow(unused_mut)]
    let mut wait = std::time::Duration::from_secs(90);
    // Debug builds: an end-to-end test sets the wait.
    #[cfg(debug_assertions)]
    if let Some(ms) = std::env::var("HIR_LOGIN_CACHE_SWEEP_MS").ok().and_then(|v| v.parse().ok()) {
        wait = std::time::Duration::from_millis(ms);
    }
    tokio::time::sleep(wait).await;
    let Ok(root) = crate::store::user_data_root() else { return };
    let Ok(dirs) = std::fs::read_dir(&root) else { return };
    let ids: Vec<String> = dirs.flatten().filter(|e| e.path().is_dir()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    let (mut freed, mut n) = (0u64, 0usize);
    for id in ids {
        if crate::process::Tracker::shared().is_running(&id) || crate::launch::is_starting(&id) {
            continue;
        }
        let udd = root.join(&id);
        let f = tokio::task::spawn_blocking(move || clean(&udd)).await.unwrap_or(0);
        if f > 0 {
            freed += f;
            n += 1;
        }
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }
    if freed > 0 {
        crate::cloud_sync::log_line(&format!("browser caches: {} MB freed from {n} closed profiles", freed / 1_000_000));
    }
}

fn dir_size(dir: &Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(dir) else { return 0 };
    rd.flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_size(&e.path()),
            Ok(_) => e.metadata().map(|m| m.len()).unwrap_or(0),
            Err(_) => 0,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_the_caches_and_keeps_what_matters() {
        let udd = std::env::temp_dir().join(format!("cache-clean-{}", uuid::Uuid::new_v4()));
        let put = |rel: &str, bytes: usize| {
            let p = udd.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, vec![0u8; bytes]).unwrap();
        };
        put("Default/Cache/Cache_Data/data_1", 5000);
        put("Default/Code Cache/js/abc", 3000);
        put("GrShaderCache/data_0", 2000);
        put("Default/Service Worker/CacheStorage/x/1", 1000);
        put("Default/Service Worker/Database/CURRENT", 10);
        put("Default/Network/Cookies", 10);
        put("Default/Local Storage/leveldb/000003.log", 10);
        put("Default/IndexedDB/https_x/1.log", 10);
        put("Default/Extensions/abc/1.0_0/manifest.json", 10);
        put("Default/Preferences", 10);

        assert_eq!(clean(&udd), 11000);
        for gone in ["Default/Cache", "Default/Code Cache", "GrShaderCache", "Default/Service Worker/CacheStorage"] {
            assert!(!udd.join(gone).exists(), "{gone} stays");
        }
        for kept in ["Default/Service Worker/Database/CURRENT", "Default/Network/Cookies", "Default/Local Storage/leveldb/000003.log", "Default/IndexedDB/https_x/1.log", "Default/Extensions/abc/1.0_0/manifest.json", "Default/Preferences"] {
            assert!(udd.join(kept).exists(), "{kept} was removed");
        }
        assert_eq!(clean(&udd), 0, "nothing left to free");
        let _ = std::fs::remove_dir_all(&udd);
    }
}
