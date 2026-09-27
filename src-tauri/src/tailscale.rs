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

/// One key as Tailscale's API reports it — never the secret value itself,
/// only returned once at creation and not stored anywhere after.
#[derive(serde::Deserialize, serde::Serialize, Clone, Default)]
pub struct KeyMeta {
    pub id: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub expires: String,
    #[serde(default)]
    pub revoked: String,
    #[serde(default)]
    pub invalid: bool,
}

fn http_client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)).build()?)
}

/// `error_for_status` alone throws away the response body — and Tailscale's API
/// puts the actual reason for a 4xx there (e.g. which field it didn't like), not
/// in the status line. This keeps that reason so the error shown in the app says
/// something a person can act on instead of just "400 Bad Request".
async fn ok_body(resp: reqwest::Response) -> anyhow::Result<reqwest::Response> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    let text = resp.text().await.unwrap_or_default();
    let reason = extract_message(&text).unwrap_or(text);
    if reason.trim().is_empty() {
        anyhow::bail!("Tailscale trả lỗi {status}");
    }
    anyhow::bail!("Tailscale trả lỗi {status}: {reason}");
}

/// Tailscale's error bodies are `{"message": "..."}`; pull just that out when
/// present so the app doesn't dump raw JSON at the operator.
fn extract_message(body: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()?
        .get("message")?
        .as_str()
        .map(str::to_string)
}

async fn oauth_token(client: &reqwest::Client, client_id: &str, client_secret: &str) -> anyhow::Result<String> {
    if client_id.trim().is_empty() || client_secret.trim().is_empty() {
        anyhow::bail!("chưa cấu hình OAuth Client (Client ID/Secret)");
    }
    let resp = client
        .post("https://api.tailscale.com/api/v2/oauth/token")
        .form(&[("grant_type", "client_credentials"), ("client_id", client_id), ("client_secret", client_secret)])
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("không gọi được Tailscale: {e}"))?;
    let resp = ok_body(resp).await.map_err(|e| anyhow::anyhow!("Tailscale từ chối Client ID/Secret — {e}"))?;
    let tok: TokenResp = resp.json().await.map_err(|e| anyhow::anyhow!("phản hồi lấy access token không đọc được: {e}"))?;
    Ok(tok.access_token)
}

fn normalize_tag(tag: &str) -> String {
    let t = if tag.trim().is_empty() { "hirlogin".to_string() } else { tag.trim().trim_start_matches("tag:").to_string() };
    format!("tag:{t}")
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
    let client = http_client()?;
    let access_token = oauth_token(&client, client_id, client_secret).await?;
    let tag = normalize_tag(tag);

    let body = serde_json::json!({
        "capabilities": { "devices": { "create": {
            "reusable": true, "ephemeral": false, "preauthorized": true, "tags": [tag],
        } } },
        "expirySeconds": expiry_seconds,
        "description": description,
    });
    let resp = client
        .post("https://api.tailscale.com/api/v2/tailnet/-/keys")
        .bearer_auth(&access_token)
        .json(&body)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("không tạo được key: {e}"))?;
    let resp = ok_body(resp).await.map_err(|e| anyhow::anyhow!("{e} (kiểm tra thẻ {tag} đã gán cho OAuth client chưa)"))?;
    let key: KeyResp = resp.json().await.map_err(|e| anyhow::anyhow!("phản hồi tạo key không đọc được: {e}"))?;

    Ok(key.key)
}

/// Every key in the tailnet, newest first. The bulk list endpoint sometimes
/// answers with only an id per entry, so a key missing its description is
/// backfilled with one extra call — the team is small, this stays cheap.
pub async fn list_keys(client_id: &str, client_secret: &str) -> anyhow::Result<Vec<KeyMeta>> {
    let client = http_client()?;
    let access_token = oauth_token(&client, client_id, client_secret).await?;

    #[derive(serde::Deserialize)]
    struct ListResp {
        #[serde(default)]
        keys: Vec<KeyMeta>,
    }
    let resp = client
        .get("https://api.tailscale.com/api/v2/tailnet/-/keys")
        .bearer_auth(&access_token)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("không lấy được danh sách key: {e}"))?;
    let resp = ok_body(resp).await?;
    let resp: ListResp = resp.json().await.map_err(|e| anyhow::anyhow!("phản hồi danh sách key không đọc được: {e}"))?;

    let mut out = Vec::with_capacity(resp.keys.len());
    for k in resp.keys {
        if k.description.is_empty() && !k.id.is_empty() {
            let resp = client
                .get(format!("https://api.tailscale.com/api/v2/tailnet/-/keys/{}", k.id))
                .bearer_auth(&access_token)
                .send()
                .await
                .ok()
                .and_then(|r| r.error_for_status().ok());
            let full: Option<KeyMeta> = match resp { Some(r) => r.json().await.ok(), None => None };
            out.push(full.unwrap_or(k));
        } else {
            out.push(k);
        }
    }
    out.sort_by(|a, b| b.created.cmp(&a.created));
    Ok(out)
}

/// Revoke one key by id — immediate, cannot be undone.
pub async fn revoke_key(client_id: &str, client_secret: &str, key_id: &str) -> anyhow::Result<()> {
    let client = http_client()?;
    let access_token = oauth_token(&client, client_id, client_secret).await?;
    let resp = client
        .delete(format!("https://api.tailscale.com/api/v2/tailnet/-/keys/{key_id}"))
        .bearer_auth(&access_token)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("không thu hồi được key: {e}"))?;
    ok_body(resp).await?;
    Ok(())
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
    fn error_body_message_is_extracted_from_json() {
        assert_eq!(extract_message(r#"{"message":"requested tags are invalid or not permitted"}"#).as_deref(), Some("requested tags are invalid or not permitted"));
        assert_eq!(extract_message("not json at all"), None);
        assert_eq!(extract_message(""), None);
        assert_eq!(extract_message(r#"{"other":"field"}"#), None);
    }

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
