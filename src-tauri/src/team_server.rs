use anyhow::{Context, Result};
use axum::{
    body::Body,
    extract::Path as AxumPath,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::oneshot;

const MAX_BUNDLE_BYTES: usize = 2 * 1024 * 1024 * 1024;

fn server_data_dir() -> Result<PathBuf> {
    let dir = crate::store::data_root()?.join("team-sync");
    std::fs::create_dir_all(dir.join("bundles"))?;
    Ok(dir)
}

fn bundles_dir() -> Result<PathBuf> {
    Ok(server_data_dir()?.join("bundles"))
}
fn locks_path() -> Result<PathBuf> {
    Ok(server_data_dir()?.join("locks.json"))
}
fn meta_path() -> Result<PathBuf> {
    Ok(server_data_dir()?.join("meta.json"))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn lock_ttl_ms() -> u64 {
    // 6h default, same as sync-server/server.js
    6 * 3600 * 1000
}

fn load_json(path: &PathBuf, fallback: Value) -> Value {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(fallback)
}

fn save_json_atomic(path: &PathBuf, val: &Value) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(val)?)?;
    // best-effort fsync
    if let Ok(f) = std::fs::File::open(&tmp) {
        let _ = f.sync_all();
    }
    // retry rename like profile.rs does
    for _ in 0..4 {
        if std::fs::rename(&tmp, path).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    std::fs::rename(&tmp, path).context("atomic write failed")?;
    Ok(())
}

fn is_expired(lock: &Value) -> bool {
    lock.get("expiresAt")
        .and_then(|v| v.as_u64())
        .map(|exp| now_ms() > exp)
        .unwrap_or(true)
}

fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

// ---- shared state ----

struct ServerState {
    port: u16,
    token: String,
}

static SERVER_STATE: OnceLock<Mutex<Option<ServerState>>> = OnceLock::new();
fn server_state() -> &'static Mutex<Option<ServerState>> {
    SERVER_STATE.get_or_init(|| Mutex::new(None))
}

static SHUTDOWN_TX: OnceLock<Mutex<Option<oneshot::Sender<()>>>> = OnceLock::new();
fn shutdown_cell() -> &'static Mutex<Option<oneshot::Sender<()>>> {
    SHUTDOWN_TX.get_or_init(|| Mutex::new(None))
}

// ---- auth helper ----

fn check_auth(headers: &HeaderMap, token: &str) -> bool {
    let Some(h) = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
    else {
        return false;
    };
    let Some(t) = h
        .strip_prefix("Bearer ")
        .or_else(|| h.strip_prefix("bearer "))
    else {
        return false;
    };
    // timing-safe compare
    let a = t.trim().as_bytes();
    let b = token.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

// ---- handlers ----

async fn health() -> impl IntoResponse {
    Json(json!({"ok": true}))
}

async fn list_profiles(
    headers: HeaderMap,
    axum::extract::State(state): axum::extract::State<Arc<ServerState>>,
) -> Response {
    if !check_auth(&headers, &state.token) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"ok":false,"error":"unauthorized"}))).into_response();
    }
    let locks: Value = load_json(&locks_path().unwrap_or_default(), json!({}));
    let meta: Value = load_json(&meta_path().unwrap_or_default(), json!({}));
    let locks_map = locks.as_object().cloned().unwrap_or_default();
    let meta_map = meta.as_object().cloned().unwrap_or_default();
    let mut ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for k in locks_map.keys() { ids.insert(k.clone()); }
    for k in meta_map.keys() { ids.insert(k.clone()); }
    let profiles: Vec<Value> = ids.into_iter().map(|id| {
        let lock = locks_map.get(&id).cloned().unwrap_or(Value::Null);
        let held = !lock.is_null() && !is_expired(&lock);
        let m = meta_map.get(&id);
        json!({
            "id": id,
            "updatedAt": m.and_then(|v| v.get("updatedAt")).cloned().unwrap_or(Value::Null),
            "updatedBy": m.and_then(|v| v.get("updatedBy")).cloned().unwrap_or(Value::Null),
            "sizeBytes": m.and_then(|v| v.get("sizeBytes")).cloned().unwrap_or(Value::Null),
            "locked": held,
            "holder": if held { lock.get("holder").cloned().unwrap_or(Value::Null) } else { Value::Null },
            "lockExpiresAt": if held { lock.get("expiresAt").cloned().unwrap_or(Value::Null) } else { Value::Null },
        })
    }).collect();
    Json(json!({"ok": true, "profiles": profiles})).into_response()
}

async fn lock_profile(
    headers: HeaderMap,
    axum::extract::State(state): axum::extract::State<Arc<ServerState>>,
    AxumPath(id): AxumPath<String>,
    body: axum::body::Bytes,
) -> Response {
    if !check_auth(&headers, &state.token) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"ok":false,"error":"unauthorized"}))).into_response();
    }
    if !safe_id(&id) {
        return (StatusCode::BAD_REQUEST, Json(json!({"ok":false,"error":"bad id"}))).into_response();
    }
    let v: Value = serde_json::from_slice(&body).unwrap_or(json!({}));
    let holder = v.get("holder").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
    if holder.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({"ok":false,"error":"holder required"}))).into_response();
    }
    let holder = holder.chars().take(200).collect::<String>();
    let locks_path = match locks_path() { Ok(p) => p, Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok":false,"error":e.to_string()}))).into_response() };
    let mut locks = load_json(&locks_path, json!({}));
    let map = locks.as_object_mut().unwrap();
    if let Some(existing) = map.get(&id).cloned() {
        if !is_expired(&existing) && existing.get("holder").and_then(|h| h.as_str()) != Some(&holder) {
            let holder_other = existing.get("holder").cloned().unwrap_or(Value::Null);
            let exp = existing.get("expiresAt").cloned().unwrap_or(Value::Null);
            return (StatusCode::CONFLICT, Json(json!({"ok":false,"holder":holder_other,"expiresAt":exp}))).into_response();
        }
    }
    let expires_at = now_ms() + lock_ttl_ms();
    map.insert(id.clone(), json!({"holder": holder, "acquiredAt": now_ms(), "expiresAt": expires_at}));
    let _ = save_json_atomic(&locks_path, &locks);
    Json(json!({"ok": true, "expiresAt": expires_at})).into_response()
}

async fn unlock_profile(
    headers: HeaderMap,
    axum::extract::State(state): axum::extract::State<Arc<ServerState>>,
    AxumPath(id): AxumPath<String>,
    body: axum::body::Bytes,
) -> Response {
    if !check_auth(&headers, &state.token) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"ok":false,"error":"unauthorized"}))).into_response();
    }
    if !safe_id(&id) {
        return (StatusCode::BAD_REQUEST, Json(json!({"ok":false,"error":"bad id"}))).into_response();
    }
    let v: Value = serde_json::from_slice(&body).unwrap_or(json!({}));
    let holder = v.get("holder").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let force = v.get("force").and_then(|x| x.as_bool()).unwrap_or(false);
    let locks_path = match locks_path() { Ok(p) => p, Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok":false,"error":e.to_string()}))).into_response() };
    let mut locks = load_json(&locks_path, json!({}));
    if let Some(map) = locks.as_object_mut() {
        if let Some(existing) = map.get(&id).cloned() {
            if !is_expired(&existing) && existing.get("holder").and_then(|h| h.as_str()) != Some(holder.as_str()) && !force {
                let h = existing.get("holder").cloned().unwrap_or(Value::Null);
                return (StatusCode::FORBIDDEN, Json(json!({"ok":false,"error":"held by another holder","holder":h}))).into_response();
            }
        }
        map.remove(&id);
    }
    let _ = save_json_atomic(&locks_path, &locks);
    Json(json!({"ok": true})).into_response()
}

async fn get_bundle(
    headers: HeaderMap,
    axum::extract::State(state): axum::extract::State<Arc<ServerState>>,
    AxumPath(id): AxumPath<String>,
) -> Response {
    if !check_auth(&headers, &state.token) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"ok":false,"error":"unauthorized"}))).into_response();
    }
    if !safe_id(&id) {
        return (StatusCode::BAD_REQUEST, Json(json!({"ok":false,"error":"bad id"}))).into_response();
    }
    let dir = match bundles_dir() { Ok(d) => d, Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok":false,"error":e.to_string()}))).into_response() };
    let path = dir.join(format!("{id}.zip"));
    match tokio::fs::read(&path).await {
        Ok(bytes) => {
            let mut headers_map = HeaderMap::new();
            headers_map.insert("Content-Type", "application/zip".parse().unwrap());
            (StatusCode::OK, headers_map, Body::from(bytes)).into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, Json(json!({"ok":false,"error":"no bundle yet"}))).into_response(),
    }
}

async fn put_bundle(
    headers: HeaderMap,
    axum::extract::State(state): axum::extract::State<Arc<ServerState>>,
    AxumPath(id): AxumPath<String>,
    body: axum::body::Bytes,
) -> Response {
    if !check_auth(&headers, &state.token) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"ok":false,"error":"unauthorized"}))).into_response();
    }
    if !safe_id(&id) {
        return (StatusCode::BAD_REQUEST, Json(json!({"ok":false,"error":"bad id"}))).into_response();
    }
    if body.len() > MAX_BUNDLE_BYTES {
        return (StatusCode::PAYLOAD_TOO_LARGE, Json(json!({"ok":false,"error":"bundle too large"}))).into_response();
    }
    let holder = headers.get("x-sync-holder").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let locks_path = match locks_path() { Ok(p) => p, Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok":false,"error":e.to_string()}))).into_response() };
    let mut locks = load_json(&locks_path, json!({}));
    let existing = locks.get(&id).cloned().unwrap_or(Value::Null);
    if existing.is_null() || is_expired(&existing) || existing.get("holder").and_then(|h| h.as_str()) != Some(holder.as_str()) {
        return (StatusCode::FORBIDDEN, Json(json!({"ok":false,"error":"you do not hold the lock for this profile"}))).into_response();
    }
    let dir = match bundles_dir() { Ok(d) => d, Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok":false,"error":e.to_string()}))).into_response() };
    let file_path = dir.join(format!("{id}.zip"));
    if let Err(e) = tokio::fs::write(&file_path, &body).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok":false,"error":e.to_string()}))).into_response();
    }
    // update meta
    let meta_path = match meta_path() { Ok(p) => p, Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok":false,"error":e.to_string()}))).into_response() };
    let mut meta = load_json(&meta_path, json!({}));
    let meta_map = meta.as_object_mut().unwrap();
    // ISO 8601 UTC
    let now_iso = {
        let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        // simple ISO without chrono dep
        let dt = time_format(secs);
        dt
    };
    meta_map.insert(id.clone(), json!({"updatedAt": now_iso, "updatedBy": holder, "sizeBytes": body.len()}));
    let _ = save_json_atomic(&meta_path, &meta);
    // refresh lock TTL
    if let Some(map) = locks.as_object_mut() {
        map.insert(id.clone(), json!({"holder": holder, "acquiredAt": existing.get("acquiredAt").cloned().unwrap_or(json!(now_ms())), "expiresAt": now_ms() + lock_ttl_ms()}));
        let _ = save_json_atomic(&locks_path, &locks);
    }
    Json(json!({"ok": true, "sizeBytes": body.len()})).into_response()
}

fn time_format(secs: u64) -> String {
    // Format as ISO 8601 using simple calculation — avoid extra dep
    // Use chrono-like output: 2026-09-26T12:34:56Z
    // We use a minimal approach: format via SystemTime debug is not ISO, so do manual
    let days = secs / 86400;
    let rem = secs % 86400;
    let h = rem / 3600;
    let m = (rem % 3600) / 60;
    let s = rem % 60;
    // days since 1970-01-01 → date (proleptic, good enough for display)
    let (y, mo, d) = days_to_ymd(days);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Howard Hinnant's civil_from_days
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as u64, m as u64, d as u64)
}

// ---- public API ----

pub fn is_running() -> bool {
    server_state().lock().map(|g| g.is_some()).unwrap_or(false)
}

pub fn running_info() -> Option<(u16, String)> {
    server_state()
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|s| (s.port, s.token.clone())))
}

pub async fn start(port: u16, token: String) -> Result<u16> {
    if token.len() < 8 {
        anyhow::bail!("token phải dài ít nhất 8 ký tự");
    }
    // Already running
    if is_running() {
        anyhow::bail!("team server đang chạy rồi");
    }

    let state = Arc::new(ServerState { port, token: token.clone() });

    let app = Router::new()
        .route("/health", get(health))
        .route("/profiles", get(list_profiles))
        .route("/profiles/:id/lock", post(lock_profile))
        .route("/profiles/:id/unlock", post(unlock_profile))
        .route("/profiles/:id/bundle", get(get_bundle).put(put_bundle))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await
        .with_context(|| format!("không bind được port {port}"))?;
    let actual_port = listener.local_addr().map(|a| a.port()).unwrap_or(port);

    // Update state with actual port
    {
        let mut g = server_state().lock().unwrap();
        *g = Some(ServerState { port: actual_port, token });
    }

    let (tx, rx) = oneshot::channel::<()>();
    *shutdown_cell().lock().unwrap() = Some(tx);

    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = rx.await;
            })
            .await
            .ok();
        // Clear state on exit
        if let Ok(mut g) = server_state().lock() { *g = None; }
    });

    // Small delay to let bind succeed
    tokio::time::sleep(Duration::from_millis(100)).await;

    let _ = tokio::task::spawn_blocking(move || ensure_firewall_rule(actual_port));

    Ok(actual_port)
}

/// Windows Firewall drops inbound connections on the Tailscale interface (it is
/// classed as a Public network) unless a rule allows the port, so other machines
/// time out even though Tailscale itself is fine. Add one rule per port, once;
/// creating it needs administrator rights, so it goes through a UAC prompt.
/// Best effort: a refused prompt leaves the server running, just unreachable.
#[cfg(target_os = "windows")]
fn ensure_firewall_rule(port: u16) {
    use std::os::windows::process::CommandExt;
    const NO_WINDOW: u32 = 0x08000000;
    let name = format!("Hir-Login Team Server {port}");
    let exists = std::process::Command::new("netsh")
        .args(["advfirewall", "firewall", "show", "rule", &format!("name={name}")])
        .creation_flags(NO_WINDOW)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if exists {
        return;
    }
    let add = format!(
        "advfirewall firewall add rule name=\"{name}\" dir=in action=allow protocol=TCP localport={port} profile=any"
    );
    let ps = format!(
        "Start-Process netsh -ArgumentList '{}' -Verb RunAs -WindowStyle Hidden -Wait",
        add.replace('\'', "''")
    );
    let _ = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &ps])
        .creation_flags(NO_WINDOW)
        .status();
}

#[cfg(not(target_os = "windows"))]
fn ensure_firewall_rule(_port: u16) {}

pub fn stop() -> Result<()> {
    let tx = shutdown_cell().lock().unwrap().take();
    if let Some(tx) = tx {
        let _ = tx.send(());
    }
    if let Ok(mut g) = server_state().lock() { *g = None; }
    Ok(())
}

pub fn tailscale_ip() -> Option<String> {
    // Try to find a 100.x.x.x address (CGNAT range used by Tailscale)
    #[cfg(unix)]
    {
        if let Ok(out) = std::process::Command::new("sh")
            .arg("-c")
            .arg("ifconfig 2>/dev/null | grep -o '100\\.[0-9]*\\.[0-9]*\\.[0-9]*' | head -1")
            .output()
        {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() { return Some(s); }
        }
        // fallback: tailscale ip command
        if let Ok(out) = std::process::Command::new("tailscale")
            .args(["ip", "-4"])
            .output()
        {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if s.starts_with("100.") { return Some(s); }
        }
    }
    #[cfg(windows)]
    {
        if let Ok(out) = std::process::Command::new("tailscale")
            .args(["ip", "-4"])
            .output()
        {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if s.starts_with("100.") { return Some(s); }
        }
    }
    None
}
