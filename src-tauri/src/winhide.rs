//! Keeps a profile's browser window out of sight while automation drives it.
//!
//! The page is still shown in the studio pane (it is streamed from the browser), so the
//! operator watches it there. The real window is moved far off screen rather than
//! minimised: a minimised window is "hidden" to the browser, which stops painting and
//! slows its timers, while an off-screen one keeps running as normal.

/// Moves every visible top-level window of process `pid` off screen, for a few
/// seconds after launch (the window appears a moment after the process starts, and a
/// restored session can open more than one).
pub fn move_offscreen_soon(pid: u32) {
    #[cfg(windows)]
    {
        std::thread::spawn(move || {
            for _ in 0..50 {
                std::thread::sleep(std::time::Duration::from_millis(300));
                win::move_windows_of(pid);
            }
        });
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
    }
}

/// Brings a browser window that was put out of sight back onto the screen. `false` when
/// there was nothing hidden to bring back (the window was already in view).
pub fn bring_back(pid: u32) -> bool {
    #[cfg(windows)]
    {
        win::bring_back_windows_of(pid) > 0
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        false
    }
}

#[cfg(windows)]
mod win {
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible,
        SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE, SW_HIDE, SW_SHOW, SW_SHOWNA, SWP_NOACTIVATE,
        SWP_NOSIZE, SWP_NOZORDER, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
    };

    const OFFSCREEN: i32 = -32000;

    unsafe extern "system" fn each(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let want = lparam as u32;
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == want && IsWindowVisible(hwnd) != 0 {
            let mut r: RECT = std::mem::zeroed();
            if GetWindowRect(hwnd, &mut r) != 0 && r.left > OFFSCREEN + 100 && (r.right - r.left) > 200 {
                SetWindowPos(hwnd, std::ptr::null_mut(), OFFSCREEN, OFFSCREEN, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
                // No taskbar button either: a button for a window nobody can bring up
                // only invites a click that shows nothing. A tool window gets none; the
                // style only takes effect on a hidden window, so it is shown again right after.
                let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
                if ex & WS_EX_TOOLWINDOW == 0 {
                    ShowWindow(hwnd, SW_HIDE);
                    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ((ex | WS_EX_TOOLWINDOW) & !WS_EX_APPWINDOW) as isize);
                    ShowWindow(hwnd, SW_SHOWNA);
                }
            }
        }
        1
    }

    unsafe extern "system" fn each_back(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let args = &mut *(lparam as *mut (u32, u32));
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == args.0 && IsWindowVisible(hwnd) != 0 {
            let mut r: RECT = std::mem::zeroed();
            if GetWindowRect(hwnd, &mut r) != 0 && r.left <= OFFSCREEN + 100 && (r.right - r.left) > 200 {
                let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
                ShowWindow(hwnd, SW_HIDE);
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ((ex & !WS_EX_TOOLWINDOW) | WS_EX_APPWINDOW) as isize);
                SetWindowPos(hwnd, std::ptr::null_mut(), 80, 60, 0, 0, SWP_NOSIZE | SWP_NOZORDER);
                ShowWindow(hwnd, SW_SHOW);
                SetForegroundWindow(hwnd);
                args.1 += 1;
            }
        }
        1
    }

    /// How many windows of `pid` were brought back.
    pub fn bring_back_windows_of(pid: u32) -> u32 {
        let mut args = (pid, 0u32);
        // SAFETY: `args` outlives the call; `each_back` only touches it through the pointer it is given.
        unsafe {
            EnumWindows(Some(each_back), &mut args as *mut (u32, u32) as LPARAM);
        }
        args.1
    }

    pub fn move_windows_of(pid: u32) {
        // SAFETY: `each` only reads window state and calls SetWindowPos on handles the system gives it.
        unsafe {
            EnumWindows(Some(each), pid as LPARAM);
        }
    }
}
