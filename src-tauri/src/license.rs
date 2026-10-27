//! License keys from the shared HIR key system: keys `HLG-XXXX-XXXX-XXXX`
//! are created and managed by the admin in the Telegram bot @Hirkeys_bot
//! (/menu → Hir-Login, source `D:\AI\HIR-IMG\bot-key.js`). They live in the
//! HIR-IMG Supabase project, in the same `ban_quyen` table as HIR-IMG (HIR-)
//! and HirS (HRS-) keys — products are told apart only by prefix, so this app
//! accepts `HLG-` and nothing else.
//!
//! The app embeds only the Supabase **anon** key, which can call the single
//! RPC `kich_hoat(p_key, p_may, p_ten_may)` and nothing else (RLS blocks all
//! direct table access). That RPC checks lock / expiry / machine count, binds
//! this machine on first use and returns `{ok, loi, het_han, ghi_chu}`.
//!
//! The key is re-checked at startup when the last good check is older than
//! `RECHECK_SECS`, so a key locked, expired or deleted in the bot stops
//! working here. Without network the app keeps running for `OFFLINE_GRACE_SECS`
//! after the last good check.
//!
//! Machines activated before 0.2.2 hold an old `HIR-` key from the previous
//! key system (Supabase myyqyillsgizshrvoncs, RPC `activate_license`). They
//! keep working exactly as before — a local, device-bound record, never sent
//! to the new server (where `HIR-` means a HIR-IMG key and would be refused).
//! Only new machines need an `HLG-` key.

use crate::store;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::time::Duration;

// HIR-IMG Supabase (Project Settings → API). The anon key is meant to be
// public/embedded in clients — RLS denies it any direct table access.
// 27/10/2026: kho key chuyển từ Supabase sang Cloudflare Worker hir-key (cùng giao thức /rest/v1/rpc/…; anon key chỉ để tương thích, Worker bỏ qua).
const SUPABASE_URL: &str = "https://hir-key.hir-contact-vn.workers.dev";
const SUPABASE_ANON_KEY: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6ImJnbXl0a253enRhZ2FoZnBlbWdjIiwicm9sZSI6ImFub24iLCJpYXQiOjE3OTIwOTA2NDYsImV4cCI6MjEwNzY2NjY0Nn0.Z0dXBYn1PVFxkuezzs7sbU86fSQfaaeN_vxq3GQ4_sg";

const PREFIX: &str = "HLG-";
/// Keys of the previous Hir-Login key system (before 0.2.2).
const LEGACY_PREFIX: &str = "HIR-";
const RECHECK_SECS: u64 = 6 * 3600;
const OFFLINE_GRACE_SECS: u64 = 3 * 86400;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ActivationRecord {
    key: String,
    device_id: String,
    activated_at: String,
    /// Unix secs of the last check the server answered `ok`.
    #[serde(default)]
    last_ok: u64,
    /// Expiry from the server (ISO 8601), None = permanent.
    #[serde(default)]
    expires_at: Option<String>,
    /// The admin's note on the key (`ghi_chu` in the bot).
    #[serde(default)]
    note: String,
    /// Set when the server refused the key on a re-check; the record is then
    /// no longer valid but keeps the key so the gate can show it again.
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    customer_name: Option<String>,
    #[serde(default)]
    customer_phone: Option<String>,
    #[serde(default)]
    customer_email: Option<String>,
}

/// What the Settings page and the gate show.
#[derive(Debug, Clone, Serialize)]
pub struct LicenseInfo {
    pub key: String,
    pub device_id: String,
    pub activated_at: String,
    pub expires_at: Option<String>,
    pub note: String,
    pub error: Option<String>,
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
            expires_at: r.expires_at,
            note: r.note,
            error: r.error,
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
/// Read once per run: on Windows it costs a `reg` process, and the API gate
/// asks for it far more often than the machine changes.
pub fn device_id() -> Result<String> {
    static ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    if let Some(id) = ID.get() {
        return Ok(id.clone());
    }
    let id = compute_device_id()?;
    Ok(ID.get_or_init(|| id).clone())
}

fn compute_device_id() -> Result<String> {
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

/// `HLG-ABCD-EFGH-JKLM`: the prefix, then three groups of four A–Z / 0–9.
fn valid_shape(key: &str) -> bool {
    has_shape(key, PREFIX)
}

/// A record made by the old key system on this machine.
fn is_legacy(rec: &ActivationRecord) -> bool {
    has_shape(&rec.key, LEGACY_PREFIX)
}

fn has_shape(key: &str, prefix: &str) -> bool {
    let Some(rest) = key.strip_prefix(prefix) else { return false };
    let groups: Vec<&str> = rest.split('-').collect();
    groups.len() == 3
        && groups
            .iter()
            .all(|g| g.len() == 4 && g.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
}

/// Accepts the key typed with or without dashes ("hlgabcd…" → "HLG-ABCD-…").
fn normalize_key(key: &str) -> String {
    let compact: String = key
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if compact.len() == 15 {
        format!("{}-{}-{}-{}", &compact[..3], &compact[3..7], &compact[7..11], &compact[11..])
    } else {
        key.trim().to_ascii_uppercase()
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Local record that belongs to this machine (a copied activation.json from
/// another machine has a different device id and is ignored).
fn local_record() -> Option<ActivationRecord> {
    let path = activation_path().ok()?;
    let body = fs::read_to_string(&path).ok()?;
    let rec: ActivationRecord = serde_json::from_str(&body).ok()?;
    let this_device = device_id().ok()?;
    (rec.device_id == this_device && (valid_shape(&rec.key) || is_legacy(&rec))).then_some(rec)
}

fn save_record(rec: &ActivationRecord) -> Result<()> {
    let path = activation_path()?;
    fs::write(&path, serde_json::to_string_pretty(rec)?).context("write activation record")
}

fn expired(rec: &ActivationRecord) -> bool {
    rec.expires_at
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .is_some_and(|t| t.timestamp() < now_secs() as i64)
}

/// Usable right now without asking the server.
fn locally_valid(rec: &ActivationRecord) -> bool {
    rec.error.is_none() && !expired(rec) && now_secs().saturating_sub(rec.last_ok) < OFFLINE_GRACE_SECS
}

/// Whether this machine may use the app. Re-checks the key with the server
/// when the last good check is older than `RECHECK_SECS`; a network failure
/// keeps the local verdict (offline grace), a refusal invalidates it.
pub async fn check() -> bool {
    let ok = check_uncached().await;
    remember(ok);
    ok
}

async fn check_uncached() -> bool {
    let Some(rec) = local_record() else { return false };
    if is_legacy(&rec) {
        return true;
    }
    if rec.error.is_some() {
        return false;
    }
    if now_secs().saturating_sub(rec.last_ok) < RECHECK_SECS && !expired(&rec) {
        return true;
    }
    match call_rpc(&rec.key, &rec.device_id, &machine_label(&rec)).await {
        Ok(r) => {
            let snapshot = rec.clone();
            let _ = apply_result(rec, &r);
            if r.ok {
                push_contact(&snapshot).await;
            }
            r.ok
        }
        Err(_) => locally_valid(&rec),
    }
}

/// How long the API gate trusts the last verdict before asking `check` again.
/// `check` itself only reaches the server every `RECHECK_SECS`; this keeps a
/// burst of API calls from re-reading the record for each one.
const GATE_CACHE_SECS: u64 = 300;

fn verdict_cell() -> &'static std::sync::Mutex<Option<(u64, bool)>> {
    static CELL: std::sync::OnceLock<std::sync::Mutex<Option<(u64, bool)>>> = std::sync::OnceLock::new();
    CELL.get_or_init(|| std::sync::Mutex::new(None))
}

fn remember(ok: bool) {
    if let Ok(mut g) = verdict_cell().lock() {
        *g = Some((now_secs(), ok));
    }
}

/// Whether the automation API may answer: the same verdict as the app's own
/// gate, at most `GATE_CACHE_SECS` old. A refusal is re-checked as soon as it
/// is stale, so a key activated in the UI opens the API within that time —
/// and at once, since `activate` records its success here.
pub async fn allowed() -> bool {
    let cached = verdict_cell().lock().ok().and_then(|g| *g);
    match cached {
        Some((at, ok)) if now_secs().saturating_sub(at) < GATE_CACHE_SECS => ok,
        _ => check().await,
    }
}

#[cfg(test)]
pub fn set_verdict_for_test(ok: bool) {
    remember(ok);
}

/// For the Settings page and the gate. None before activation, or if the
/// local record does not belong to this machine.
pub fn local_info() -> Option<LicenseInfo> {
    local_record().map(Into::into)
}

#[derive(Debug, Serialize)]
struct RpcBody<'a> {
    p_key: &'a str,
    p_may: &'a str,
    p_ten_may: &'a str,
}

#[derive(Debug, Deserialize)]
struct RpcResult {
    ok: bool,
    #[serde(default)]
    loi: Option<String>,
    #[serde(default)]
    het_han: Option<String>,
    #[serde(default)]
    ghi_chu: Option<String>,
}

fn friendly_error(code: &str) -> &'static str {
    match code {
        "KHONG_CO" => "Key không đúng — kiểm tra lại (dạng HLG-XXXX-XXXX-XXXX).",
        "BI_KHOA" => "Key đã bị khoá — liên hệ quản lý.",
        "HET_HAN" => "Key đã hết hạn — liên hệ quản lý để gia hạn.",
        "DU_MAY" => "Key đã dùng đủ số máy cho phép — nhờ quản lý gỡ máy cũ rồi kích hoạt lại.",
        "MA_MAY" => "Không đọc được mã máy.",
        _ => "Key không hợp lệ.",
    }
}

/// What the admin sees under "Máy đang dùng" in the bot: the customer's name
/// and phone once given, then this computer's name. The RPC keeps 80 chars.
fn machine_label(rec: &ActivationRecord) -> String {
    let host = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_default();
    let user = std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_default();
    let machine = if user.is_empty() { host } else { format!("{host} ({user})") };
    [rec.customer_name.as_deref(), rec.customer_phone.as_deref(), Some(machine.as_str())]
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
        .chars()
        .take(80)
        .collect()
}

async fn call_rpc(key: &str, device: &str, label: &str) -> Result<RpcResult> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let resp = client
        .post(format!("{SUPABASE_URL}/rest/v1/rpc/kich_hoat"))
        .header("apikey", SUPABASE_ANON_KEY)
        .header("Authorization", format!("Bearer {SUPABASE_ANON_KEY}"))
        .json(&RpcBody { p_key: key, p_may: device, p_ten_may: label })
        .send()
        .await
        .context("contact license server")?;

    if !resp.status().is_success() {
        anyhow::bail!("Máy chủ key trả lỗi {}", resp.status());
    }
    resp.json().await.context("parse license server response")
}

/// Best effort: copies the name / Zalo / Gmail the customer typed in this app onto the key itself (RPC `cap_nhat_lien_he`,
/// see supabase-lien-he.sql), so the Telegram bot lists the same fields for every tool. Silent when the info is incomplete,
/// when the RPC does not exist yet, or offline — the label sent with `kich_hoat` still carries name + phone.
async fn push_contact(rec: &ActivationRecord) {
    let name = rec.customer_name.as_deref().unwrap_or("").trim();
    let phone: String = rec.customer_phone.as_deref().unwrap_or("").chars().filter(|c| c.is_ascii_digit()).collect();
    let email = rec.customer_email.as_deref().unwrap_or("").trim().to_lowercase();
    if name.chars().count() < 2 || !(8..=15).contains(&phone.len()) || !email.contains('@') {
        return;
    }
    let Ok(client) = reqwest::Client::builder().timeout(Duration::from_secs(15)).build() else { return };
    let _ = client
        .post(format!("{SUPABASE_URL}/rest/v1/rpc/cap_nhat_lien_he"))
        .header("apikey", SUPABASE_ANON_KEY)
        .header("Authorization", format!("Bearer {SUPABASE_ANON_KEY}"))
        .json(&serde_json::json!({ "p_key": rec.key, "p_may": rec.device_id, "p_ho_ten": name, "p_gmail": email, "p_zalo": phone }))
        .send()
        .await;
}

/// Stores the server's answer: refreshes expiry / note / last_ok on success,
/// marks the record invalid (keeping the key) on refusal.
fn apply_result(mut rec: ActivationRecord, r: &RpcResult) -> Result<ActivationRecord> {
    if r.ok {
        rec.last_ok = now_secs();
        rec.expires_at = r.het_han.clone();
        rec.note = r.ghi_chu.clone().unwrap_or_default();
        rec.error = None;
    } else {
        rec.error = Some(friendly_error(r.loi.as_deref().unwrap_or("")).to_string());
    }
    save_record(&rec)?;
    Ok(rec)
}

/// Verifies `key` against the server and, on success, writes the local
/// activation record. Keeps customer info from an earlier activation of the
/// same machine.
pub async fn activate(key: &str) -> Result<()> {
    let key = normalize_key(key);
    if key.is_empty() {
        anyhow::bail!("Vui lòng nhập key.");
    }
    if !valid_shape(&key) {
        if key.starts_with(LEGACY_PREFIX) {
            anyhow::bail!("Key HIR- cũ chỉ dùng tiếp trên máy đã kích hoạt trước đây. Máy mới cần key dạng HLG-XXXX-XXXX-XXXX — liên hệ quản lý.");
        }
        if key.starts_with("HRS-") || key.starts_with("TPT-") {
            anyhow::bail!("Đây là key của tool khác — Hir-Login cần key dạng HLG-XXXX-XXXX-XXXX.");
        }
        anyhow::bail!("Key phải có dạng HLG-XXXX-XXXX-XXXX.");
    }
    let dev = device_id().context("Không đọc được mã máy")?;

    let previous = fs::read_to_string(activation_path()?)
        .ok()
        .and_then(|b| serde_json::from_str::<ActivationRecord>(&b).ok())
        .filter(|r| r.device_id == dev);
    let rec = ActivationRecord {
        key: key.clone(),
        device_id: dev,
        activated_at: chrono::Utc::now().to_rfc3339(),
        customer_name: previous.as_ref().and_then(|p| p.customer_name.clone()),
        customer_phone: previous.as_ref().and_then(|p| p.customer_phone.clone()),
        customer_email: previous.as_ref().and_then(|p| p.customer_email.clone()),
        ..Default::default()
    };

    let r = call_rpc(&key, &rec.device_id, &machine_label(&rec))
        .await
        .map_err(|e| anyhow::anyhow!("Không kết nối được máy chủ key ({e:#}) — kiểm tra mạng rồi thử lại."))?;
    if !r.ok {
        anyhow::bail!(friendly_error(r.loi.as_deref().unwrap_or("")));
    }
    let rec = apply_result(rec, &r)?;
    push_contact(&rec).await;
    remember(true);
    Ok(())
}

/// Saves the customer's name / phone-Zalo / email locally and re-sends the
/// activation so the bot shows name and phone next to this machine. Can be
/// called again later to correct a typo.
pub async fn submit_customer_info(name: &str, phone: &str, email: &str) -> Result<()> {
    let mut rec = local_record().context("Máy này chưa kích hoạt")?;
    rec.customer_name = (!name.trim().is_empty()).then(|| name.trim().to_string());
    rec.customer_phone = (!phone.trim().is_empty()).then(|| phone.trim().to_string());
    rec.customer_email = (!email.trim().is_empty()).then(|| email.trim().to_string());
    save_record(&rec)?;
    if is_legacy(&rec) {
        return Ok(());
    }

    // Best effort: the info is already saved; the label reaches the bot on
    // the next successful check if this one cannot get through.
    if let Ok(r) = call_rpc(&rec.key, &rec.device_id, &machine_label(&rec)).await {
        let rec = apply_result(rec, &r)?;
        push_contact(&rec).await;
    }
    Ok(())
}
