//! Built-in Chrome extension ("Hir-Login Proxy") that lets the operator switch a running
//! browser's proxy live from the toolbar without restarting the profile.
//!
//! Chromium's `chrome.proxy.settings` cannot authenticate SOCKS5 proxies with username/password,
//! so switching goes through `proxy_relay::switch` (an ephemeral loopback HTTP forwarder that
//! handles both HTTP Basic auth and SOCKS5 RFC 1929 auth, including special characters). A tiny
//! loopback-only control server (`127.0.0.1:0`) secured with a per-session random Bearer token
//! connects the extension to Hir-Login's proxy list and profile store.

use crate::{profile, proxy, proxy_relay};
use anyhow::Result;
use axum::{
    extract::{Query, Request},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tokio::sync::OnceCell;

struct ControlServer {
    port: u16,
    token: String,
}

static CONTROL: OnceCell<ControlServer> = OnceCell::const_new();

/// Tracks the live proxy currently active in the browser session (overrides what is on disk if the
/// operator switched without saving to the profile).
fn active_overrides() -> &'static Mutex<HashMap<String, Option<proxy::ProxyEntry>>> {
    static M: OnceLock<Mutex<HashMap<String, Option<proxy::ProxyEntry>>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Clears any session-only override when a profile launches fresh.
pub fn reset_session(profile_id: &str) {
    if let Ok(mut m) = active_overrides().lock() {
        m.remove(profile_id);
    }
}

async fn ensure_server() -> Result<&'static ControlServer> {
    CONTROL
        .get_or_try_init(|| async {
            let token = format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
            crate::api::no_inherit(&listener);
            let port = listener.local_addr()?.port();
            let expected = token.clone();
            let app = Router::new()
                .route("/state", get(get_state))
                .route("/switch", post(post_switch))
                .route("/check", post(post_check))
                .fallback(preflight_ok)
                .layer(middleware::from_fn(move |req: Request, next: Next| {
                    let expected = expected.clone();
                    async move { auth_and_cors(req, next, &expected).await }
                }));
            tokio::spawn(async move {
                let _ = axum::serve(listener, app).await;
            });
            Ok(ControlServer { port, token })
        })
        .await
}

async fn preflight_ok() -> Response {
    StatusCode::NO_CONTENT.into_response()
}

async fn auth_and_cors(req: Request, next: Next, expected: &str) -> Response {
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .filter(|o| o.starts_with("chrome-extension://"))
        .map(String::from);

    if req.method() == Method::OPTIONS {
        let mut resp = StatusCode::NO_CONTENT.into_response();
        apply_cors(resp.headers_mut(), origin.as_deref());
        return resp;
    }

    let ok = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer ").or_else(|| h.strip_prefix("bearer ")))
        .map(|t| t.trim() == expected)
        .unwrap_or(false);

    let mut resp = if ok {
        next.run(req).await
    } else {
        (StatusCode::UNAUTHORIZED, Json(json!({"ok": false, "error": "unauthorized"}))).into_response()
    };
    apply_cors(resp.headers_mut(), origin.as_deref());
    resp
}

fn apply_cors(headers: &mut HeaderMap, origin: Option<&str>) {
    if let Some(o) = origin {
        if let Ok(val) = HeaderValue::from_str(o) {
            headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, val);
        }
    }
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("Authorization, Content-Type"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, OPTIONS"),
    );
}

fn summarize_proxy(p: &proxy::ProxyEntry) -> Value {
    json!({
        "id": p.id,
        "name": p.name,
        "kind": p.kind.as_str(),
        "host": p.host,
        "port": p.port,
        "country": p.country,
        "hasAuth": !p.username.is_empty() || !p.password.is_empty(),
    })
}

fn current_proxy_for(profile_id: &str) -> (String, Option<proxy::ProxyEntry>) {
    let stored = profile::load_raw(profile_id).ok();
    let name = stored
        .as_ref()
        .and_then(|s| s.config.get("name"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(profile_id)
        .to_string();
    if let Ok(m) = active_overrides().lock() {
        if let Some(ov) = m.get(profile_id) {
            return (name, ov.clone());
        }
    }
    let bound = stored.as_ref().and_then(|s| profile::launch_proxy(&s.meta));
    (name, bound)
}

#[derive(Deserialize)]
struct StateQuery {
    profile: String,
}

async fn get_state(Query(q): Query<StateQuery>) -> Response {
    let (profile_name, current) = current_proxy_for(&q.profile);
    let proxies: Vec<Value> = proxy::load()
        .map(|s| s.proxies.iter().map(summarize_proxy).collect())
        .unwrap_or_default();
    Json(json!({
        "ok": true,
        "profileName": profile_name,
        "current": current.as_ref().map(summarize_proxy),
        "relayPort": proxy_relay::port_of(&q.profile),
        "proxies": proxies,
    }))
    .into_response()
}

#[derive(Deserialize)]
struct SwitchReq {
    profile_id: String,
    #[serde(default)]
    proxy_id: Option<String>,
    #[serde(default)]
    raw: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    direct: bool,
    #[serde(default = "default_true")]
    save_to_profile: bool,
}

fn default_true() -> bool {
    true
}

async fn post_switch(Json(req): Json<SwitchReq>) -> Response {
    let target: Option<proxy::ProxyEntry> = if req.direct {
        None
    } else if let Some(pid) = req.proxy_id.as_deref().filter(|s| !s.trim().is_empty()) {
        match proxy::get(pid).ok().flatten() {
            Some(p) => Some(proxy::effective(&p).await),
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(json!({"ok": false, "error": "Không tìm thấy proxy đã chọn"})),
                )
                    .into_response();
            }
        }
    } else if let Some(raw) = req.raw.as_deref().filter(|s| !s.trim().is_empty()) {
        let default_kind = match req.kind.as_deref().unwrap_or("") {
            "http" => proxy::ProxyKind::Http,
            "https" => proxy::ProxyKind::Https,
            _ => proxy::ProxyKind::Socks5,
        };
        let Some(parsed) = proxy::parse_single_with_kind(raw, default_kind) else {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "error": "Định dạng proxy không hợp lệ (vd: host:port:user:pass hoặc socks5://user:pass@host:port)"})),
            )
                .into_response();
        };
        let detected = proxy::effective(&parsed).await;
        if req.save_to_profile {
            match proxy::upsert_dedup(detected.clone()) {
                Ok(saved) => {
                    crate::notify_store_changed("proxies");
                    Some(saved)
                }
                Err(_) => Some(detected),
            }
        } else {
            Some(detected)
        }
    } else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok": false, "error": "Thiếu thông tin proxy"})),
        )
            .into_response();
    };

    let relay_port = match proxy_relay::switch(&req.profile_id, target.as_ref()).await {
        Ok(port) => port,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"ok": false, "error": format!("Không bật được bộ chuyển tiếp proxy: {e}")})),
            )
                .into_response();
        }
    };

    if let Ok(mut m) = active_overrides().lock() {
        m.insert(req.profile_id.clone(), target.clone());
    }

    if req.save_to_profile {
        if let Ok(mut stored) = profile::load_raw(&req.profile_id) {
            let new_pid = target.as_ref().map(|p| p.id.clone()).filter(|id| !id.is_empty());
            profile::bind_proxy_stamped(&mut stored.meta, new_pid);
            stored.meta.temp_proxy = None;
            stored.meta.temp_proxy_at = 0;
            if profile::save_raw(&mut stored).is_ok() {
                crate::notify_store_changed("profiles");
                crate::cloud_sync::kick();
            }
        }
    }

    let proxies: Vec<Value> = proxy::load()
        .map(|s| s.proxies.iter().map(summarize_proxy).collect())
        .unwrap_or_default();

    Json(json!({
        "ok": true,
        "relayPort": relay_port,
        "current": target.as_ref().map(summarize_proxy),
        "proxies": proxies,
    }))
    .into_response()
}

#[derive(Deserialize)]
struct CheckReq {
    profile_id: String,
}

async fn post_check(Json(req): Json<CheckReq>) -> Response {
    let (_, current) = current_proxy_for(&req.profile_id);
    let started = std::time::Instant::now();
    match proxy::geo_check_via(current.as_ref(), None).await {
        Ok(g) => Json(json!({
            "ok": true,
            "ip": g.ip,
            "country": g.country,
            "countryCode": g.country_code,
            "city": g.city,
            "ms": started.elapsed().as_millis() as u64,
        }))
        .into_response(),
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"ok": false, "error": format!("{e:#}")})),
        )
            .into_response(),
    }
}

/// Writes the unpacked MV3 proxy-switcher extension into `<udd>/hir-proxy-ext/` and returns its
/// directory for `--load-extension`.
pub async fn prepare_extension(udd: &Path, profile_id: &str) -> Result<PathBuf> {
    reset_session(profile_id);
    let ctrl = ensure_server().await?;
    let dir = udd.join("hir-proxy-ext");
    std::fs::create_dir_all(&dir)?;

    let config = json!({
        "port": ctrl.port,
        "token": ctrl.token,
        "profileId": profile_id,
    });
    std::fs::write(dir.join("config.json"), serde_json::to_vec(&config)?)?;
    std::fs::write(dir.join("manifest.json"), MANIFEST_JSON)?;
    std::fs::write(dir.join("bg.js"), BG_JS)?;
    std::fs::write(dir.join("popup.html"), POPUP_HTML)?;
    std::fs::write(dir.join("popup.js"), POPUP_JS)?;
    write_icon_png(&dir.join("icon.png"))?;
    Ok(dir)
}

/// Generates a clean 32x32 RGBA PNG icon (indigo rounded square with a crisp white ring) using
/// the `png` crate already in our dependencies — no external binary asset needed.
fn write_icon_png(path: &Path) -> Result<()> {
    const N: u32 = 32;
    let mut rgba = vec![0u8; (N * N * 4) as usize];
    for y in 0..N {
        for x in 0..N {
            let i = ((y * N + x) * 4) as usize;
            let dx = x as f32 - 15.5;
            let dy = y as f32 - 15.5;
            let ax = dx.abs();
            let ay = dy.abs();
            // Rounded square background (#335cff)
            let corner_x = (ax - 9.5).max(0.0);
            let corner_y = (ay - 9.5).max(0.0);
            let inside = corner_x * corner_x + corner_y * corner_y <= 5.5 * 5.5;
            if !inside {
                continue;
            }
            let r = (dx * dx + dy * dy).sqrt();
            let ring = (r >= 6.8 && r <= 9.2) || (r <= 2.6) || (ay <= 1.1 && ax <= 8.5) || (ax <= 1.1 && ay <= 8.5);
            if ring {
                rgba[i] = 255;
                rgba[i + 1] = 255;
                rgba[i + 2] = 255;
                rgba[i + 3] = 255;
            } else {
                rgba[i] = 51;
                rgba[i + 1] = 92;
                rgba[i + 2] = 255;
                rgba[i + 3] = 255;
            }
        }
    }
    let file = std::fs::File::create(path)?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), N, N);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header()?;
    writer.write_image_data(&rgba)?;
    Ok(())
}

const MANIFEST_JSON: &str = r#"{
  "manifest_version": 3,
  "name": "Hir-Login Proxy",
  "version": "1.0.0",
  "description": "Đổi proxy trực tiếp trên trình duyệt Hir-Login không cần khởi động lại.",
  "permissions": ["proxy", "storage"],
  "host_permissions": ["http://127.0.0.1/*"],
  "icons": { "32": "icon.png" },
  "action": {
    "default_title": "Hir-Login Proxy",
    "default_icon": { "32": "icon.png" },
    "default_popup": "popup.html"
  },
  "background": {
    "service_worker": "bg.js"
  }
}"#;

const BG_JS: &str = r#"// On a fresh browser launch, clear any leftover proxy override from a previous session so the
// launcher's `--proxy-server` flag takes effect until the user switches live from the popup.
function clearOncePerSession() {
  if (!chrome.storage || !chrome.storage.session) return;
  chrome.storage.session.get("hirProxyBooted", (res) => {
    if (!res || !res.hirProxyBooted) {
      chrome.storage.session.set({ hirProxyBooted: true });
      chrome.proxy.settings.clear({ scope: "regular" });
    }
  });
}

chrome.runtime.onStartup.addListener(clearOncePerSession);
chrome.runtime.onInstalled.addListener(clearOncePerSession);
clearOncePerSession();

chrome.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
  if (!msg || msg.type !== "hir-apply-proxy") return;
  if (msg.relayPort) {
    chrome.proxy.settings.set(
      {
        value: {
          mode: "fixed_servers",
          rules: {
            singleProxy: { scheme: "http", host: "127.0.0.1", port: Number(msg.relayPort) },
            bypassList: ["127.0.0.1", "localhost", "<local>"],
          },
        },
        scope: "regular",
      },
      () => sendResponse({ ok: !chrome.runtime.lastError, error: chrome.runtime.lastError?.message })
    );
  } else {
    chrome.proxy.settings.set(
      { value: { mode: "direct" }, scope: "regular" },
      () => sendResponse({ ok: !chrome.runtime.lastError, error: chrome.runtime.lastError?.message })
    );
  }
  return true;
});
"#;

const POPUP_HTML: &str = r#"<!doctype html>
<html lang="vi">
<head>
<meta charset="utf-8" />
<title>Hir-Login Proxy</title>
<style>
  * { box-sizing: border-box; }
  body {
    margin: 0; width: 350px; font-family: system-ui, -apple-system, Segoe UI, sans-serif;
    background: #0f1117; color: #eceff4; font-size: 12.5px;
  }
  .head {
    display: flex; align-items: center; justify-content: space-between; gap: 8px;
    padding: 10px 12px; border-bottom: 1px solid #222634; background: #151822;
  }
  .title { font-weight: 600; font-size: 13px; }
  .prof { color: #8c95aa; font-size: 11px; max-width: 170px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .box { padding: 10px 12px; border-bottom: 1px solid #222634; }
  .cur { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  .cur-val { font-family: ui-monospace, Consolas, monospace; font-size: 12px; color: #7dd3fc; word-break: break-all; }
  .cur-direct { color: #fbbf24; }
  .row { display: flex; gap: 6px; margin-top: 8px; }
  input, select {
    width: 100%; background: #181c28; color: #eceff4; border: 1px solid #2a3042;
    border-radius: 6px; padding: 6px 8px; font-size: 12px; outline: none;
  }
  input:focus, select:focus { border-color: #3b82f6; }
  button {
    cursor: pointer; border: 1px solid #2a3042; background: #1d2230; color: #eceff4;
    border-radius: 6px; padding: 6px 10px; font-size: 12px; font-weight: 500; white-space: nowrap;
  }
  button:hover { background: #262d40; }
  button.primary { background: #335cff; border-color: #335cff; color: #fff; }
  button.primary:hover { background: #264ce0; }
  button.warn { color: #fbbf24; border-color: #4d3b15; background: #241d0f; }
  button:disabled { opacity: 0.5; cursor: default; }
  .check { display: flex; align-items: center; gap: 6px; margin-top: 8px; color: #aeb6c8; font-size: 11.5px; cursor: pointer; }
  .check input { width: auto; margin: 0; }
  .status { margin-top: 6px; font-size: 11.5px; min-height: 16px; color: #94a3b8; }
  .status.ok { color: #34d399; }
  .status.err { color: #f87171; }
  .list { max-height: 220px; overflow-y: auto; display: flex; flex-direction: column; gap: 4px; margin-top: 8px; }
  .item {
    display: flex; align-items: center; justify-content: space-between; gap: 8px;
    padding: 6px 8px; border-radius: 6px; background: #151822; border: 1px solid #222634;
    cursor: pointer; text-align: left; width: 100%;
  }
  .item:hover { border-color: #335cff; background: #1a2033; }
  .item.active { border-color: #34d399; background: #11241f; }
  .item-main { min-width: 0; flex: 1; }
  .item-host { font-family: ui-monospace, Consolas, monospace; font-size: 11.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .item-sub { font-size: 10.5px; color: #8c95aa; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .badge { font-size: 10px; padding: 1px 6px; border-radius: 99px; background: #22283a; color: #cbd5e1; text-transform: uppercase; flex: none; }
</style>
</head>
<body>
  <div class="head">
    <span class="title">Hir-Login Proxy</span>
    <span class="prof" id="profName">…</span>
  </div>

  <div class="box">
    <div class="cur">
      <span id="curProxy" class="cur-val">Đang tải…</span>
      <button id="btnCheck" type="button">Check IP</button>
    </div>
    <div class="row">
      <button id="btnDirect" class="warn" type="button" style="flex:1">Tắt proxy (Trực tiếp)</button>
    </div>
    <label class="check">
      <input id="saveProfile" type="checkbox" checked />
      <span>Lưu proxy mới vào profile</span>
    </label>
    <div id="status" class="status"></div>
  </div>

  <div class="box">
    <div style="font-weight:600;margin-bottom:6px;font-size:11.5px;color:#aeb6c8">Dán proxy nhanh</div>
    <input id="rawInput" placeholder="host:port:user:pass hoặc socks5://..." />
    <div class="row">
      <select id="kindSel" style="width:110px;flex:none">
        <option value="socks5">SOCKS5</option>
        <option value="http">HTTP</option>
        <option value="https">HTTPS</option>
      </select>
      <button id="btnApplyRaw" class="primary" type="button" style="flex:1">Đổi proxy ngay</button>
    </div>
  </div>

  <div class="box" style="border-bottom:none">
    <div style="font-weight:600;margin-bottom:6px;font-size:11.5px;color:#aeb6c8">Chọn từ danh sách Proxy</div>
    <input id="search" placeholder="Tìm theo tên, IP, quốc gia…" />
    <div id="list" class="list"></div>
  </div>

  <script src="popup.js"></script>
</body>
</html>
"#;

const POPUP_JS: &str = r#"let cfg = null;
let state = { current: null, proxies: [] };

const $ = (id) => document.getElementById(id);

function setStatus(msg, cls = "") {
  const el = $("status");
  el.textContent = msg || "";
  el.className = "status" + (cls ? " " + cls : "");
}

async function api(path, opts = {}) {
  const res = await fetch(`http://127.0.0.1:${cfg.port}${path}`, {
    ...opts,
    headers: {
      Authorization: `Bearer ${cfg.token}`,
      "Content-Type": "application/json",
      ...(opts.headers || {}),
    },
  });
  const data = await res.json().catch(() => ({ ok: false, error: `HTTP ${res.status}` }));
  if (!res.ok || data.ok === false) throw new Error(data.error || `HTTP ${res.status}`);
  return data;
}

function applyBrowserProxy(relayPort) {
  return new Promise((resolve, reject) => {
    chrome.runtime.sendMessage({ type: "hir-apply-proxy", relayPort }, (resp) => {
      if (chrome.runtime.lastError) return reject(new Error(chrome.runtime.lastError.message));
      if (!resp || !resp.ok) return reject(new Error(resp?.error || "Không áp dụng được proxy vào Chrome"));
      resolve();
    });
  });
}

function renderCurrent() {
  const el = $("curProxy");
  if (!state.current) {
    el.textContent = "Trực tiếp (Không proxy)";
    el.className = "cur-val cur-direct";
  } else {
    const c = state.current;
    const cc = c.country ? ` (${c.country})` : "";
    el.textContent = `${String(c.kind || "").toUpperCase()} ${c.host}:${c.port}${cc}`;
    el.className = "cur-val";
  }
}

function renderList() {
  const q = ($("search").value || "").trim().toLowerCase();
  const listEl = $("list");
  listEl.innerHTML = "";
  const items = (state.proxies || []).filter((p) => {
    if (!q) return true;
    return `${p.name || ""} ${p.host}:${p.port} ${p.country || ""} ${p.kind || ""}`.toLowerCase().includes(q);
  });
  if (items.length === 0) {
    const empty = document.createElement("div");
    empty.style.cssText = "color:#64748b;text-align:center;padding:12px 0;font-size:11.5px";
    empty.textContent = "Không có proxy nào.";
    listEl.appendChild(empty);
    return;
  }
  for (const p of items.slice(0, 100)) {
    const btn = document.createElement("button");
    btn.type = "button";
    const isActive = state.current && state.current.host === p.host && state.current.port === p.port;
    btn.className = "item" + (isActive ? " active" : "");
    btn.innerHTML = `<div class="item-main"><div class="item-host"></div><div class="item-sub"></div></div><span class="badge"></span>`;
    btn.querySelector(".item-host").textContent = `${p.host}:${p.port}`;
    btn.querySelector(".item-sub").textContent = [p.name, p.country].filter(Boolean).join(" · ") || "Proxy đã lưu";
    btn.querySelector(".badge").textContent = p.kind || "socks5";
    btn.addEventListener("click", () => switchProxy({ proxy_id: p.id }));
    listEl.appendChild(btn);
  }
}

async function switchProxy(payload) {
  setStatus("Đang đổi proxy…");
  try {
    const data = await api("/switch", {
      method: "POST",
      body: JSON.stringify({
        profile_id: cfg.profileId,
        save_to_profile: $("saveProfile").checked,
        ...payload,
      }),
    });
    await applyBrowserProxy(data.relayPort || null);
    state.current = data.current || null;
    if (Array.isArray(data.proxies)) state.proxies = data.proxies;
    renderCurrent();
    renderList();
    setStatus(state.current ? `Đã đổi sang ${state.current.host}:${state.current.port}` : "Đã tắt proxy (kết nối trực tiếp)", "ok");
  } catch (e) {
    setStatus(String(e.message || e), "err");
  }
}

async function checkIp() {
  const btn = $("btnCheck");
  btn.disabled = true;
  setStatus("Đang kiểm tra IP…");
  try {
    const r = await api("/check", {
      method: "POST",
      body: JSON.stringify({ profile_id: cfg.profileId }),
    });
    const place = [r.countryCode, r.city].filter(Boolean).join(" · ");
    setStatus(`IP: ${r.ip}${place ? " (" + place + ")" : ""} · ${r.ms}ms`, "ok");
  } catch (e) {
    setStatus(`Kiểm tra IP lỗi: ${e.message || e}`, "err");
  } finally {
    btn.disabled = false;
  }
}

async function boot() {
  try {
    const res = await fetch(chrome.runtime.getURL("config.json"));
    cfg = await res.json();
    const data = await api(`/state?profile=${encodeURIComponent(cfg.profileId)}`);
    $("profName").textContent = data.profileName || cfg.profileId;
    state.current = data.current || null;
    state.proxies = data.proxies || [];
    renderCurrent();
    renderList();
  } catch (e) {
    setStatus("Không kết nối được Hir-Login — hãy mở lại profile.", "err");
  }
}

$("btnDirect").addEventListener("click", () => switchProxy({ direct: true }));
$("btnCheck").addEventListener("click", checkIp);
$("search").addEventListener("input", renderList);
$("btnApplyRaw").addEventListener("click", () => {
  const raw = ($("rawInput").value || "").trim();
  if (!raw) return;
  switchProxy({ raw, kind: $("kindSel").value }).then(() => { $("rawInput").value = ""; });
});
$("rawInput").addEventListener("keydown", (e) => {
  if (e.key === "Enter") $("btnApplyRaw").click();
});

boot();
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn prepares_unpacked_extension_with_valid_manifest_config_and_png() {
        let dir = std::env::temp_dir().join(format!("hir-pswitch-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let ext = prepare_extension(&dir, "prof-1").await.unwrap();
        assert!(ext.join("manifest.json").is_file());
        assert!(ext.join("bg.js").is_file());
        assert!(ext.join("popup.html").is_file());
        assert!(ext.join("popup.js").is_file());
        let png_bytes = std::fs::read(ext.join("icon.png")).unwrap();
        assert_eq!(&png_bytes[0..8], &[137, 80, 78, 71, 13, 10, 26, 10], "valid PNG header");
        let cfg: Value = serde_json::from_str(&std::fs::read_to_string(ext.join("config.json")).unwrap()).unwrap();
        assert_eq!(cfg["profileId"], "prof-1");
        assert!(cfg["port"].as_u64().unwrap_or(0) > 0);
        assert!(!cfg["token"].as_str().unwrap_or("").is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
