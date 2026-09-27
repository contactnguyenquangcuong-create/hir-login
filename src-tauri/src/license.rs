//! First-run license activation against a self-hosted Supabase project (see
//! `license-system/` at the repo root). A key binds to exactly one device id
//! on first successful activation; after that, this machine never needs the
//! network again — only a *different* device id trying the same key gets
//! refused, and that check happens server-side in the `activate_license` RPC.
//!
//! The app embeds only the Supabase **anon** key, which can call that one
//! RPC and nothing else (see `license-system/supabase.sql` — RLS blocks
//! direct table access). The admin-only service_role key lives solely in the
//! Telegram bot (`license-system/bot.js`), never in this binary.

use crate::store;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::time::Duration;

// ---- Fill these in once, before building for real distribution ----
// Project Settings → API in the Supabase dashboard. The anon key is meant to
// be public/embedded in clients — it is safe here precisely because RLS
// denies it any direct table access (see supabase.sql).
const SUPABASE_URL: &str = "https://myyqyillsgizshrvoncs.supabase.co";
const SUPABASE_ANON_KEY: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6Im15eXF5aWxsc2dpenNocnZvbmNzIiwicm9sZSI6ImFub24iLCJpYXQiOjE3OTAwMTg3MDIsImV4cCI6MjEwNTU5NDcwMn0.yUlWBPSIJMlf7hoc3eBfRhCCUXryUiqyN3zue7jeaPU";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ActivationRecord {
    key: String,
    device_id: String,
    activated_at: String,
    #[serde(default)]
    customer_name: Option<String>,
    #[serde(default)]
    customer_phone: Option<String>,
    #[serde(default)]
    customer_email: Option<String>,
}

/// What the Settings page shows — same shape, just public and named for the
/// frontend rather than the on-disk record.
#[derive(Debug, Clone, Serialize)]
pub struct LicenseInfo {
    pub key: String,
    pub device_id: String,
    pub activated_at: String,
    pub customer_name: Option<String>,
    pub customer_phone: Option<String>,
    pub customer_email: Option<String>,
}

impl From<ActivationRecord> for LicenseInfo {
    fn from(r: ActivationRecord) -> Self {
        LicenseInfo {
            key: r.key,
            device_id: r.device_id,
            activated_at: r.activated_at,
            customer_name: r.customer_name,
            customer_phone: r.customer_phone,
            customer_email: r.customer_email,
        }
    }
}

fn activation_path() -> Result<std::path::PathBuf> {
    Ok(store::config_root()?.join("activation.json"))
}

/// A stable per-machine id derived from hardware, not the raw serial —
/// hashed so nothing identifying leaves the device other than an opaque tag.
pub fn device_id() -> Result<String> {
    let raw = raw_hardware_id()?;
    let mut hasher = <sha2::Sha256 as sha2::Digest>::new();
    sha2::Digest::update(&mut hasher, b"hir-login-device-v1:");
    sha2::Digest::update(&mut hasher, raw.trim().as_bytes());
    let digest = sha2::Digest::finalize(hasher);
    Ok(digest.iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(target_os = "macos")]
fn raw_hardware_id() -> Result<String> {
    let out = std::process::Command::new("ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
        .context("run ioreg")?;
    let text = String::from_utf8_lossy(&out.stdout);
    // Line looks like: "IOPlatformUUID" = "1F2E3D4C-....."
    const MARKER: &str = "\"IOPlatformUUID\" = \"";
    let start = text.find(MARKER).context("IOPlatformUUID not found in ioreg output")? + MARKER.len();
    let end = text[start..].find('"').context("unterminated IOPlatformUUID value")?;
    Ok(text[start..start + end].to_string())
}

#[cfg(target_os = "windows")]
fn raw_hardware_id() -> Result<String> {
    use std::os::windows::process::CommandExt;
    let out = std::process::Command::new("reg")
        .args(["query", r"HKLM\SOFTWARE\Microsoft\Cryptography", "/v", "MachineGuid"])
        .creation_flags(0x08000000) // CREATE_NO_WINDOW — no console flash
        .output()
        .context("run reg query")?;
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        if let Some(idx) = line.find("MachineGuid") {
            let rest = line[idx..].trim();
            if let Some(v) = rest.split_whitespace().last() {
                return Ok(v.to_string());
            }
        }
    }
    anyhow::bail!("MachineGuid not found")
}

#[cfg(target_os = "linux")]
fn raw_hardware_id() -> Result<String> {
    for p in ["/etc/machine-id", "/var/lib/dbus/machine-id"] {
        if let Ok(s) = fs::read_to_string(p) {
            if !s.trim().is_empty() {
                return Ok(s);
            }
        }
    }
    anyhow::bail!("no machine-id file found")
}

fn local_record() -> Option<ActivationRecord> {
    let path = activation_path().ok()?;
    let body = fs::read_to_string(&path).ok()?;
    let rec: ActivationRecord = serde_json::from_str(&body).ok()?;
    let this_device = device_id().ok()?;
    (rec.device_id == this_device).then_some(rec)
}

/// Whether this machine already has a valid local activation record.
/// Offline — no network call. Only meaningful together with the device_id
/// check: a copied activation.json from another machine has a different
/// device_id than this one computes, so it is rejected here already.
pub fn is_activated() -> bool {
    local_record().is_some()
}

/// For the Settings page. None before activation, or if the local record
/// does not belong to this machine.
pub fn local_info() -> Option<LicenseInfo> {
    local_record().map(Into::into)
}

#[derive(Debug, Serialize)]
struct RpcBody<'a> {
    p_key: &'a str,
    p_device_id: &'a str,
}

#[derive(Debug, Deserialize)]
struct RpcResult {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
}

fn friendly_error(code: &str) -> &'static str {
    match code {
        "not_found" => "Key không tồn tại.",
        "revoked" => "Key này đã bị thu hồi.",
        "in_use" => "Key này đã được kích hoạt trên máy khác.",
        "empty_key" => "Vui lòng nhập key.",
        _ => "Kích hoạt thất bại — thử lại sau.",
    }
}

/// Verifies `key` against Supabase and, on success, writes the local
/// activation record so future launches are offline. Network required only
/// for this call.
pub async fn activate(key: &str) -> Result<()> {
    let key = key.trim();
    if key.is_empty() {
        anyhow::bail!(friendly_error("empty_key"));
    }
    let dev = device_id().context("compute device id")?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let resp = client
        .post(format!("{SUPABASE_URL}/rest/v1/rpc/activate_license"))
        .header("apikey", SUPABASE_ANON_KEY)
        .header("Authorization", format!("Bearer {SUPABASE_ANON_KEY}"))
        .json(&RpcBody { p_key: key, p_device_id: &dev })
        .send()
        .await
        .context("contact license server")?;

    if !resp.status().is_success() {
        anyhow::bail!("license server error: {}", resp.status());
    }
    let result: RpcResult = resp.json().await.context("parse license server response")?;
    if !result.ok {
        anyhow::bail!(friendly_error(result.error.as_deref().unwrap_or("")));
    }

    let rec = ActivationRecord {
        key: key.to_string(),
        device_id: dev,
        activated_at: chrono_now_iso(),
        customer_name: None,
        customer_phone: None,
        customer_email: None,
    };
    let path = activation_path()?;
    fs::write(&path, serde_json::to_string_pretty(&rec)?).context("write activation record")?;
    Ok(())
}

#[derive(Debug, Serialize)]
struct CustomerInfoBody<'a> {
    p_key: &'a str,
    p_device_id: &'a str,
    p_name: &'a str,
    p_phone: &'a str,
    p_email: &'a str,
}

/// Sends the customer's name/phone-Zalo/email to Supabase (tied to this
/// machine's own key + device id — see `submit_customer_info` in
/// supabase.sql) and updates the local cache the Settings page reads. Can be
/// called again later to correct a typo; each call overwrites the last.
pub async fn submit_customer_info(name: &str, phone: &str, email: &str) -> Result<()> {
    let mut rec = local_record().context("this machine is not activated")?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let resp = client
        .post(format!("{SUPABASE_URL}/rest/v1/rpc/submit_customer_info"))
        .header("apikey", SUPABASE_ANON_KEY)
        .header("Authorization", format!("Bearer {SUPABASE_ANON_KEY}"))
        .json(&CustomerInfoBody {
            p_key: &rec.key,
            p_device_id: &rec.device_id,
            p_name: name,
            p_phone: phone,
            p_email: email,
        })
        .send()
        .await
        .context("contact license server")?;

    if !resp.status().is_success() {
        anyhow::bail!("license server error: {}", resp.status());
    }
    let result: RpcResult = resp.json().await.context("parse license server response")?;
    if !result.ok {
        anyhow::bail!(friendly_error(result.error.as_deref().unwrap_or("")));
    }

    rec.customer_name = (!name.trim().is_empty()).then(|| name.trim().to_string());
    rec.customer_phone = (!phone.trim().is_empty()).then(|| phone.trim().to_string());
    rec.customer_email = (!email.trim().is_empty()).then(|| email.trim().to_string());
    let path = activation_path()?;
    fs::write(&path, serde_json::to_string_pretty(&rec)?).context("write activation record")?;
    Ok(())
}

fn chrono_now_iso() -> String {
    let s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("@{s}")
}
