//! `rename` / `remove_dir_all` that survive a Windows quirk.
//!
//! Rust's std replaces and deletes with POSIX semantics. On some machines
//! (seen with Defender's real-time scanning in %APPDATA%) that fails with
//! "being used by another process" (os error 32) on files nothing visibly holds,
//! while the classic Win32 calls (`MoveFileExW`, `DeleteFileW`) go through.
//! So std stays the first choice and the classic call is only the fallback.

use std::io;
use std::path::Path;

/// `fs::rename` that replaces an existing target.
pub fn rename_replace(from: &Path, to: &Path) -> io::Result<()> {
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(e) => {
            #[cfg(windows)]
            if legacy::move_replace(from, to).is_ok() {
                return Ok(());
            }
            Err(e)
        }
    }
}

/// `fs::remove_dir_all`.
pub fn remove_dir_all(dir: &Path) -> io::Result<()> {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(e) => {
            #[cfg(windows)]
            if legacy::remove_tree(dir).is_ok() {
                return Ok(());
            }
            Err(e)
        }
    }
}

#[cfg(windows)]
mod legacy {
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use windows_sys::Win32::Storage::FileSystem::{
        DeleteFileW, MoveFileExW, RemoveDirectoryW, MOVEFILE_REPLACE_EXISTING,
        MOVEFILE_WRITE_THROUGH,
    };

    fn wide(p: &Path) -> Vec<u16> {
        p.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
    }

    pub fn move_replace(from: &Path, to: &Path) -> io::Result<()> {
        let (f, t) = (wide(from), wide(to));
        // SAFETY: both buffers are NUL-terminated and outlive the call.
        let ok = unsafe {
            MoveFileExW(f.as_ptr(), t.as_ptr(), MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)
        };
        if ok == 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
    }

    fn delete_file(p: &Path) -> io::Result<()> {
        // A read-only file refuses deletion; the std call clears it too.
        if let Ok(meta) = std::fs::symlink_metadata(p) {
            let mut perm = meta.permissions();
            if perm.readonly() {
                #[allow(clippy::permissions_set_readonly_false)]
                perm.set_readonly(false);
                let _ = std::fs::set_permissions(p, perm);
            }
        }
        let w = wide(p);
        // SAFETY: NUL-terminated buffer that outlives the call.
        if unsafe { DeleteFileW(w.as_ptr()) } == 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
    }

    fn remove_dir(p: &Path) -> io::Result<()> {
        let w = wide(p);
        // SAFETY: NUL-terminated buffer that outlives the call.
        if unsafe { RemoveDirectoryW(w.as_ptr()) } == 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
    }

    pub fn remove_tree(dir: &Path) -> io::Result<()> {
        // A symlinked directory is removed as a link, never followed into.
        let is_real_dir = std::fs::symlink_metadata(dir)?.file_type().is_dir();
        if is_real_dir {
            for entry in std::fs::read_dir(dir)? {
                let path = entry?.path();
                let ft = std::fs::symlink_metadata(&path)?.file_type();
                if ft.is_dir() {
                    remove_tree(&path)?;
                } else if ft.is_symlink() && path.is_dir() {
                    remove_dir(&path)?;
                } else {
                    delete_file(&path)?;
                }
            }
        }
        remove_dir(dir)
    }
}
