use std::process::Command;

/// `Command::new`, but on Windows it won't flash a console window — every one of
/// these is a console-subsystem binary (tailscale.exe), and a GUI app spawning
/// one without this flag gets a visible black window for an instant each time.
fn cmd(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    c
}

fn tailscale_bin() -> Option<String> {
    // Try PATH first
    if cmd("tailscale").arg("version").output().map(|o| o.status.success()).unwrap_or(false) {
        return Some("tailscale".into());
    }
    for p in [
        "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
        "/opt/homebrew/bin/tailscale",
        "/usr/local/bin/tailscale",
    ] {
        if std::path::Path::new(p).exists() {
            // verify it actually runs
            if cmd(p).arg("version").output().map(|o| o.status.success()).unwrap_or(false) {
                return Some(p.into());
            }
            // App Store binary may be there but version fails — still return it for `up`
            return Some(p.into());
        }
    }
    None
}

fn has_100_ip() -> bool {
    #[cfg(unix)]
    {
        if let Ok(out) = Command::new("sh").arg("-c").arg("ifconfig 2>/dev/null | grep -o '100\\.[0-9]*\\.[0-9]*\\.[0-9]*' | head -1").output() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if s.starts_with("100.") && !s.is_empty() { return true; }
        }
    }
    false
}

pub fn is_installed() -> bool {
    tailscale_bin().is_some() || has_100_ip()
}

pub fn is_connected() -> bool {
    if let Some(bin) = tailscale_bin() {
        if let Ok(out) = cmd(&bin).args(["ip", "-4"]).output() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if s.starts_with("100.") { return true; }
        }
    }
    has_100_ip()
}

pub fn tailscale_ip() -> Option<String> {
    if let Some(bin) = tailscale_bin() {
        if let Ok(out) = cmd(&bin).args(["ip", "-4"]).output() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if s.starts_with("100.") { return Some(s); }
        }
    }
    #[cfg(unix)]
    {
        if let Ok(out) = Command::new("sh").arg("-c").arg("ifconfig 2>/dev/null | grep -o '100\\.[0-9]*\\.[0-9]*\\.[0-9]*' | head -1").output() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() { return Some(s); }
        }
    }
    None
}

/// Try to join tailnet using a reusable auth key. Returns Ok(()) on success or already connected.
pub fn join_with_auth_key(auth_key: &str) -> anyhow::Result<()> {
    if auth_key.trim().is_empty() {
        anyhow::bail!("auth key trống");
    }
    if is_connected() {
        return Ok(());
    }
    let bin = tailscale_bin().ok_or_else(|| anyhow::anyhow!("chưa cài Tailscale — tải tại https://tailscale.com/download"))?;
    let out = cmd(&bin)
        .args(["up", "--authkey", auth_key.trim()])
        .output()
        .map_err(|e| anyhow::anyhow!("không chạy được tailscale: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let out_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
        anyhow::bail!("{}", if err.is_empty() { out_str } else { err });
    }
    Ok(())
}
