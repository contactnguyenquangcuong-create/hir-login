// Local automation HTTP API (axum) for ShardX Launcher.
// 127.0.0.1:<api_port>; every endpoint except /health requires Bearer JWT (HS256).

use std::sync::{OnceLock, RwLock};

use axum::{
    extract::{Path, Query, Request},
    http::{header::AUTHORIZATION, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

// ---- HS256 secret (process-global so live rotation invalidates old tokens) ----

fn secret_cell() -> &'static RwLock<String> {
    static SECRET: OnceLock<RwLock<String>> = OnceLock::new();
    SECRET.get_or_init(|| RwLock::new(String::new()))
}

/// Install/replace the signing secret.
pub fn set_secret(s: &str) {
    if let Ok(mut g) = secret_cell().write() {
        *g = s.to_string();
    }
}

fn read_secret() -> String {
    secret_cell().read().map(|g| g.clone()).unwrap_or_default()
}

// ---- JWT ----

#[derive(serde::Serialize, serde::Deserialize)]
struct Claims {
    sub: String,
    iat: u64,
    exp: u64,
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn mint(secret: &str, ttl_secs: u64) -> Result<String, String> {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    let now = unix_now();
    let claims = Claims {
        sub: "shardx-api".into(),
        iat: now,
        exp: now.saturating_add(ttl_secs),
    };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| e.to_string())
}

fn verify(secret: &str, token: &str) -> bool {
    use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )
    .is_ok()
}

/// 10-year token shown in Settings UI.
pub fn long_lived_token(secret: &str) -> Result<String, String> {
    mint(secret, 60 * 60 * 24 * 365 * 10)
}

// ---- error type ----

struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

fn err(code: StatusCode, msg: impl Into<String>) -> ApiError {
    ApiError(code, msg.into())
}

type ApiResult = Result<Json<Value>, ApiError>;

// ---- auth middleware ----

async fn auth(req: Request, next: Next) -> Result<Response, StatusCode> {
    let secret = read_secret();
    let ok = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| {
            h.strip_prefix("Bearer ")
                .or_else(|| h.strip_prefix("bearer "))
        })
        .map(|t| verify(&secret, t.trim()))
        .unwrap_or(false);
    if ok {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

/// Hir-Login without a valid HLG- key is locked in the UI; the API is the
/// same app, so it answers nothing either. A JSON code rather than a bare
/// status, so a client (HirS, MCP) can say "activate Hir-Login" instead of
/// "something failed".
async fn license_gate(req: Request, next: Next) -> Response {
    if crate::license::allowed().await {
        next.run(req).await
    } else {
        (
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "license_required",
                "message": "Hir-Login chưa kích hoạt key. Mở Hir-Login và nhập key HLG- để tiếp tục.",
            })),
        )
            .into_response()
    }
}

/// The HirS tool marks its requests `X-Hir-Client: HirS`, and gets nothing
/// until the operator switches "Cho phép HirS kết nối" on in Settings. Read
/// from the file on every request, so the switch takes effect at once.
async fn hirs_gate(req: Request, next: Next) -> Response {
    if is_hirs(&req) && !hirs_allowed() {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "hirs_not_allowed",
                "message": "Hir-Login chưa cho phép HirS kết nối. Mở Hir-Login → Cài đặt → Nâng cao → bật \"Cho phép HirS kết nối\".",
            })),
        )
            .into_response();
    }
    next.run(req).await
}

fn is_hirs(req: &Request) -> bool {
    req.headers()
        .get("x-hir-client")
        .and_then(|h| h.to_str().ok())
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("hirs"))
}

fn hirs_allowed() -> bool {
    crate::settings::load().map(|s| s.hirs_allowed).unwrap_or(false)
}

// ---- HirS sees only the folders shared with it ----
//
// On top of the switch above, the owner shares folders one by one ("Chia sẻ cho HirS",
// see hirs_share.rs). HirS writes its own proxies into the profiles it uses, so it must never
// see or touch a folder that has proxies of its own. For requests marked `X-Hir-Client: HirS`:
// lists are filtered to shared folders; a profile outside them answers 403 folder_not_shared;
// a profile is created / moved only into a shared folder, or into a brand-new folder name
// (which is then shared, as it is HirS's own); routes HirS has no use for are refused.

fn hirs_refuse(status: StatusCode, code: &str, message: String) -> Response {
    (status, Json(json!({ "error": message, "message": message, "code": code }))).into_response()
}

fn folder_not_shared(folder: &str) -> Response {
    let message = if folder.is_empty() {
        "Profile này không nằm trong thư mục nào nên không chia sẻ cho HirS.".to_string()
    } else {
        format!("Thư mục \"{folder}\" chưa được chia sẻ cho HirS. Mở Hir-Login → Thư mục → bật \"Chia sẻ cho HirS\" cho thư mục này.")
    };
    hirs_refuse(StatusCode::FORBIDDEN, "folder_not_shared", message)
}

/// Every folder some profile carries — a name not among them is new.
fn existing_folders() -> std::collections::HashSet<String> {
    crate::profile::list_all()
        .map(|all| all.into_iter().map(|m| m.folder).filter(|f| !f.is_empty()).collect())
        .unwrap_or_default()
}

/// May HirS put a profile into `folder`? None = yes (and `Some(name)` in the Ok means "share it
/// once the request succeeds", for a new folder); Err = the refusal to send.
fn hirs_target_folder(folder: &str) -> Result<Option<String>, Response> {
    let folder = folder.trim();
    if folder.is_empty() {
        return Err(hirs_refuse(
            StatusCode::BAD_REQUEST,
            "folder_required",
            "HirS phải đặt profile vào một thư mục (đã chia sẻ, hoặc thư mục mới).".into(),
        ));
    }
    if crate::hirs_share::is_shared(folder) {
        return Ok(None);
    }
    if existing_folders().contains(folder) {
        return Err(folder_not_shared(folder));
    }
    Ok(Some(folder.to_string()))
}

fn profile_folder(id: &str) -> Option<String> {
    crate::profile::load_raw(id).ok().map(|p| p.meta.folder)
}

async fn hirs_scope(req: Request, next: Next) -> Response {
    if !is_hirs(&req) {
        return next.run(req).await;
    }
    use axum::http::Method;
    let method = req.method().clone();
    let path = req.uri().path().trim_end_matches('/').to_string();
    let seg: Vec<String> = path.split('/').filter(|s| !s.is_empty()).map(|s| {
        percent_decode(s)
    }).collect();
    let seg: Vec<&str> = seg.iter().map(String::as_str).collect();
    let shared = crate::hirs_share::list();
    let shared_set: std::collections::HashSet<&str> = shared.iter().map(String::as_str).collect();

    match (method.clone(), seg.as_slice()) {
        // Not folder-bound: a new fingerprint, the fingerprint library.
        (Method::GET, ["fingerprint", "new"]) | (Method::GET, ["fingerprint", "new", _]) | (Method::GET, ["fingerprints"]) => next.run(req).await,

        // Lists: only what is in a shared folder.
        (Method::GET, ["profiles"]) => {
            let res = next.run(req).await;
            filter_json_array(res, |row| row.get("folder").and_then(|f| f.as_str()).is_some_and(|f| shared_set.contains(f))).await
        }
        (Method::GET, ["running"]) => {
            let folders: std::collections::HashMap<String, String> = crate::profile::list_all()
                .map(|all| all.into_iter().map(|m| (m.id, m.folder)).collect())
                .unwrap_or_default();
            let res = next.run(req).await;
            filter_json_array(res, |row| {
                row.get("profile_id").and_then(|i| i.as_str()).and_then(|i| folders.get(i)).is_some_and(|f| shared_set.contains(f.as_str()))
            })
            .await
        }
        // Shared folders, including ones with no profile yet.
        (Method::GET, ["folders"]) => Json(json!(shared)).into_response(),

        // Creating a profile.
        (Method::POST, ["profiles"]) => {
            let (req, body) = match take_json(req).await { Ok(x) => x, Err(r) => return r };
            let folder = body.get("folder").and_then(|f| f.as_str()).unwrap_or("");
            run_into_folder(req, next, folder).await
        }
        (Method::POST, ["folders", folder, "profiles"]) => {
            let folder = folder.to_string();
            run_into_folder(req, next, &folder).await
        }

        // A folder itself: only a shared one (a rename keeps the share — profile::rename_folder).
        (Method::DELETE, ["folders", folder]) => {
            if shared_set.contains(*folder) { next.run(req).await } else { folder_not_shared(folder) }
        }
        // A rename onto an existing unshared folder would merge into it and share it: the new
        // name must be shared already or brand new.
        (Method::PATCH, ["folders", folder]) => {
            if !shared_set.contains(*folder) {
                return folder_not_shared(folder);
            }
            let (req, body) = match take_json(req).await { Ok(x) => x, Err(r) => return r };
            let target = body.get("name").and_then(|f| f.as_str()).unwrap_or("").trim().to_string();
            if target != *folder {
                if let Err(r) = hirs_target_folder(&target) {
                    return r;
                }
            }
            next.run(req).await
        }

        // One profile: only inside a shared folder; moving it needs a shared or new target.
        (_, ["profiles", id, rest @ ..]) if *id != "temporary" => {
            let current = match profile_folder(id) {
                Some(f) => f,
                None => return next.run(req).await, // unknown id: the handler's own 404
            };
            if !shared_set.contains(current.as_str()) {
                return folder_not_shared(&current);
            }
            let allowed = matches!(
                (&method, rest),
                (&Method::GET, []) | (&Method::PATCH, []) | (&Method::DELETE, []) | (&Method::POST, ["start"]) | (&Method::POST, ["stop"])
            );
            if !allowed {
                return hirs_route_refused();
            }
            if method == Method::PATCH {
                let (req, body) = match take_json(req).await { Ok(x) => x, Err(r) => return r };
                if let Some(target) = body.get("folder").and_then(|f| f.as_str()) {
                    return run_into_folder(req, next, target).await;
                }
                return next.run(req).await;
            }
            next.run(req).await
        }

        _ => hirs_route_refused(),
    }
}

fn hirs_route_refused() -> Response {
    hirs_refuse(StatusCode::FORBIDDEN, "hirs_route_not_allowed", "HirS không được dùng chức năng này của Hir-Login.".into())
}

/// Runs a create / move into `folder` if HirS may use it, sharing a brand-new folder once the
/// request has gone through.
async fn run_into_folder(req: Request, next: Next, folder: &str) -> Response {
    let share_after = match hirs_target_folder(folder) {
        Ok(s) => s,
        Err(r) => return r,
    };
    let res = next.run(req).await;
    if let Some(f) = share_after {
        if res.status().is_success() {
            let _ = crate::hirs_share::set(&f, true);
        }
    }
    res
}

/// Reads the JSON body and puts it back, so the handler still gets it.
async fn take_json(req: Request) -> Result<(Request, Value), Response> {
    let (parts, body) = req.into_parts();
    let bytes = axum::body::to_bytes(body, 32 * 1024 * 1024)
        .await
        .map_err(|_| err(StatusCode::BAD_REQUEST, "body too large or unreadable").into_response())?;
    let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    Ok((Request::from_parts(parts, axum::body::Body::from(bytes)), value))
}

/// Keeps only the rows of a JSON array answer that `keep` accepts.
async fn filter_json_array(res: Response, keep: impl Fn(&Value) -> bool) -> Response {
    if !res.status().is_success() {
        return res;
    }
    let (parts, body) = res.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, 64 * 1024 * 1024).await else {
        return err(StatusCode::INTERNAL_SERVER_ERROR, "unreadable answer").into_response();
    };
    match serde_json::from_slice::<Value>(&bytes) {
        Ok(Value::Array(rows)) => {
            let kept: Vec<Value> = rows.into_iter().filter(|r| keep(r)).collect();
            let mut out = Json(Value::Array(kept)).into_response();
            *out.status_mut() = parts.status;
            out
        }
        _ => Response::from_parts(parts, axum::body::Body::from(bytes)),
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ---- handlers ----

async fn health() -> Json<Value> {
    Json(json!({
        "ok": true,
        "name": "hir-login",
        "version": env!("CARGO_PKG_VERSION"),
        "hirs_allowed": hirs_allowed(),
        // The folders shared with HirS ("Chia sẻ cho HirS"); HirS sees nothing outside them.
        "hirs_folders": crate::hirs_share::list(),
        // POST /profiles/{id}/start understands {"proxy": "..."} for that launch only.
        "start_proxy": true,
    }))
}

async fn list_profiles() -> ApiResult {
    let metas = crate::profile::list_all().map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let running = crate::process::Tracker::shared().running();
    let by_id: std::collections::HashMap<String, crate::process::RunningProfile> =
        running.into_iter().map(|r| (r.profile_id.clone(), r)).collect();
    // Team sync: which profiles another machine has open (or is still saving) right now — a
    // client can grey those out instead of finding out from a refused /start.
    let elsewhere: std::collections::HashMap<String, crate::cloud_sync::ElsewhereLock> =
        crate::cloud_sync::activity().elsewhere.into_iter().map(|l| (l.id.clone(), l)).collect();
    let close_sync = crate::cloud_sync::CloseSync::now();
    let out: Vec<Value> = metas
        .into_iter()
        .map(|m| {
            let r = by_id.get(&m.id);
            json!({
                "id": m.id,
                "name": m.name,
                "notes": m.notes,
                "proxy_id": m.proxy_id,
                "last_launched_at": m.last_launched_at,
                "created_at": m.created_at,
                "pinned": m.pinned,
                "folder": m.folder,
                "color": m.color,
                "extensions": m.extensions,
                "running": r.is_some(),
                "pid": r.map(|x| x.pid),
                "cdp": r.and_then(|x| x.cdp.clone()),
                "in_use_elsewhere": elsewhere.get(&m.id).map(|l| json!({ "by": l.by, "saving": l.saving })),
                // This machine's last close of it, with the team: ok | pending | failed | none.
                "sync": close_sync.of(&m.id),
            })
        })
        .collect();
    Ok(Json(json!(out)))
}

async fn get_profile(Path(id): Path<String>) -> ApiResult {
    let stored = crate::profile::load_raw(&id)
        .map_err(|e| err(StatusCode::NOT_FOUND, e.to_string()))?;
    let mut val = serde_json::to_value(stored)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if let Some(cdp) = crate::process::Tracker::shared().cdp(&id) {
        if let Some(obj) = val.as_object_mut() {
            obj.insert("running".into(), json!(true));
            obj.insert("cdp".into(), serde_json::to_value(cdp).unwrap_or(Value::Null));
        }
    }
    Ok(Json(val))
}

// ---- get-new-fingerprint ----

/// Uniquified fingerprint without persisting; create-profile stores verbatim.
async fn new_fingerprint() -> ApiResult {
    new_fingerprint_impl(None).await
}

async fn new_fingerprint_for(Path(platform): Path<String>) -> ApiResult {
    new_fingerprint_impl(Some(platform)).await
}

async fn new_fingerprint_impl(platform: Option<String>) -> ApiResult {
    let fid = random_fingerprint_for(platform.as_deref())?;
    let mut cfg = crate::build_fingerprint_config(crate::main_window().as_ref(), &fid)
        .map_err(|e| err(StatusCode::BAD_REQUEST, e))?;
    cfg.remove("_meta");
    Ok(Json(json!({ "fingerprint": cfg })))
}

// ---- create-profile ----

#[derive(Deserialize)]
struct CreateReq {
    name: Option<String>,
    notes: Option<String>,
    proxy_id: Option<String>,
    /// Proxy string: added to store + full-tested, bound by id.
    proxy: Option<String>,
    folder: Option<String>,
    /// Icon and omnibox-pill accent, `#rrggbb`. Omitted = derived from the name.
    color: Option<String>,
    /// Extension-library ids to load at launch (`GET /extensions`).
    extensions: Option<Vec<String>>,
    /// Claimed display refresh rate in Hz (24-480). Absent = the engine's 60.
    refresh_rate: Option<i64>,
    fingerprint: Value,
}

/// Persist verbatim (enrich=false); proxy_id binds, proxy string upserts+tests.
async fn persist_created(folder_override: Option<String>, body: CreateReq) -> ApiResult {
    let mut cfg = body
        .fingerprint
        .as_object()
        .cloned()
        .ok_or_else(|| err(StatusCode::BAD_REQUEST, "`fingerprint` must be an object"))?;
    cfg.remove("_meta");
    if let Some(n) = body.name.as_ref() {
        cfg.insert("name".into(), json!(n));
    }
    if let Some(n) = body.notes.as_ref() {
        cfg.insert("notes".into(), json!(n));
    }

    let folder = folder_override.or(body.folder).unwrap_or_default();
    let mut meta = json!({ "id": "", "folder": folder });
    if let Some(c) = body.color.as_ref().filter(|c| !c.is_empty()) {
        meta["color"] = json!(c);
    }
    if let Some(ids) = body.extensions.as_ref() {
        validate_extension_ids(ids)?;
        meta["extensions"] = json!(ids);
    }
    if let Some(pid) = body.proxy_id.as_ref() {
        meta["proxy_id"] = json!(pid);
    } else if let Some(pstr) = body.proxy.as_ref() {
        let entry = crate::proxy::parse_single(pstr)
            .ok_or_else(|| err(StatusCode::BAD_REQUEST, format!("unparseable proxy: {pstr}")))?;
        let stored = crate::proxy::upsert_dedup(entry)
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        // Best-effort full test (UDP + geo); launch re-probes UDP live anyway.
        let _ = crate::proxy::full_test(&stored).await;
        meta["proxy_id"] = json!(stored.id);
        crate::notify_store_changed("proxies");
    }
    crate::ensure_default_noise(&mut cfg);
    if let Some(hz) = body.refresh_rate {
        apply_refresh_rate(&mut cfg, hz).map_err(|e| err(StatusCode::BAD_REQUEST, e))?;
    }
    cfg.insert("_meta".into(), meta);

    let pm = crate::save_profile_core(crate::main_window().as_ref(), Value::Object(cfg), false)
        .map_err(|e| err(StatusCode::BAD_REQUEST, e))?;
    crate::notify_store_changed("profiles");
    Ok(Json(serde_json::to_value(pm).unwrap_or(Value::Null)))
}

async fn create_profile(Json(body): Json<CreateReq>) -> ApiResult {
    persist_created(None, body).await
}

async fn create_profile_in_folder(Path(folder): Path<String>, Json(body): Json<CreateReq>) -> ApiResult {
    persist_created(Some(folder), body).await
}

// ---- temporary profiles ----

#[derive(Deserialize)]
struct TempReq {
    fingerprint_id: Option<String>,
    platform: Option<String>,
    /// Inline proxy (not stored).
    proxy: Option<String>,
    name: Option<String>,
    folder: Option<String>,
    /// Per vector: `{"canvas": true}` or a full block. Omitted vectors stay off.
    noise: Option<Value>,
    /// Claimed display refresh rate in Hz (24-480). Absent = the engine's 60.
    refresh_rate: Option<i64>,
}

/// Temporary profile (hidden, auto-deleted on close); pair with /start.
async fn create_temporary(Json(body): Json<TempReq>) -> ApiResult {
    let fid = match body.fingerprint_id {
        Some(f) => f,
        None => random_fingerprint_for(body.platform.as_deref())?,
    };
    let mut cfg = crate::build_fingerprint_config(crate::main_window().as_ref(), &fid)
        .map_err(|e| err(StatusCode::BAD_REQUEST, e))?;
    cfg.remove("_meta");
    if let Some(n) = body.name.as_ref() {
        cfg.insert("name".into(), json!(n));
    }
    crate::ensure_default_noise(&mut cfg);
    if let Some(n) = body.noise.as_ref() {
        apply_noise_overrides(&mut cfg, n)
            .map_err(|e| err(StatusCode::BAD_REQUEST, e))?;
    }
    if let Some(hz) = body.refresh_rate {
        apply_refresh_rate(&mut cfg, hz).map_err(|e| err(StatusCode::BAD_REQUEST, e))?;
    }
    let mut meta = json!({ "id": "", "folder": body.folder.unwrap_or_default(), "temporary": true });
    if let Some(pstr) = body.proxy.as_ref() {
        let entry = crate::proxy::parse_single(pstr)
            .ok_or_else(|| err(StatusCode::BAD_REQUEST, format!("unparseable proxy: {pstr}")))?;
        meta["inline_proxy"] = serde_json::to_value(entry).unwrap_or(Value::Null);
    }
    cfg.insert("_meta".into(), meta);

    let pm = crate::save_profile_core(crate::main_window().as_ref(), Value::Object(cfg), false)
        .map_err(|e| err(StatusCode::BAD_REQUEST, e))?;
    Ok(Json(json!({
        "id": pm.id,
        "name": pm.name,
        "fingerprint_id": fid,
        "temporary": true,
        "proxy_inline": body.proxy.is_some(),
    })))
}

/// Write `screen.refresh_rate` into a config. Its own field rather than part of
/// `fingerprint` because it is the one piece of the screen block an operator
/// sets on its own: a page reads it by timing frames, not by asking.
fn apply_refresh_rate(cfg: &mut serde_json::Map<String, Value>, hz: i64) -> Result<(), String> {
    if !(24..=480).contains(&hz) {
        return Err(format!("refresh_rate must be between 24 and 480, got {hz}"));
    }
    let screen = cfg
        .entry("screen")
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    match screen.as_object_mut() {
        Some(o) => {
            o.insert("refresh_rate".into(), json!(hz));
            Ok(())
        }
        None => Err("`screen` is not an object".into()),
    }
}

/// Merge a caller's noise request into the config's block. Merged, not
/// replaced: `{"webgl": true}` must not zero its intensity.
fn apply_noise_overrides(
    cfg: &mut serde_json::Map<String, Value>,
    req: &Value,
) -> Result<(), String> {
    const VECTORS: &[&str] = &["canvas", "webgl", "audio", "client_rects", "sensors", "fonts"];
    let req = req.as_object().ok_or("`noise` must be an object")?;
    let noise = cfg
        .get_mut("noise")
        .and_then(|n| n.as_object_mut())
        .ok_or("profile has no noise block")?;

    for (key, val) in req {
        let key = key.as_str();
        if !VECTORS.contains(&key) {
            return Err(format!("unknown noise vector `{key}`"));
        }
        let slot = noise
            .entry(key.to_string())
            .or_insert_with(|| json!({ "enabled": false, "seed": 0 }));
        let Some(slot) = slot.as_object_mut() else { continue };
        match val {
            Value::Bool(on) => {
                slot.insert("enabled".into(), json!(on));
                // The defaults the UI writes for the two vectors that carry a
                // strength; turning one on with a zero strength is a no-op.
                if *on && key == "webgl" && slot.get("intensity").and_then(|v| v.as_f64()) == Some(0.0) {
                    slot.insert("intensity".into(), json!(0.0005));
                }
                if *on && key == "client_rects" && slot.get("max_offset").and_then(|v| v.as_f64()) == Some(0.0) {
                    slot.insert("max_offset".into(), json!(1));
                }
            }
            Value::Object(fields) => {
                for (k, v) in fields {
                    slot.insert(k.clone(), v.clone());
                }
            }
            _ => return Err(format!("`noise.{key}` must be a bool or an object")),
        }
    }
    Ok(())
}

/// Into the trash, like the UI's delete. Temporary profiles are torn down by
/// the Tracker and never reach it.
async fn delete_profile(Path(id): Path<String>) -> ApiResult {
    crate::trash::move_to_trash(&id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    crate::notify_store_changed("profiles");
    Ok(Json(json!({ "deleted": true, "id": id, "trashed": true })))
}

#[derive(Deserialize)]
struct EditReq {
    name: Option<String>,
    notes: Option<String>,
    /// "" unfiles.
    folder: Option<String>,
    /// "" unbinds.
    proxy_id: Option<String>,
    /// Proxy string: stored + tested, then bound.
    proxy: Option<String>,
    /// `#rrggbb`; "" goes back to the name-derived colour.
    color: Option<String>,
    /// Replaces the whole list; `[]` loads none.
    extensions: Option<Vec<String>>,
    /// Claimed display refresh rate in Hz (24-480); applied after `fingerprint`.
    refresh_rate: Option<i64>,
    /// Replace stored fingerprint verbatim.
    fingerprint: Option<Value>,
}

/// Reject unknown ids rather than writing a profile that launches without them.
fn validate_extension_ids(ids: &[String]) -> Result<(), ApiError> {
    let known: std::collections::HashSet<String> = crate::extensions::list()
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .into_iter()
        .map(|e| e.id)
        .collect();
    let missing: Vec<&str> = ids
        .iter()
        .filter(|id| !known.contains(*id))
        .map(|s| s.as_str())
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(err(
            StatusCode::BAD_REQUEST,
            format!("no such extension: {}", missing.join(", ")),
        ))
    }
}

/// Edit profile; only provided fields change. Returns the updated profile.
async fn edit_profile(Path(id): Path<String>, Json(body): Json<EditReq>) -> ApiResult {
    let mut stored = crate::profile::load_raw(&id)
        .map_err(|e| err(StatusCode::NOT_FOUND, e.to_string()))?;

    if let Some(fp) = body.fingerprint {
        let mut cfg = fp
            .as_object()
            .cloned()
            .ok_or_else(|| err(StatusCode::BAD_REQUEST, "`fingerprint` must be an object"))?;
        cfg.remove("_meta");
        stored.config = cfg;
    }
    if let Some(n) = body.name.as_ref() {
        stored.config.insert("name".into(), json!(n));
    }
    if let Some(n) = body.notes.as_ref() {
        stored.config.insert("notes".into(), json!(n));
    }
    if let Some(pid) = body.proxy_id.as_ref() {
        crate::profile::bind_proxy_stamped(&mut stored.meta, if pid.is_empty() { None } else { Some(pid.clone()) });
        stored.meta.inline_proxy = None;
    } else if let Some(pstr) = body.proxy.as_ref() {
        let entry = crate::proxy::parse_single(pstr)
            .ok_or_else(|| err(StatusCode::BAD_REQUEST, format!("unparseable proxy: {pstr}")))?;
        let s = crate::proxy::upsert_dedup(entry)
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let _ = crate::proxy::full_test(&s).await;
        crate::profile::bind_proxy_stamped(&mut stored.meta, Some(s.id));
        stored.meta.inline_proxy = None;
        crate::notify_store_changed("proxies");
    }
    if let Some(c) = body.color.as_ref() {
        // "" is how a caller asks for the derived colour back.
        stored.meta.color = (!c.is_empty()).then(|| c.clone());
    }
    if let Some(ids) = body.extensions.as_ref() {
        validate_extension_ids(ids)?;
        stored.meta.extensions = ids.clone();
    }
    if let Some(hz) = body.refresh_rate {
        apply_refresh_rate(&mut stored.config, hz)
            .map_err(|e| err(StatusCode::BAD_REQUEST, e))?;
    }

    crate::profile::save_raw(&mut stored)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    // set_folder handles unfile; save_raw keeps the existing folder when empty.
    if let Some(f) = body.folder.as_ref() {
        crate::profile::set_folder(&id, f)
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    let updated = crate::profile::load_raw(&id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    crate::notify_store_changed("profiles");
    Ok(Json(serde_json::to_value(updated).unwrap_or(Value::Null)))
}

#[derive(Deserialize)]
struct RenameFolderReq {
    name: String,
}

async fn rename_folder_ep(Path(folder): Path<String>, Json(body): Json<RenameFolderReq>) -> ApiResult {
    let n = crate::profile::rename_folder(&folder, &body.name)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    crate::notify_store_changed("profiles");
    Ok(Json(json!({ "renamed_to": body.name, "profiles": n })))
}

#[derive(Deserialize)]
struct DeleteFolderQuery {
    /// true → delete profiles; false (default) → unfile.
    #[serde(default)]
    delete_profiles: bool,
}

async fn delete_folder_ep(Path(folder): Path<String>, Query(q): Query<DeleteFolderQuery>) -> ApiResult {
    let n = crate::profile::delete_folder(&folder, q.delete_profiles)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    crate::notify_store_changed("profiles");
    Ok(Json(json!({
        "deleted_folder": folder,
        "delete_profiles": q.delete_profiles,
        "profiles": n,
    })))
}

#[derive(Deserialize, Default)]
struct StartReq {
    #[serde(default)]
    headless: bool,
    /// A proxy for this launch only ("http://127.0.0.1:42101", "socks5://user:pass@host:port"),
    /// used in place of the profile's own. Never saved to the profile, the proxy store or the
    /// team; the time zone / language / location it leads to are not pinned either.
    #[serde(default)]
    proxy: Option<String>,
}

/// The team refused to hand this profile over because another machine has it open (or is
/// still saving it): a 409 a client can act on, worded for the person. `error` carries the
/// sentence too, so a client that only shows `error` already says the right thing.
fn in_use_response(id: &str, launch_error: &str) -> Option<Response> {
    let saving = launch_error.contains("being saved by");
    if !saving && !launch_error.contains("is in use by") {
        return None;
    }
    let mine = launch_error.contains("by you on another machine");
    // The member, from the team's last report — or, before that has come in, from the refusal
    // itself ("… in use by Admin — try again …").
    let from_text = || {
        ["in use by ", "being saved by "].iter().find_map(|k| {
            let rest = &launch_error[launch_error.find(k)? + k.len()..];
            let name = rest.split(" —").next().unwrap_or("").trim();
            (!name.is_empty() && name != "you on another machine").then(|| name.to_string())
        })
    };
    let by = crate::cloud_sync::activity()
        .elsewhere
        .into_iter()
        .find(|l| l.id == id)
        .map(|l| l.by)
        .or_else(from_text)
        .unwrap_or_default();
    let who = if by.is_empty() { "Thành viên khác".to_string() } else { by.clone() };
    // The same sentences the app's own table shows (locales/vi.json err.profileInUse…), so a
    // client and the app never word one refusal two ways.
    let message = match (mine, saving) {
        (true, true) => "Bạn vừa đóng profile này ở máy khác và nó đang được đồng bộ. Đợi vài giây cho xong rồi mở lại.".to_string(),
        (true, false) => "Chính bạn đang mở profile này ở một máy khác. Hãy đóng nó ở máy đó rồi thử lại.".to_string(),
        (false, true) => format!("{who} vừa đóng profile này và nó đang được đồng bộ lên nhóm. Đợi vài giây cho xong rồi mở lại."),
        (false, false) => format!("Profile này đang được {who} mở. Hãy đợi người đó đóng profile rồi thử lại."),
    };
    Some(
        (
            StatusCode::CONFLICT,
            Json(json!({ "error": message, "message": message, "code": "profile_in_use", "by": by, "saving": saving })),
        )
            .into_response(),
    )
}

/// Launch with CDP; body `{ "headless": true }` opt-in, `{ "proxy": "..." }` for this launch
/// only (the answer then says `"proxy_override": true`).
async fn start_profile(Path(id): Path<String>, body: Option<Json<StartReq>>) -> Response {
    let req = body.map(|Json(b)| b).unwrap_or_default();
    let headless = req.headless;
    let override_proxy = match req.proxy.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None => None,
        Some(s) => match crate::proxy::parse_single(s) {
            Some(p) => Some(p),
            None => {
                let message = format!("Proxy cho lần mở này không đúng dạng: {s}");
                return (StatusCode::BAD_REQUEST, Json(json!({ "error": message, "message": message, "code": "bad_proxy" }))).into_response();
            }
        },
    };
    let proxy_override = override_proxy.is_some();
    let launched = match override_proxy {
        Some(p) => crate::launch::launch_profile_via(&id, true, headless, p).await,
        None => crate::launch::launch_profile(&id, true, headless).await,
    };
    let outcome = match launched {
        Ok(o) => o,
        Err(e) => {
            let text = e.to_string();
            return in_use_response(&id, &format!("{e:#}"))
                .unwrap_or_else(|| err(StatusCode::INTERNAL_SERVER_ERROR, text).into_response());
        }
    };
    Json(json!({
        "profile_id": id,
        "pid": outcome.pid,
        "headless": headless,
        "cdp": outcome.cdp,
        "cdp_error": outcome.cdp_error,
        "proxy_override": proxy_override,
    }))
    .into_response()
}

/// Answers once the browser has gone and its close has been saved to the team (or after 15 s);
/// `sync` says how that went: "ok" | "pending" (still uploading — poll /profiles `sync`) |
/// "failed" (the close did not reach the team; the next close sends it again) | "none" (no team).
async fn stop_profile(Path(id): Path<String>) -> ApiResult {
    let stopped = crate::process::Tracker::shared()
        .kill(&id)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({ "profile_id": id, "stopped": stopped, "sync": crate::cloud_sync::close_sync_state(&id) })))
}

async fn export_cookies(Path(id): Path<String>) -> ApiResult {
    let cookies = crate::cookies::export(&id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({ "cookies": cookies })))
}

#[derive(Deserialize)]
struct ImportCookiesReq {
    #[serde(default)]
    cookies: Vec<crate::cookies::Cookie>,
    /// Alternatively the text of a cookie file in any supported shape (JSON, Netscape
    /// cookies.txt, a Facebook cookie string) — see `cookies::parse_any`.
    #[serde(default)]
    text: Option<String>,
}

async fn import_cookies(Path(id): Path<String>, Json(body): Json<ImportCookiesReq>) -> ApiResult {
    // Running browser would clobber imports on exit.
    if crate::is_profile_running(&id) {
        return Err(err(
            StatusCode::CONFLICT,
            "stop the profile before importing cookies",
        ));
    }
    let list = match body.text.as_deref() {
        Some(t) => crate::cookies::parse_any(t).map_err(|e| err(StatusCode::BAD_REQUEST, format!("{e:#}")))?,
        None => body.cookies,
    };
    let n = crate::cookies::import(&id, &list)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({ "imported": n })))
}

async fn list_running() -> Json<Value> {
    Json(json!(crate::process::Tracker::shared().running()))
}

async fn list_fingerprints() -> ApiResult {
    let all = crate::fingerprints::list_all()
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let out: Vec<Value> = all
        .into_iter()
        .map(|e| {
            json!({
                "id": e.id,
                "label": e.label,
                "platform": e.platform,
                "chrome": e.chrome,
                "gpu": e.gpu,
                "builtin": e.builtin,
            })
        })
        .collect();
    Ok(Json(json!(out)))
}

#[derive(Deserialize)]
struct AddProxyReq {
    /// "scheme://user:pass@host:port" or "host:port:user:pass"; wins over fields.
    proxy: Option<String>,
    /// socks5 | http | https (default socks5).
    kind: Option<String>,
    host: Option<String>,
    port: Option<u16>,
    username: Option<String>,
    password: Option<String>,
    name: Option<String>,
    country: Option<String>,
    notes: Option<String>,
}

/// Add proxy (deduped by endpoint); returns summary.
async fn add_proxy(Json(body): Json<AddProxyReq>) -> ApiResult {
    let mut entry = if let Some(s) = body.proxy.as_ref() {
        crate::proxy::parse_single(s)
            .ok_or_else(|| err(StatusCode::BAD_REQUEST, format!("unparseable proxy: {s}")))?
    } else {
        let host = body
            .host
            .clone()
            .ok_or_else(|| err(StatusCode::BAD_REQUEST, "`proxy` string or host+port required"))?;
        let port = body
            .port
            .ok_or_else(|| err(StatusCode::BAD_REQUEST, "`port` required"))?;
        let kind = match body.kind.as_deref() {
            Some("http") => crate::proxy::ProxyKind::Http,
            Some("https") => crate::proxy::ProxyKind::Https,
            _ => crate::proxy::ProxyKind::Socks5,
        };
        crate::proxy::ProxyEntry {
            id: String::new(),
            name: String::new(),
            kind,
            host,
            port,
            username: body.username.clone().unwrap_or_default(),
            password: body.password.clone().unwrap_or_default(),
            country: String::new(),
            notes: String::new(),
        }
    };
    // metadata overrides (applied to parsed entries too).
    if let Some(n) = body.name.filter(|s| !s.is_empty()) {
        entry.name = n;
    }
    if let Some(c) = body.country {
        entry.country = c;
    }
    if let Some(nt) = body.notes {
        entry.notes = nt;
    }
    let stored = crate::proxy::upsert_dedup(entry)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    crate::notify_store_changed("proxies");
    Ok(Json(json!({
        "id": stored.id,
        "name": stored.name,
        "kind": stored.kind,
        "host": stored.host,
        "port": stored.port,
        "country": stored.country,
    })))
}

async fn delete_proxy(Path(id): Path<String>) -> ApiResult {
    crate::proxy::delete(&id).map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    crate::notify_store_changed("proxies");
    Ok(Json(json!({ "deleted": true, "id": id })))
}

async fn list_proxies() -> ApiResult {
    let list = crate::proxy::list().map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    // Credentials never exposed over API.
    let out: Vec<Value> = list
        .into_iter()
        .map(|p| {
            json!({
                "id": p.id,
                "name": p.name,
                "kind": p.kind,
                "host": p.host,
                "port": p.port,
                "country": p.country,
            })
        })
        .collect();
    Ok(Json(json!(out)))
}

// ---- extensions ----

fn extension_json(e: &crate::extensions::ExtensionEntry, with_icon: bool) -> Value {
    json!({
        "id": e.id,
        "name": e.name,
        "version": e.version,
        "description": e.description,
        "size_bytes": e.size_bytes,
        "added_at": e.added_at,
        // Data URLs run to hundreds of kilobytes each; a listing of thirty
        // extensions is not the place for them.
        "icon": with_icon.then(|| e.icon.clone()),
    })
}

async fn list_extensions() -> ApiResult {
    let list = crate::extensions::list()
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let out: Vec<Value> = list.iter().map(|e| extension_json(e, false)).collect();
    Ok(Json(json!(out)))
}

#[derive(Deserialize)]
struct AddExtensionReq {
    /// Web Store page, a bare extension id, or a direct .crx / .zip link.
    url: Option<String>,
    /// Local .crx / .zip file, or an unpacked folder, on this machine.
    path: Option<String>,
}

/// Add to the library. The response carries the icon, since the caller has
/// just asked for exactly this one.
async fn add_extension(Json(body): Json<AddExtensionReq>) -> ApiResult {
    let entry = match (body.url.as_deref(), body.path.as_deref()) {
        (Some(u), _) if !u.is_empty() => crate::extensions::import_url(u)
            .await
            .map_err(|e| err(StatusCode::BAD_REQUEST, format!("{e:#}")))?,
        (_, Some(p)) if !p.is_empty() => {
            crate::extensions::import(std::path::Path::new(p))
                .map_err(|e| err(StatusCode::BAD_REQUEST, format!("{e:#}")))?
        }
        _ => return Err(err(StatusCode::BAD_REQUEST, "`url` or `path` required")),
    };
    crate::notify_store_changed("extensions");
    Ok(Json(extension_json(&entry, true)))
}

async fn delete_extension(Path(id): Path<String>) -> ApiResult {
    crate::extensions::delete(&id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    crate::notify_store_changed("extensions");
    Ok(Json(json!({ "deleted": true, "id": id })))
}

// ---- bookmarks ----

async fn list_bookmarks() -> ApiResult {
    let list = crate::bookmarks::list()
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::to_value(list).unwrap_or(Value::Null)))
}

#[derive(Deserialize)]
struct BookmarkReq {
    /// Present = update that bookmark; absent = a new one.
    id: Option<String>,
    url: String,
    /// Blank uses the address itself.
    title: Option<String>,
    /// Launcher folder; "" (or absent) means every profile.
    folder: Option<String>,
}

/// Upsert. Profiles pick the change up on their next launch, not immediately —
/// the bookmarks file is written just before the browser reads it.
async fn save_bookmark(Json(body): Json<BookmarkReq>) -> ApiResult {
    let saved = crate::bookmarks::save(crate::bookmarks::Bookmark {
        id: body.id.unwrap_or_default(),
        title: body.title.unwrap_or_default(),
        url: body.url,
        folder: body.folder.unwrap_or_default(),
    })
    .map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?;
    crate::notify_store_changed("bookmarks");
    Ok(Json(serde_json::to_value(saved).unwrap_or(Value::Null)))
}

async fn delete_bookmark(Path(id): Path<String>) -> ApiResult {
    crate::bookmarks::delete(&id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    crate::notify_store_changed("bookmarks");
    Ok(Json(json!({ "deleted": true, "id": id })))
}

// ---- trash ----

async fn list_trash() -> ApiResult {
    let list = crate::trash::list()
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::to_value(list).unwrap_or(Value::Null)))
}

/// Puts the profile back under its own id, so anything that referenced it
/// still points at it.
async fn restore_trash(Path(id): Path<String>) -> ApiResult {
    let meta = crate::trash::restore(&id)
        .map_err(|e| err(StatusCode::NOT_FOUND, e.to_string()))?;
    crate::notify_store_changed("profiles");
    Ok(Json(serde_json::to_value(meta).unwrap_or(Value::Null)))
}

/// Deletes the archive for good. There is nothing after this.
async fn purge_trash(Path(id): Path<String>) -> ApiResult {
    crate::trash::purge(&id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    crate::notify_store_changed("trash");
    Ok(Json(json!({ "purged": true, "id": id })))
}

async fn list_folders() -> ApiResult {
    let metas = crate::profile::list_all().map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let mut set = std::collections::BTreeSet::new();
    for m in metas {
        if !m.folder.is_empty() {
            set.insert(m.folder);
        }
    }
    Ok(Json(json!(set.into_iter().collect::<Vec<_>>())))
}

/// Normalize platform string to library tag vocabulary.
fn normalize_platform(p: &str) -> String {
    match p.trim().to_lowercase().as_str() {
        "windows" | "win" => "Windows".into(),
        "linux" => "Linux".into(),
        "mac" | "macos" | "osx" | "darwin" => "macOS".into(),
        other => other.to_string(),
    }
}

/// Random fingerprint id for platform (host OS when None); falls back to all.
fn random_fingerprint_for(platform: Option<&str>) -> Result<String, ApiError> {
    let want = platform
        .map(normalize_platform)
        .unwrap_or_else(crate::host_platform);
    let all = crate::fingerprints::list_all()
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if all.is_empty() {
        return Err(err(StatusCode::NOT_FOUND, "fingerprint library is empty"));
    }
    let matching: Vec<crate::fingerprints::LibraryEntry> = all
        .iter()
        .filter(|e| e.platform.eq_ignore_ascii_case(&want))
        .cloned()
        .collect();
    let pool = if matching.is_empty() { all } else { matching };
    let idx = (uuid::Uuid::new_v4().as_bytes()[0] as usize) % pool.len();
    Ok(pool[idx].id.clone())
}

// ---- automation ----
//
// Storage answers in every build; running only where the `automation` feature
// is compiled in.

async fn list_projects() -> ApiResult {
    let list = crate::automation::list()
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::to_value(list).unwrap_or(Value::Null)))
}

#[derive(Deserialize)]
struct CreateProjectReq {
    #[serde(default)]
    name: String,
}

async fn create_project(body: Option<Json<CreateProjectReq>>) -> ApiResult {
    let name = body.map(|Json(b)| b.name).unwrap_or_default();
    let project = crate::automation::create(&name)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    crate::notify_store_changed("automation");
    Ok(Json(serde_json::to_value(project).unwrap_or(Value::Null)))
}

fn find_project(id: &str) -> Result<crate::automation::Project, ApiError> {
    crate::automation::list()
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "no such project"))
}

async fn get_project(Path(id): Path<String>) -> ApiResult {
    Ok(Json(serde_json::to_value(find_project(&id)?).unwrap_or(Value::Null)))
}

/// Whole-project replace. The id in the path wins over the body's, and an
/// unknown id is a 404 rather than a silent create.
async fn save_project(Path(id): Path<String>, Json(mut body): Json<Value>) -> ApiResult {
    find_project(&id)?;
    if let Some(obj) = body.as_object_mut() {
        obj.insert("id".into(), Value::String(id.clone()));
    }
    let project: crate::automation::Project = serde_json::from_value(body)
        .map_err(|e| err(StatusCode::BAD_REQUEST, format!("that is not a project: {e}")))?;
    let saved = crate::automation::save(project)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    crate::notify_store_changed("automation");
    Ok(Json(serde_json::to_value(saved).unwrap_or(Value::Null)))
}

async fn delete_project(Path(id): Path<String>) -> ApiResult {
    find_project(&id)?;
    crate::automation::delete(&id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    crate::notify_store_changed("automation");
    Ok(Json(json!({ "deleted": true, "id": id })))
}

async fn duplicate_project(Path(id): Path<String>) -> ApiResult {
    let copy = crate::automation::duplicate(&id)
        .map_err(|e| err(StatusCode::NOT_FOUND, e.to_string()))?;
    crate::notify_store_changed("automation");
    Ok(Json(serde_json::to_value(copy).unwrap_or(Value::Null)))
}

/// Export strips every parameter marked secret, so a bundle carries no passwords.
async fn export_project(Path(id): Path<String>) -> ApiResult {
    let bundle = crate::automation::export(&id)
        .map_err(|e| err(StatusCode::NOT_FOUND, e.to_string()))?;
    Ok(Json(serde_json::to_value(bundle).unwrap_or(Value::Null)))
}

async fn import_project(Json(body): Json<Value>) -> ApiResult {
    let bundle: crate::automation::Bundle = serde_json::from_value(body)
        .map_err(|e| err(StatusCode::BAD_REQUEST, format!("that is not a project bundle: {e}")))?;
    let project = crate::automation::import(bundle)
        .map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?;
    crate::notify_store_changed("automation");
    Ok(Json(serde_json::to_value(project).unwrap_or(Value::Null)))
}

/// Answers as soon as the run is under way; progress comes from `/status`.
async fn run_project(Path(id): Path<String>) -> ApiResult {
    #[cfg(feature = "automation")]
    {
        find_project(&id)?;
        crate::runner::start(&id)
            .await
            .map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?;
        return Ok(Json(json!({ "project_id": id, "running": true })));
    }
    #[cfg(not(feature = "automation"))]
    {
        let _ = id;
        Err(err(StatusCode::NOT_IMPLEMENTED, "automation is not compiled into this build"))
    }
}

/// Asks the run to stop. The browsers it started close on their own once the
/// step they are in finishes, so a run does not vanish the instant this answers.
async fn stop_project(Path(id): Path<String>) -> ApiResult {
    #[cfg(feature = "automation")]
    {
        crate::runner::stop(&id);
        return Ok(Json(json!({ "project_id": id, "stopping": true })));
    }
    #[cfg(not(feature = "automation"))]
    {
        let _ = id;
        Err(err(StatusCode::NOT_IMPLEMENTED, "automation is not compiled into this build"))
    }
}

/// The run's state, or null when the project is not running and has not run
/// since the launcher started.
async fn project_status(Path(id): Path<String>) -> ApiResult {
    #[cfg(feature = "automation")]
    {
        return Ok(Json(
            serde_json::to_value(crate::runner::status(&id)).unwrap_or(Value::Null),
        ));
    }
    #[cfg(not(feature = "automation"))]
    {
        let _ = id;
        Err(err(StatusCode::NOT_IMPLEMENTED, "automation is not compiled into this build"))
    }
}

/// Every run going right now — what the fleet window shows.
async fn list_runs() -> ApiResult {
    #[cfg(feature = "automation")]
    {
        return Ok(Json(serde_json::to_value(crate::runner::all()).unwrap_or(Value::Null)));
    }
    #[cfg(not(feature = "automation"))]
    Err(err(StatusCode::NOT_IMPLEMENTED, "automation is not compiled into this build"))
}

async fn list_modules() -> ApiResult {
    #[cfg(feature = "automation")]
    {
        let list = crate::wasm::list()
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        return Ok(Json(serde_json::to_value(list).unwrap_or(Value::Null)));
    }
    #[cfg(not(feature = "automation"))]
    Err(err(StatusCode::NOT_IMPLEMENTED, "automation is not compiled into this build"))
}

#[derive(Deserialize)]
struct InstallModuleReq {
    path: String,
}

/// `path` is a .wasm file on this machine; the API is local and a module is
/// native code the operator built themselves.
async fn install_module(Json(body): Json<InstallModuleReq>) -> ApiResult {
    #[cfg(feature = "automation")]
    {
        if body.path.trim().is_empty() {
            return Err(err(StatusCode::BAD_REQUEST, "`path` required"));
        }
        let info = crate::wasm::install(body.path.trim())
            .map_err(|e| err(StatusCode::BAD_REQUEST, format!("{e:#}")))?;
        return Ok(Json(serde_json::to_value(info).unwrap_or(Value::Null)));
    }
    #[cfg(not(feature = "automation"))]
    {
        let _ = body.path;
        Err(err(StatusCode::NOT_IMPLEMENTED, "automation is not compiled into this build"))
    }
}

async fn remove_module(Path(id): Path<String>) -> ApiResult {
    #[cfg(feature = "automation")]
    {
        crate::wasm::remove(&id)
            .map_err(|e| err(StatusCode::NOT_FOUND, e.to_string()))?;
        return Ok(Json(json!({ "deleted": true, "id": id })));
    }
    #[cfg(not(feature = "automation"))]
    {
        let _ = id;
        Err(err(StatusCode::NOT_IMPLEMENTED, "automation is not compiled into this build"))
    }
}

// ---- server ----

pub async fn serve(secret: String, port: u16) {
    set_secret(&secret);
    let app = router();

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => {
            eprintln!("[launcher] automation API listening on http://{addr}");
            if let Err(e) = axum::serve(listener, app).await {
                eprintln!("[launcher] API server error: {e}");
            }
        }
        Err(e) => eprintln!("[launcher] API bind {addr} failed: {e}"),
    }
}

/// Every route, assembled. Split out so a test can build it: axum only catches
/// two routes on one path at construction, and that is a panic at startup.
fn router() -> Router {
    let protected = Router::new()
        .route("/profiles", get(list_profiles).post(create_profile))
        .route("/profiles/temporary", post(create_temporary))
        .route("/profiles/:id", get(get_profile).patch(edit_profile).delete(delete_profile))
        .route("/profiles/:id/start", post(start_profile))
        .route("/profiles/:id/stop", post(stop_profile))
        .route("/profiles/:id/cookies", get(export_cookies).post(import_cookies))
        .route("/folders", get(list_folders))
        .route("/folders/:folder", patch(rename_folder_ep).delete(delete_folder_ep))
        .route("/folders/:folder/profiles", post(create_profile_in_folder))
        .route("/fingerprint/new", get(new_fingerprint))
        .route("/fingerprint/new/:platform", get(new_fingerprint_for))
        .route("/fingerprints", get(list_fingerprints))
        .route("/running", get(list_running))
        .route("/proxies", get(list_proxies).post(add_proxy))
        .route("/proxies/:id", delete(delete_proxy))
        .route("/extensions", get(list_extensions).post(add_extension))
        .route("/extensions/:id", delete(delete_extension))
        .route("/bookmarks", get(list_bookmarks).post(save_bookmark))
        .route("/bookmarks/:id", delete(delete_bookmark))
        .route("/trash", get(list_trash))
        .route("/trash/:id", delete(purge_trash))
        .route("/trash/:id/restore", post(restore_trash))
        .route("/automation/projects", get(list_projects).post(create_project))
        .route(
            "/automation/projects/:id",
            get(get_project).put(save_project).delete(delete_project),
        )
        .route("/automation/projects/:id/duplicate", post(duplicate_project))
        .route("/automation/projects/:id/export", get(export_project))
        .route("/automation/projects/:id/run", post(run_project))
        .route("/automation/projects/:id/stop", post(stop_project))
        .route("/automation/projects/:id/status", get(project_status))
        .route("/automation/import", post(import_project))
        .route("/automation/runs", get(list_runs))
        .route("/automation/modules", get(list_modules).post(install_module))
        .route("/automation/modules/:id", delete(remove_module))
        // Layers run last-added first: token (401), then key (license_required),
        // then whether HirS is let in (hirs_not_allowed).
        .route_layer(middleware::from_fn(hirs_scope))
        .route_layer(middleware::from_fn(hirs_gate))
        .route_layer(middleware::from_fn(license_gate))
        .route_layer(middleware::from_fn(auth));

    Router::new().route("/health", get(health)).merge(protected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    /// Two routes on one path panic when the router is built, so building it
    /// is the whole test.
    #[test]
    fn router_builds() {
        let _ = router();
    }

    #[tokio::test]
    async fn automation_needs_a_token() {
        for (method, path) in [
            ("GET", "/automation/projects"),
            ("GET", "/automation/runs"),
            ("POST", "/automation/projects/x/run"),
            ("GET", "/automation/modules"),
        ] {
            let req = Request::builder()
                .method(method)
                .uri(path)
                .body(Body::empty())
                .unwrap();
            let res = router().oneshot(req).await.unwrap();
            assert_eq!(
                res.status(),
                StatusCode::UNAUTHORIZED,
                "{method} {path} answered without a token"
            );
        }
    }

    /// With a token but no activated key, every route answers the one code a
    /// client can act on; with the key, the same request goes through. One
    /// test for both, since the verdict is process-wide.
    #[tokio::test]
    async fn a_token_alone_is_not_enough_without_a_key() {
        set_secret("gate-test-secret");
        let token = mint("gate-test-secret", 60).unwrap();
        let get = |path: &str| {
            Request::builder()
                .uri(path)
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap()
        };

        crate::license::set_verdict_for_test(false);
        let res = router().oneshot(get("/running")).await.unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        let body = axum::body::to_bytes(res.into_body(), 1 << 16).await.unwrap();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["error"], "license_required");
        assert!(v["message"].as_str().is_some_and(|m| m.contains("HLG-")), "a sentence the user can read");

        // No token still says 401, not 403: the key check comes after auth.
        let anon = Request::builder().uri("/running").body(Body::empty()).unwrap();
        assert_eq!(router().oneshot(anon).await.unwrap().status(), StatusCode::UNAUTHORIZED);
        // And /health stays open either way.
        let health = Request::builder().uri("/health").body(Body::empty()).unwrap();
        assert_eq!(router().oneshot(health).await.unwrap().status(), StatusCode::OK);

        crate::license::set_verdict_for_test(true);
        let res = router().oneshot(get("/running")).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    /// Only a request that says it is HirS is turned away by the switch; any
    /// other client with the same token goes through.
    #[tokio::test]
    async fn hirs_is_refused_while_the_switch_is_off() {
        let ok = |req: &Request<Body>| is_hirs(req);
        let with = |v: &str| Request::builder().uri("/running").header("X-Hir-Client", v).body(Body::empty()).unwrap();
        assert!(ok(&with("HirS")));
        assert!(ok(&with(" hirs ")));
        assert!(!ok(&with("mcp")));
        assert!(!ok(&Request::builder().uri("/running").body(Body::empty()).unwrap()));
    }

    /// A start the team refuses because another machine has the profile open is a 409 with a
    /// code and a Vietnamese sentence (in `error` too, for clients that only show `error`);
    /// any other launch failure is left alone.
    #[tokio::test]
    async fn a_profile_open_on_another_machine_is_a_409_in_words() {
        let res = in_use_response("p1", "this profile is in use by Admin — try again once they close it").unwrap();
        assert_eq!(res.status(), StatusCode::CONFLICT);
        let body = axum::body::to_bytes(res.into_body(), 1 << 16).await.unwrap();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["code"], "profile_in_use");
        assert_eq!(v["saving"], false);
        assert_eq!(v["error"], "Profile này đang được Admin mở. Hãy đợi người đó đóng profile rồi thử lại.");
        assert_eq!(v["error"], v["message"]);

        let saving = in_use_response("p1", "this profile is being saved by Admin — try again in a moment").unwrap();
        let body = axum::body::to_bytes(saving.into_body(), 1 << 16).await.unwrap();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["saving"], true);

        let mine = in_use_response("p1", "this profile is in use by you on another machine — try again once you close it").unwrap();
        let body = axum::body::to_bytes(mine.into_body(), 1 << 16).await.unwrap();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert!(v["error"].as_str().unwrap().starts_with("Chính bạn đang mở profile này"));

        assert!(in_use_response("p1", "read profiles/p1.json: not found").is_none());
    }

    /// `/health` is the one route that stays open, for probing before a token.
    #[tokio::test]
    async fn health_stays_open() {
        let req = Request::builder().uri("/health").body(Body::empty()).unwrap();
        let res = router().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }
}
