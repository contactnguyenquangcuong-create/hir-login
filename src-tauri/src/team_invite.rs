use anyhow::{bail, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};

const PREFIX: &str = "HIR-";

#[derive(Serialize, Deserialize)]
struct InvitePayload {
    url: String,
    token: String,
    /// Optional reusable Tailscale auth key — lets members auto-join without a separate invite link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auth_key: Option<String>,
}

pub fn generate_invite_code(server_url: &str, token: &str) -> String {
    generate_invite_code_with_auth(server_url, token, None)
}

pub fn generate_invite_code_with_auth(server_url: &str, token: &str, auth_key: Option<&str>) -> String {
    let payload = InvitePayload {
        url: server_url.trim_end_matches('/').to_string(),
        token: token.to_string(),
        auth_key: auth_key.filter(|s| !s.trim().is_empty()).map(|s| s.trim().to_string()),
    };
    let json = serde_json::to_string(&payload).unwrap_or_default();
    let encoded = URL_SAFE_NO_PAD.encode(json.as_bytes());
    let chunks: Vec<String> = encoded
        .as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).to_string())
        .collect();
    format!("{PREFIX}{}", chunks.join("-"))
}

/// Returns (url, token, auth_key)
pub fn parse_invite_code(code: &str) -> Result<(String, String, Option<String>)> {
    let raw = code.trim().to_uppercase().replace(' ', "");
    let stripped = raw
        .strip_prefix(PREFIX)
        .unwrap_or(raw.as_str())
        .replace('-', "");
    if stripped.is_empty() {
        bail!("mã team trống");
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(&stripped)
        .map_err(|_| anyhow::anyhow!("mã team không hợp lệ"))?;
    let payload: InvitePayload = serde_json::from_slice(&bytes)
        .map_err(|_| anyhow::anyhow!("mã team không hợp lệ"))?;
    if payload.url.is_empty() || payload.token.is_empty() {
        bail!("mã team thiếu thông tin server");
    }
    Ok((payload.url, payload.token, payload.auth_key))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_basic() {
        let code = generate_invite_code("http://100.64.0.1:8787", "my-secret-token-123");
        let (url, token, auth) = parse_invite_code(&code).unwrap();
        assert_eq!(url, "http://100.64.0.1:8787");
        assert_eq!(token, "my-secret-token-123");
        assert!(auth.is_none());
    }
    #[test]
    fn roundtrip_with_auth() {
        let code = generate_invite_code_with_auth("http://100.64.0.1:8787", "tok", Some("tskey-auth-abc123"));
        let (url, token, auth) = parse_invite_code(&code).unwrap();
        assert_eq!(url, "http://100.64.0.1:8787");
        assert_eq!(token, "tok");
        assert_eq!(auth.as_deref(), Some("tskey-auth-abc123"));
    }
    #[test]
    fn parse_with_lowercase_and_spaces() {
        let code = generate_invite_code("http://10.0.0.1:8787", "tok");
        let lower = code.to_lowercase();
        let (url, _, _) = parse_invite_code(&lower).unwrap();
        assert_eq!(url, "http://10.0.0.1:8787");
    }
    #[test]
    fn old_code_still_parses() {
        // Code generated before auth_key existed (no auth_key field)
        let code = generate_invite_code("http://10.0.0.1:8787", "old-token");
        let (url, token, auth) = parse_invite_code(&code).unwrap();
        assert_eq!(url, "http://10.0.0.1:8787");
        assert_eq!(token, "old-token");
        assert!(auth.is_none());
    }
}
