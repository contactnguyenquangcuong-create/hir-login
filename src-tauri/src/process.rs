// Tracker for launched ShardX child processes; keyed by profile_id.

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;
use tokio::process::Child;

pub struct Tracker {
    inner: Mutex<HashMap<String, ChildEntry>>,
}

struct ChildEntry {
    pid: u32,
    killer: tokio::sync::mpsc::Sender<()>,
    /// Set once DevToolsActivePort is read; None for UI launches.
    cdp: Option<CdpInfo>,
    /// Process start; serialised as elapsed ms in RunningProfile.
    started_at: Instant,
}

/// CDP endpoint for an API-launched profile.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CdpInfo {
    pub port: u16,
    pub http_url: String,
    /// ws://127.0.0.1:<port>/devtools/browser/<id> for Puppeteer/Playwright.
    pub web_socket_debugger_url: String,
}

/// Profiles whose browser has exited and whose close is still being saved to the team (the upload
/// after the exit). They no longer count as running, but the app must not quit, nor the computer
/// shut down, until they are through: an upload cut short leaves the server with the older copy.
fn closing_set() -> &'static Mutex<std::collections::HashSet<String>> {
    static S: std::sync::OnceLock<Mutex<std::collections::HashSet<String>>> = std::sync::OnceLock::new();
    S.get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}

/// Which profiles are being saved after their browser closed.
pub fn closing_ids() -> Vec<String> {
    closing_set().lock().map(|s| s.iter().cloned().collect()).unwrap_or_default()
}

/// Whether this profile's close is still being saved.
pub fn is_closing(id: &str) -> bool {
    closing_set().lock().map(|s| s.contains(id)).unwrap_or(false)
}

/// Ask a browser to quit through its DevTools endpoint (`Browser.close`). True
/// once the browser has taken the request; false if it could not be reached in
/// 2 s, and the caller falls back to WM_CLOSE / SIGTERM.
async fn cdp_browser_close(ws_url: &str) -> bool {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    let attempt = async {
        let (mut ws, _) = tokio_tungstenite::connect_async(ws_url).await.ok()?;
        ws.send(Message::Text(r#"{"id":1,"method":"Browser.close"}"#.into())).await.ok()?;
        // The reply, or the socket dropping as the browser goes, means it is on its way out.
        let _ = ws.next().await;
        Some(())
    };
    tokio::time::timeout(std::time::Duration::from_secs(2), attempt)
        .await
        .ok()
        .flatten()
        .is_some()
}

pub(crate) fn closing_begin(id: &str) {
    if let Ok(mut s) = closing_set().lock() {
        s.insert(id.to_string());
    }
    crate::cloud_sync::notify_working();
}

pub(crate) fn closing_end(id: &str) {
    if let Ok(mut s) = closing_set().lock() {
        s.remove(id);
    }
    crate::cloud_sync::notify_working();
}

impl Tracker {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// Take a spawned child + monitor it; entry removed on exit/kill.
    pub fn track(self: &'static Self, profile_id: String, mut child: Child, temporary: bool) -> u32 {
        let pid = child.id().unwrap_or(0);
        let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(1);

        {
            let mut g = self.inner.lock().unwrap();
            let previous = g.insert(
                profile_id.clone(),
                ChildEntry { pid, killer: tx, cdp: None, started_at: Instant::now() },
            );
            // Replacing the entry drops the old killer and orphans that browser.
            // `launch` refuses a second launch up front; this is the backstop.
            if let Some(prev) = previous {
                eprintln!(
                    "[tracker] {profile_id} was already tracked as pid {}; stopping it",
                    prev.pid
                );
                let _ = prev.killer.try_send(());
            }
        }

        // Graceful shutdown (SIGTERM / taskkill WM_CLOSE) → 5s → hard kill.
        // Graceful path flushes session state so next launch skips the restore prompt.
        let started_at = Instant::now();
        // While the browser runs, keep a second copy of its login every few minutes — the cookie
        // file can be lost with a computer that is switched off under it (see `cookies::snapshot`).
        if !temporary {
            let id = profile_id.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(180)).await;
                    if !Self::shared().is_running(&id) {
                        break;
                    }
                    let id2 = id.clone();
                    let _ = tokio::task::spawn_blocking(move || crate::cookies::snapshot(&id2)).await;
                }
            });
        }
        tokio::spawn(async move {
            // How it ended, for the close line in the sync log: "a profile closes by itself" could
            // not be told apart from a person closing it, or from the app stopping it.
            let how: String = tokio::select! {
                st = child.wait() => match st.ok().and_then(|s| s.code()) {
                    Some(0) => "the browser closed (its window was closed, or it quit)".to_string(),
                    Some(c) => format!("the browser exited by itself with code {c} (0x{:08X}) — a crash, not a close", c as u32),
                    None => "the browser ended without an exit code".to_string(),
                },
                _ = rx.recv() => {
                    // A browser with a DevTools endpoint is asked to quit through it: the
                    // same orderly shutdown as closing its last window, and the only one a
                    // headless browser hears — it has no window for WM_CLOSE, so every stop
                    // used to sit out the full 5 s below before the hard kill.
                    let closed_by_cdp = match Self::shared().cdp(&profile_id) {
                        Some(c) => cdp_browser_close(&c.web_socket_debugger_url).await,
                        None => false,
                    };
                    #[cfg(unix)]
                    if !closed_by_cdp {
                        if let Some(p) = child.id() {
                            // SAFETY: libc::kill on a child pid we own.
                            unsafe { libc::kill(p as libc::pid_t, libc::SIGTERM); }
                        }
                    }
                    #[cfg(windows)]
                    if !closed_by_cdp {
                        use std::os::windows::process::CommandExt;
                        if let Some(p) = child.id() {
                            // taskkill /PID without /F posts WM_CLOSE for clean shutdown.
                            // 0x08000000 = CREATE_NO_WINDOW — suppress the console flash.
                            let _ = std::process::Command::new("taskkill")
                                .args(["/PID", &p.to_string()])
                                .creation_flags(0x08000000)
                                .stdout(std::process::Stdio::null())
                                .stderr(std::process::Stdio::null())
                                .status();
                        }
                    }
                    let graceful = tokio::time::timeout(
                        std::time::Duration::from_secs(5),
                        child.wait(),
                    ).await;
                    if graceful.is_err() {
                        let _ = child.kill().await;
                        let _ = child.wait().await;
                    }
                    "stopped from the app (the Stop button, an automation run or the API)".to_string()
                }
            };
            // From here until the close has been saved, the profile counts as "being saved".
            closing_begin(&profile_id);
            if let Ok(mut g) = Self::shared().inner.lock() {
                g.remove(&profile_id);
            }
            crate::proxy_relay::stop(&profile_id);
            // The cookie file as the browser left it: with the line written at launch, this
            // places a vanished login — gone before the browser started, or while it ran.
            if !temporary {
                // On a Mac: which key the browser sealed what it kept with. A different one from
                // the one assumed means the logins restored at launch were unreadable to it and
                // dropped — the server still has them, so every profile pulls in full next time.
                if let Some(note) = crate::cookies::learn_engine_key(&profile_id) {
                    crate::cloud_sync::log_line(&format!("close {profile_id}: {note}"));
                    crate::cloud_sync::force_repull();
                }
                let _ = crate::cookies::snapshot(&profile_id);
                crate::cloud_sync::log_line(&format!("close {profile_id}: ran {} s, {how} — {}", started_at.elapsed().as_secs(), crate::cookies::census(&profile_id)));
            }
            // Bump the persisted total runtime; non-temporary only (temp
            // profiles get deleted next line so their counter is moot).
            if !temporary {
                let elapsed_ms = started_at.elapsed().as_millis() as u64;
                if let Err(e) = crate::profile::add_runtime(&profile_id, elapsed_ms) {
                    eprintln!("[launcher] add_runtime({profile_id}) failed: {e}");
                }
                // No-op unless Team Sync is configured; otherwise pushes this
                // close's state up and frees the lock for the next device.
                // Best effort: a failed push leaves the lock held rather than
                // risk another device grabbing a stale profile mid-upload.
                if let Err(e) = crate::cloud_sync::checkin(&profile_id).await {
                    eprintln!("[launcher] cloud_sync checkin({profile_id}) failed: {e:#}");
                }
            }
            closing_end(&profile_id);
            // Tear down temporary profile (config + udd) on close.
            if temporary {
                match crate::profile::delete(&profile_id) {
                    Ok(()) => eprintln!("[launcher] temporary profile {profile_id} deleted on close"),
                    Err(e) => eprintln!("[launcher] temporary profile {profile_id} cleanup failed: {e}"),
                }
            }
        });

        pid
    }

    /// Attach CDP to a tracked profile; no-op if the profile already exited.
    pub fn set_cdp(&self, profile_id: &str, cdp: CdpInfo) {
        if let Ok(mut g) = self.inner.lock() {
            if let Some(e) = g.get_mut(profile_id) {
                e.cdp = Some(cdp);
            }
        }
    }

    /// CDP endpoint when the profile was launched with remote debugging.
    /// Whether a child is live for this profile.
    pub fn is_running(&self, profile_id: &str) -> bool {
        self.inner
            .lock()
            .map(|g| g.contains_key(profile_id))
            .unwrap_or(false)
    }

    pub fn cdp(&self, profile_id: &str) -> Option<CdpInfo> {
        self.inner.lock().ok()?.get(profile_id)?.cdp.clone()
    }

    pub fn running(&self) -> Vec<RunningProfile> {
        let g = self.inner.lock().unwrap();
        g.iter()
            .map(|(id, e)| RunningProfile {
                profile_id: id.clone(),
                pid: e.pid,
                cdp: e.cdp.clone(),
                uptime_ms: e.started_at.elapsed().as_millis() as u64,
            })
            .collect()
    }

    /// Stops the profile's browser and answers once it has really gone and its
    /// close is saved (at most 15 s), so "stop, then delete / restart / copy" never
    /// races a browser that still holds its files.
    pub async fn kill(&self, profile_id: &str) -> Result<bool> {
        let killer = {
            let g = self.inner.lock().unwrap();
            g.get(profile_id).map(|e| e.killer.clone())
        };
        let Some(k) = killer else { return Ok(false) };
        let _ = k.send(()).await;
        let until = Instant::now() + std::time::Duration::from_secs(15);
        while Instant::now() < until && (self.is_running(profile_id) || is_closing(profile_id)) {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        Ok(true)
    }

    /// Whether some browser has this profile's data open right now — this app's, or one started
    /// by another copy of the app on the same machine, which this tracker knows nothing about.
    /// Chromium holds `lockfile` in the user-data folder for as long as it runs (Windows), or
    /// leaves a `SingletonLock` link (macOS / Linux).
    pub fn user_data_in_use(udd: &std::path::Path) -> bool {
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            let lock = udd.join("lockfile");
            if !lock.exists() {
                return false;
            }
            // Opened with no sharing: refused (32 / 33) while the browser holds it.
            match std::fs::OpenOptions::new().read(true).write(true).share_mode(0).open(&lock) {
                Ok(_) => false,
                Err(e) => matches!(e.raw_os_error(), Some(32) | Some(33)),
            }
        }
        #[cfg(not(windows))]
        {
            std::fs::symlink_metadata(udd.join("SingletonLock")).is_ok()
        }
    }

    pub fn shared() -> &'static Tracker {
        static INSTANCE: std::sync::OnceLock<Tracker> = std::sync::OnceLock::new();
        INSTANCE.get_or_init(Tracker::new)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RunningProfile {
    pub profile_id: String,
    pub pid: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cdp: Option<CdpInfo>,
    /// Milliseconds since the engine was spawned; frontend formats as
    /// "1h 23m" / "12m 30s" / "45s".
    pub uptime_ms: u64,
}

#[cfg(test)]
mod in_use_tests {
    use super::*;

    /// A profile folder whose `lockfile` a browser holds is in use; with no lockfile, or one
    /// nobody holds (left behind by a browser that crashed), it is not.
    #[cfg(windows)]
    #[test]
    fn a_held_lockfile_means_a_browser_has_the_profile_open() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = std::env::temp_dir().join(format!("hir-inuse-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!Tracker::user_data_in_use(&dir), "no lockfile: not in use");
        let lock = dir.join("lockfile");
        std::fs::write(&lock, b"").unwrap();
        assert!(!Tracker::user_data_in_use(&dir), "a lockfile nobody holds: not in use");
        let held = std::fs::OpenOptions::new().read(true).write(true).share_mode(0).open(&lock).unwrap();
        assert!(Tracker::user_data_in_use(&dir), "held the way the browser holds it: in use");
        drop(held);
        assert!(!Tracker::user_data_in_use(&dir), "released: not in use");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
