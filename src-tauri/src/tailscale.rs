use std::process::Command;

pub fn is_installed() -> bool {
    if Command::new("tailscale").arg("version").output().map(|o| o.status.success()).unwrap_or(false) {
        return true;
    }
    // macOS App Store bundle not in PATH
    for p in [
        "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
        "/opt/homebrew/bin/tailscale",
        "/usr/local/bin/tailscale",
    ] {
        if std::path::Path::new(p).exists() {
            return true;
        }
    }
    // fallback: 100.x IP means Tailscale is running
    is_connected()
}

pub fn is_connected() -> bool {
    // tailscale status --json or simple check: tailscale ip -4 returns 100.x
    if let Ok(out) = Command::new("tailscale").args(["ip", "-4"]).output() {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        return s.starts_with("100.");
    }
    false
}

pub fn tailscale_ip() -> Option<String> {
    if let Ok(out) = Command::new("tailscale").args(["ip", "-4"]).output() {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if s.starts_with("100.") { return Some(s); }
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
    if !is_installed() {
        anyhow::bail!("chưa cài Tailscale — tải tại https://tailscale.com/download");
    }
    let out = Command::new("tailscale")
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
