//! Moving the data root. Copy, verify, then delete — a rename cannot cross
//! disks. `launch` checks the lock, so the UI is not the only guard.

use crate::{settings, store};
use anyhow::{Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Emitter;

static MIGRATING: AtomicBool = AtomicBool::new(false);

pub fn in_progress() -> bool {
    MIGRATING.load(Ordering::SeqCst)
}

/// What the progress bar reads.
#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    /// "scan" | "copy" | "verify" | "cleanup" | "done"
    pub phase: &'static str,
    pub done: u64,
    pub total: u64,
    pub percent: u8,
    /// File currently being moved, relative to the root.
    pub current: String,
}

/// Directories that move; everything else in the config dir stays where the
/// launcher can always find it. Exactly what is looked up through `data_root()`:
/// `team-sync` is the team server's store (every profile's bundle, the members,
/// the sharing) — left behind, a server whose data was moved came back empty
/// on the new disk while the old copy kept filling the system disk.
pub(crate) const MOVED: &[&str] = &["profiles", "user-data", "extensions", "trash", "team-sync"];

fn emit(app: &tauri::AppHandle, p: Progress) {
    let _ = app.emit("data-migration", p);
}

/// Move the data root to `dst`. Returns the number of files moved.
pub fn run(app: &tauri::AppHandle, dst: &Path) -> Result<u64> {
    if MIGRATING.swap(true, Ordering::SeqCst) {
        anyhow::bail!("a migration is already running");
    }
    let out = do_run(app, dst);
    MIGRATING.store(false, Ordering::SeqCst);
    if out.is_err() {
        emit(app, Progress { phase: "done", done: 0, total: 0, percent: 100, current: String::new() });
    }
    out
}

fn do_run(app: &tauri::AppHandle, dst: &Path) -> Result<u64> {
    let src = store::data_root()?;
    let dst = dst.to_path_buf();
    if dst == src {
        anyhow::bail!("that is already where the data lives");
    }
    if dst.starts_with(&src) {
        anyhow::bail!("the new folder cannot be inside the current one");
    }
    std::fs::create_dir_all(&dst).with_context(|| format!("create {}", dst.display()))?;
    writable(&dst)?;

    // A browser still open on a profile (one this app did not start, or left behind) holds its
    // files: said by name before anything is copied, not as a failed copy half-way through.
    if let Ok(rd) = std::fs::read_dir(src.join("user-data")) {
        for e in rd.flatten() {
            if crate::process::Tracker::user_data_in_use(&e.path()) {
                let id = e.file_name().to_string_lossy().to_string();
                let name = crate::profile::load_raw(&id).ok()
                    .and_then(|p| p.config.get("name").and_then(|v| v.as_str().map(String::from)))
                    .unwrap_or_else(|| id.clone());
                anyhow::bail!("profile \"{name}\" is still open in a browser — close it (or end every chrome.exe of Hir-Login in Task Manager), then move again");
            }
        }
    }

    emit(app, Progress { phase: "scan", done: 0, total: 0, percent: 0, current: String::new() });
    let mut files: Vec<(PathBuf, PathBuf)> = Vec::new();
    for name in MOVED {
        let from = src.join(name);
        if from.exists() {
            collect(&from, &dst.join(name), &mut files)?;
        }
    }
    let total = files.len() as u64;
    // Room for all of it before the first byte: a copy that fills the disk half-way leaves
    // nothing deleted, but a full disk under every running program.
    let need: u64 = files.iter().map(|(f, _)| std::fs::metadata(f).map(|m| m.len()).unwrap_or(0)).sum();
    if let Some(free) = crate::winfs::free_space(&dst) {
        // Copied, checked, then deleted: the whole of it must fit, on the same disk too.
        if free < need + need / 20 + 1_000_000_000 {
            anyhow::bail!(
                "not enough space there: {:.1} GB needed, {:.1} GB free",
                need as f64 / 1e9,
                free as f64 / 1e9
            );
        }
    }
    if total == 0 {
        finish(&dst)?;
        emit(app, Progress { phase: "done", done: 0, total: 0, percent: 100, current: String::new() });
        return Ok(0);
    }

    let mut done = 0u64;
    let mut skipped: std::collections::HashSet<PathBuf> = Default::default();
    for (from, to) in &files {
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // An antivirus or the search indexer can hold a file for a moment: tried again a few
        // times. A cache or a tab-restore file that stays locked is left behind (the browser
        // makes it again); anything else stops the move, nothing deleted.
        let mut res = std::fs::copy(from, to);
        for _ in 0..5 {
            if res.is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(400));
            res = std::fs::copy(from, to);
        }
        if let Err(e) = res {
            if rebuilt_by_browser(from) {
                eprintln!("[launcher] move: left behind {} ({e})", from.display());
                skipped.insert(from.clone());
            } else {
                return Err(anyhow::Error::new(e))
                    .with_context(|| format!("copy {} → {}", from.display(), to.display()));
            }
        }
        done += 1;
        // One event per file is noise on a profile with ten thousand cache
        // entries; the bar only needs to move.
        if done % 25 == 0 || done == total {
            emit(app, Progress {
                phase: "copy",
                done,
                total,
                percent: ((done * 100) / total) as u8,
                current: from
                    .strip_prefix(&src)
                    .unwrap_or(from)
                    .display()
                    .to_string(),
            });
        }
    }

    emit(app, Progress { phase: "verify", done, total, percent: 100, current: String::new() });
    for (from, to) in &files {
        if skipped.contains(from) {
            continue;
        }
        let a = std::fs::metadata(from).map(|m| m.len()).unwrap_or(0);
        let b = std::fs::metadata(to).map(|m| m.len()).unwrap_or(u64::MAX);
        if a != b {
            anyhow::bail!("{} did not copy cleanly — nothing was deleted", from.display());
        }
    }

    // Only now is the old copy expendable.
    emit(app, Progress { phase: "cleanup", done, total, percent: 100, current: String::new() });
    finish(&dst)?;
    for name in MOVED {
        let old = src.join(name);
        if old.exists() {
            let _ = std::fs::remove_dir_all(&old);
        }
    }

    emit(app, Progress { phase: "done", done, total, percent: 100, current: String::new() });
    Ok(done)
}

/// Persist the new root and point the process at it.
fn finish(dst: &Path) -> Result<()> {
    let mut s = settings::load()?;
    s.data_root = Some(dst.display().to_string());
    settings::save(&s)?;
    store::set_data_root(Some(dst.to_path_buf()));
    Ok(())
}

/// Fail before copying rather than half-way through: an external disk mounted
/// read-only looks like an ordinary directory until something is written.
fn writable(dir: &Path) -> Result<()> {
    let probe = dir.join(".shardx-write-test");
    std::fs::write(&probe, b"ok")
        .with_context(|| format!("{} is not writable", dir.display()))?;
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

/// Files the browser makes again by itself: caches and the tabs to restore.
fn rebuilt_by_browser(p: &Path) -> bool {
    const DIRS: &[&str] = &["Cache", "Code Cache", "GPUCache", "DawnCache", "DawnGraphiteCache", "DawnWebGPUCache", "GrShaderCache", "GraphiteDawnCache", "ShaderCache", "Sessions", "Crashpad"];
    p.components().any(|c| DIRS.iter().any(|d| c.as_os_str().eq_ignore_ascii_case(d)))
}

fn collect(src: &Path, dst: &Path, out: &mut Vec<(PathBuf, PathBuf)>) -> Result<()> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        match entry.file_type() {
            Ok(t) if t.is_dir() => collect(&from, &to, out)?,
            // Symlinks are followed by copy(); a profile dir has none anyway.
            Ok(_) => out.push((from, to)),
            Err(_) => {}
        }
    }
    Ok(())
}
