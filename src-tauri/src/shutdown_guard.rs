//! Asks Windows to wait while profiles are still being saved to the team.
//!
//! A person who closes the last browser and shuts the computer down at once cuts the upload of
//! that close short: the server keeps the older copy, and the next open elsewhere is logged out.
//! While anything is being saved, the system is told (with a reason it shows on its shutdown
//! screen) that Hir-Login is not through yet; when the saving ends, the block is lifted. On other
//! systems the quit dialog and the banner in the window are what there is.

/// `working` = how many profiles are being saved; 0 lifts the block.
pub fn update(working: usize) {
    #[cfg(windows)]
    windows_impl::update(working);
    #[cfg(not(windows))]
    let _ = working;
}

#[cfg(windows)]
mod windows_impl {
    use std::sync::atomic::{AtomicBool, Ordering};
    use tauri::Manager;
    use windows_sys::Win32::System::Shutdown::{ShutdownBlockReasonCreate, ShutdownBlockReasonDestroy};

    static BLOCKED: AtomicBool = AtomicBool::new(false);

    pub fn update(working: usize) {
        let Some(app) = crate::app_handle() else { return };
        let Some(window) = app.get_webview_window("main") else { return };
        let Ok(hwnd) = window.hwnd() else { return };
        let hwnd = hwnd.0 as windows_sys::Win32::Foundation::HWND;
        if working > 0 {
            let reason = format!("Hir-Login đang đồng bộ {working} profile lên nhóm. Đợi xong rồi hãy tắt máy để không mất đăng nhập.");
            let wide: Vec<u16> = reason.encode_utf16().chain(std::iter::once(0)).collect();
            // SAFETY: `hwnd` is this process's own main window and `wide` is NUL-terminated.
            let ok = unsafe { ShutdownBlockReasonCreate(hwnd, wide.as_ptr()) };
            BLOCKED.store(ok != 0, Ordering::Relaxed);
        } else if BLOCKED.swap(false, Ordering::Relaxed) {
            // SAFETY: as above.
            unsafe { ShutdownBlockReasonDestroy(hwnd) };
        }
    }
}
