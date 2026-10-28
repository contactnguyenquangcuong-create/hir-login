//! Browsers this app started that outlived it.
//!
//! Hir-Login's installer ends the running app to replace it — on every update — without going
//! through the quit that closes and saves the open profiles. The browsers it had started kept
//! running with nobody tracking them: a profile looked closed while its window was still open,
//! its team lock was no longer renewed, its close was never saved, and a tool that opened and
//! closed profiles all day (HirS) left a pile of them eating memory (seen on a 16 GB machine:
//! fourteen browsers, ~10 GB, several of them blank windows nobody used any more).
//!
//! At start-up the app looks for browsers of the engine it runs whose `--user-data-dir` is one of
//! its own profiles, and takes them back: tracked as running again (their close is saved and
//! their lock renewed as for any profile it opened), their debugging port picked up again for the
//! tool that drives them, and the local proxy forwarder they were pointed at started again on the
//! same port.

use crate::{process::CdpInfo, process::Tracker, profile, store};
use std::path::{Path, PathBuf};

/// One running engine process: its id and command line.
struct EngineProcess {
    pid: u32,
    cmdline: String,
}

/// Takes back every browser of this app's profiles that runs without being tracked. Returns how
/// many it took back.
pub async fn adopt_leftovers() -> usize {
    let Ok(root) = store::user_data_root() else { return 0 };
    let Ok(bin) = crate::launch::resolve_binary() else { return 0 };
    let procs = tokio::task::spawn_blocking(move || list_engine_processes(&bin)).await.unwrap_or_default();
    let mut adopted = 0;
    for p in procs {
        // Only the browser process itself: its helpers (renderers, GPU…) go with it.
        if p.cmdline.contains("--type=") {
            continue;
        }
        let Some(udd) = user_data_dir_of(&p.cmdline) else { continue };
        if !same_dir(udd.parent().unwrap_or(Path::new("")), &root) {
            continue;
        }
        let Some(id) = udd.file_name().map(|n| n.to_string_lossy().to_string()) else { continue };
        if Tracker::shared().is_running(&id) {
            continue;
        }
        let Ok(stored) = profile::load_raw(&id) else { continue };
        let cdp = live_cdp(&udd).await;
        let driven = cdp.is_some();
        if !Tracker::shared().adopt(id.clone(), p.pid, stored.meta.temporary, cdp) {
            continue;
        }
        adopted += 1;
        // The forwarder that added the proxy's login lived in the app that ended: start it again on
        // the port the browser still points at, for the profile's own proxy.
        let mut relay = "";
        if let Some(port) = loopback_proxy_port(&p.cmdline) {
            // Resolved as the launch resolves it: the bound proxy, else the profile's inline one.
            let px = crate::profile::launch_proxy(&stored.meta);
            if let Some(px) = px {
                let px = crate::proxy::effective(&px).await;
                relay = match crate::proxy_relay::start_on(&id, &px, port).await {
                    Ok(_) => ", its proxy forwarder started again",
                    Err(_) => ", but its proxy forwarder could not be started again (the port is taken)",
                };
            }
        }
        if !stored.meta.temporary {
            crate::cloud_sync::resume_lock(&id);
        }
        crate::cloud_sync::log_line(&format!(
            "adopt {id}: its browser (pid {}) was left running by an earlier Hir-Login — tracked again{}{relay}",
            p.pid,
            if driven { ", with its debugging port" } else { "" },
        ));
    }
    if adopted > 0 {
        crate::notify_store_changed("profiles");
    }
    adopted
}

/// Ends every engine process still running on the profile in `udd` once its browser has gone —
/// renderers and the GPU process left behind keep their memory commit for good. Returns how many.
pub fn end_helpers_of(udd: &Path) -> usize {
    let Ok(bin) = crate::launch::resolve_binary() else { return 0 };
    let mut n = 0;
    for p in list_engine_processes(&bin) {
        if user_data_dir_of(&p.cmdline).is_some_and(|d| same_dir(&d, udd)) {
            crate::process::end_process(p.pid, true);
            n += 1;
        }
    }
    n
}

/// Whether any process runs from the engine right now — a browser of a profile, this app's or one
/// left by an earlier run. Its files cannot be replaced then (and moving the folder would make it
/// start new tabs from another version), so an engine update waits.
pub fn engine_in_use() -> bool {
    let Ok(bin) = crate::launch::resolve_binary() else { return false };
    !list_engine_processes(&bin).is_empty()
}

/// The engine's processes, by executable.
fn list_engine_processes(bin: &Path) -> Vec<EngineProcess> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let name = bin.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let exe = bin.to_string_lossy().replace('\'', "''");
        let script = format!(
            "Get-CimInstance Win32_Process -Filter \"Name='{}'\" | Where-Object {{ $_.ExecutablePath -eq '{exe}' }} | ForEach-Object {{ \"$($_.ProcessId)`t$($_.CommandLine)\" }}",
            name.replace('\'', "''"),
        );
        let out = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .output();
        let Ok(out) = out else { return Vec::new() };
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| {
                let (pid, cmd) = l.split_once('\t')?;
                Some(EngineProcess { pid: pid.trim().parse().ok()?, cmdline: cmd.to_string() })
            })
            .collect()
    }
    #[cfg(not(windows))]
    {
        let exe = bin.to_string_lossy().to_string();
        let Ok(out) = std::process::Command::new("ps").args(["-axww", "-o", "pid=,command="]).output() else { return Vec::new() };
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| {
                let l = l.trim_start();
                let (pid, cmd) = l.split_once(' ')?;
                if !cmd.starts_with(&exe) {
                    return None;
                }
                Some(EngineProcess { pid: pid.parse().ok()?, cmdline: cmd.to_string() })
            })
            .collect()
    }
}

/// The `--user-data-dir=` of a command line, quoted or not.
fn user_data_dir_of(cmdline: &str) -> Option<PathBuf> {
    let at = cmdline.find("--user-data-dir=")? + "--user-data-dir=".len();
    let rest = &cmdline[at..];
    let quoted = cmdline[..at - "--user-data-dir=".len()].ends_with('"');
    let end = if quoted { rest.find('"') } else { rest.find(" --").or_else(|| rest.find('"')) }.unwrap_or(rest.len());
    let v = rest[..end].trim().trim_matches('"');
    (!v.is_empty()).then(|| PathBuf::from(v))
}

/// The port of a `--proxy-server=http://127.0.0.1:<port>` (this app's forwarder), if that is it.
fn loopback_proxy_port(cmdline: &str) -> Option<u16> {
    let at = cmdline.find("--proxy-server=")? + "--proxy-server=".len();
    let v = cmdline[at..].split([' ', '"']).next()?;
    let hostport = v.strip_prefix("http://").unwrap_or(v);
    let port = hostport.strip_prefix("127.0.0.1:")?;
    port.trim_end_matches('/').parse().ok()
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| p.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase();
    if cfg!(windows) { norm(a) == norm(b) } else { a == b }
}

/// The browser's debugging endpoint, when it has one that answers (the file can be left from an
/// earlier run of the profile).
async fn live_cdp(udd: &Path) -> Option<CdpInfo> {
    let txt = std::fs::read_to_string(udd.join("DevToolsActivePort")).ok()?;
    let mut lines = txt.lines();
    let port: u16 = lines.next()?.trim().parse().ok()?;
    let path = lines.next()?.trim().to_string();
    let up = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        tokio::net::TcpStream::connect(("127.0.0.1", port)),
    )
    .await
    .map(|r| r.is_ok())
    .unwrap_or(false);
    up.then(|| CdpInfo {
        port,
        http_url: format!("http://127.0.0.1:{port}"),
        web_socket_debugger_url: format!("ws://127.0.0.1:{port}{path}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_data_dir_and_the_forwarder_port_from_a_command_line() {
        let plain = r#"C:\x\chrome.exe --fingerprint-profile=a --user-data-dir=C:\Users\u\AppData\Roaming\hir-login\user-data\0ea9b690-5fae-41ea-ae83-17679964639f --proxy-server=http://127.0.0.1:51234 about:blank"#;
        assert_eq!(
            user_data_dir_of(plain).unwrap(),
            PathBuf::from(r"C:\Users\u\AppData\Roaming\hir-login\user-data\0ea9b690-5fae-41ea-ae83-17679964639f")
        );
        assert_eq!(loopback_proxy_port(plain), Some(51234));
        let quoted = r#""C:\x\chrome.exe" "--user-data-dir=C:\Users\An Nguyen\AppData\Roaming\hir-login\user-data\abc" --headless=new"#;
        assert_eq!(user_data_dir_of(quoted).unwrap(), PathBuf::from(r"C:\Users\An Nguyen\AppData\Roaming\hir-login\user-data\abc"));
        assert_eq!(loopback_proxy_port(quoted), None);
        assert_eq!(loopback_proxy_port("--proxy-server=socks5://1.2.3.4:1080"), None);
        #[cfg(windows)]
        assert!(same_dir(Path::new(r"C:\A\user-data\"), Path::new("c:/a/USER-DATA")));
    }
}
