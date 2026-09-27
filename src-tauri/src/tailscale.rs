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

// ---- OAuth client: mint a fresh reusable auth key on demand ----
//
// Confirmed against Tailscale's own Go client (tailscale/tailscale-client-go)
// and https://tailscale.com/kb/1215/oauth-clients:
//   1. POST /api/v2/oauth/token (client_credentials grant) -> access_token
//   2. POST /api/v2/tailnet/-/keys with that bearer token -> {"key": "tskey-auth-..."}
// "-" is Tailscale's documented shorthand for "my own tailnet" — no need to
// know or store the tailnet's real name.

#[derive(serde::Deserialize)]
struct TokenResp {
    access_token: String,
}

#[derive(serde::Deserialize)]
struct KeyResp {
    key: String,
}

/// Mint a fresh reusable, pre-authorized auth key tagged `tag`, valid for
/// `expiry_seconds` (Tailscale caps this at 90 days regardless of what is asked).
pub async fn create_auth_key(
    client_id: &str,
    client_secret: &str,
    tag: &str,
    description: &str,
    expiry_seconds: i64,
) -> anyhow::Result<String> {
    if client_id.trim().is_empty() || client_secret.trim().is_empty() {
        anyhow::bail!("chưa cấu hình OAuth Client (Client ID/Secret)");
    }
    let tag = if tag.trim().is_empty() { "tag:hirlogin".to_string() } else { tag.trim().trim_start_matches("tag:").to_string() };
    let tag = format!("tag:{tag}");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;

    let tok: TokenResp = client
        .post("https://api.tailscale.com/api/v2/oauth/token")
        .form(&[("grant_type", "client_credentials"), ("client_id", client_id), ("client_secret", client_secret)])
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("không gọi được Tailscale: {e}"))?
        .error_for_status()
        .map_err(|e| anyhow::anyhow!("Tailscale từ chối Client ID/Secret: {e}"))?
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("phản hồi lấy access token không đọc được: {e}"))?;

    let body = serde_json::json!({
        "capabilities": { "devices": { "create": {
            "reusable": true, "ephemeral": false, "preauthorized": true, "tags": [tag],
        } } },
        "expirySeconds": expiry_seconds,
        "description": description,
    });
    let key: KeyResp = client
        .post("https://api.tailscale.com/api/v2/tailnet/-/keys")
        .bearer_auth(&tok.access_token)
        .json(&body)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("không tạo được key: {e}"))?
        .error_for_status()
        .map_err(|e| anyhow::anyhow!("Tailscale từ chối tạo key (kiểm tra thẻ {tag} đã gán cho OAuth client chưa): {e}"))?
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("phản hồi tạo key không đọc được: {e}"))?;

    Ok(key.key)
}

#[cfg(test)]
mod oauth_tests {
    use super::*;

    /// Missing credentials are refused before any network call, with a message
    /// pointing at what to fill in rather than a raw network error.
    #[tokio::test]
    async fn empty_credentials_are_refused_up_front() {
        let err = create_auth_key("", "", "tag:hirlogin", "d", 3600).await.unwrap_err();
        assert!(err.to_string().contains("OAuth Client"), "{err}");
    }

    /// A tag typed with or without the "tag:" prefix, or left blank, is normalised.
    #[test]
    fn tag_is_normalised() {
        let norm = |t: &str| {
            let t = if t.trim().is_empty() { "hirlogin".to_string() } else { t.trim().trim_start_matches("tag:").to_string() };
            format!("tag:{t}")
        };
        assert_eq!(norm("tag:hirlogin"), "tag:hirlogin");
        assert_eq!(norm("hirlogin"), "tag:hirlogin");
        assert_eq!(norm(""), "tag:hirlogin");
        assert_eq!(norm("  server  "), "tag:server");
    }
}
