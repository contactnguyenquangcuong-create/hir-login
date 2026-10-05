use anyhow::{Context, Result};
use axum::{
    body::Body,
    extract::Path as AxumPath,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
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
        if crate::winfs::rename_replace(&tmp, path).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    crate::winfs::rename_replace(&tmp, path).context("atomic write failed")?;
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

// ---- change notifications ----
//
// Every accepted upload is appended here and wakes anyone waiting on
// `/events/wait`, so the other machines pull the moment a profile is closed
// instead of finding out on a timer.

struct EventLog {
    seq: u64,
    items: std::collections::VecDeque<(u64, String)>,
}

static EVENT_LOG: OnceLock<Mutex<EventLog>> = OnceLock::new();
fn event_log() -> &'static Mutex<EventLog> {
    EVENT_LOG.get_or_init(|| Mutex::new(EventLog { seq: 0, items: Default::default() }))
}

static EVENT_TX: OnceLock<tokio::sync::watch::Sender<u64>> = OnceLock::new();
fn event_tx() -> &'static tokio::sync::watch::Sender<u64> {
    EVENT_TX.get_or_init(|| tokio::sync::watch::channel(0u64).0)
}

fn publish_event(id: &str) {
    let seq = {
        let mut log = event_log().lock().unwrap_or_else(|e| e.into_inner());
        log.seq += 1;
        let seq = log.seq;
        log.items.push_back((seq, id.to_string()));
        while log.items.len() > 500 {
            log.items.pop_front();
        }
        seq
    };
    let _ = event_tx().send(seq);
}

fn events_after(after: u64) -> (u64, Vec<String>) {
    let log = event_log().lock().unwrap_or_else(|e| e.into_inner());
    let ids = log.items.iter().filter(|(n, _)| *n > after).map(|(_, id)| id.clone()).collect();
    (log.seq, ids)
}

/// Long-poll: answers as soon as an upload newer than `after` exists, or after
/// ~25 seconds with nothing new. Without `after` it just reports the current
/// position so a client can start listening from "now".
async fn wait_events(
    headers: HeaderMap,
    axum::extract::State(state): axum::extract::State<Arc<ServerState>>,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let Some(who) = authenticate(&headers, &state.token) else { return unauthorized() };
    let Some(after) = q.get("after").and_then(|v| v.parse::<u64>().ok()) else {
        let (seq, _) = events_after(u64::MAX);
        return Json(json!({"ok": true, "seq": seq, "ids": Vec::<String>::new()})).into_response();
    };
    let mut rx = event_tx().subscribe();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(25);
    loop {
        let (seq, raw) = events_after(after);
        let ids: Vec<String> = if who.is_privileged() {
            raw
        } else {
            let (acl, meta) = (load_acl(), meta_map());
            raw.into_iter()
                .filter(|id| id.starts_with("lib:") || profile_level(&who, &acl, &meta, id) > Level::None)
                .collect()
        };
        if !ids.is_empty() || seq < after {
            return Json(json!({"ok": true, "seq": seq, "ids": ids})).into_response();
        }
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() || tokio::time::timeout(left, rx.changed()).await.is_err() {
            return Json(json!({"ok": true, "seq": seq, "ids": Vec::<String>::new()})).into_response();
        }
    }
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

// ---- identity, permissions, audit ----

use crate::team_acl::{self, AclStore, Identity, Level};

fn bearer(headers: &HeaderMap) -> Option<String> {
    let h = headers.get("authorization")?.to_str().ok()?;
    let t = h.strip_prefix("Bearer ").or_else(|| h.strip_prefix("bearer "))?;
    Some(t.trim().to_string())
}

/// The admin token (the one the server was started with) or a member's own.
fn authenticate(headers: &HeaderMap, admin_token: &str) -> Option<Identity> {
    if check_auth(headers, admin_token) {
        return Some(Identity::admin());
    }
    let token = bearer(headers)?;
    load_acl().authenticate(&token)
}

fn unauthorized() -> Response {
    (StatusCode::UNAUTHORIZED, Json(json!({"ok":false,"error":"unauthorized"}))).into_response()
}

fn forbidden(reason: &str) -> Response {
    (StatusCode::FORBIDDEN, Json(json!({"ok":false,"error":"forbidden","reason":reason}))).into_response()
}

/// Something the caller may not see is reported exactly like something that
/// does not exist.
fn not_found() -> Response {
    (StatusCode::NOT_FOUND, Json(json!({"ok":false,"error":"not found"}))).into_response()
}

fn acl_lock() -> std::sync::MutexGuard<'static, ()> {
    static L: OnceLock<Mutex<()>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(())).lock().unwrap_or_else(|e| e.into_inner())
}

fn load_acl() -> AclStore {
    server_data_dir().map(|d| AclStore::load(&d)).unwrap_or_default()
}

fn save_acl(acl: &AclStore) -> Result<()> {
    acl.save(&server_data_dir()?)?;
    Ok(())
}

fn meta_map() -> serde_json::Map<String, Value> {
    load_json(&meta_path().unwrap_or_default(), json!({})).as_object().cloned().unwrap_or_default()
}

/// `who`'s level on a profile, from the folder recorded when it was uploaded.
/// A profile the server has no record of is reachable only by admins/managers.
fn profile_level(who: &Identity, acl: &AclStore, meta: &serde_json::Map<String, Value>, id: &str) -> Level {
    match meta.get(id) {
        Some(m) => acl.level(who, m.get("folder").and_then(|f| f.as_str()).unwrap_or("")),
        None => if who.is_privileged() { Level::Full } else { Level::None },
    }
}

fn audit(who: &Identity, action: &str, target: &str, ok: bool, note: &str) {
    use std::io::Write;
    let Ok(dir) = server_data_dir() else { return };
    let line = json!({
        "t": time_format(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()),
        "who": who.name, "role": who.role.as_str(), "action": action, "target": target, "ok": ok, "note": note,
    });
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("audit.log")) {
        let _ = writeln!(f, "{line}");
    }
}

/// profile.json and proxy.json out of an uploaded bundle.
fn read_bundle_meta(bytes: &[u8]) -> Option<(Value, Value)> {
    use std::io::Read;
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).ok()?;
    let mut read = |name: &str| -> Value {
        z.by_name(name)
            .ok()
            .and_then(|mut f| {
                let mut s = String::new();
                f.read_to_string(&mut s).ok()?;
                serde_json::from_str(&s).ok()
            })
            .unwrap_or(Value::Null)
    };
    let profile = read("profile.json");
    if profile.is_null() {
        return None;
    }
    Some((profile, read("proxy.json")))
}

// ---- handlers ----

async fn health() -> impl IntoResponse {
    Json(json!({"ok": true}))
}

async fn list_profiles(
    headers: HeaderMap,
    axum::extract::State(state): axum::extract::State<Arc<ServerState>>,
) -> Response {
    let Some(who) = authenticate(&headers, &state.token) else { return unauthorized() };
    let locks: Value = load_json(&locks_path().unwrap_or_default(), json!({}));
    let meta: Value = load_json(&meta_path().unwrap_or_default(), json!({}));
    let locks_map = locks.as_object().cloned().unwrap_or_default();
    let meta_map = meta.as_object().cloned().unwrap_or_default();
    let mut ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for k in locks_map.keys() { ids.insert(k.clone()); }
    for k in meta_map.keys() { ids.insert(k.clone()); }
    let acl = load_acl();
    let profiles: Vec<Value> = ids.into_iter().filter(|id| profile_level(&who, &acl, &meta_map, id) > Level::None).map(|id| {
        let lock = locks_map.get(&id).cloned().unwrap_or(Value::Null);
        let held = !lock.is_null() && !is_expired(&lock);
        let m = meta_map.get(&id);
        json!({
            "id": id,
            "updatedAt": m.and_then(|v| v.get("updatedAt")).cloned().unwrap_or(Value::Null),
            "updatedBy": m.and_then(|v| v.get("updatedBy")).cloned().unwrap_or(Value::Null),
            "sizeBytes": m.and_then(|v| v.get("sizeBytes")).cloned().unwrap_or(Value::Null),
            "deleted": m.and_then(|v| v.get("deleted")).and_then(|v| v.as_bool()).unwrap_or(false),
            "access": profile_level(&who, &acl, &meta_map, &id).as_str(),
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
    let Some(who) = authenticate(&headers, &state.token) else { return unauthorized() };
    if !safe_id(&id) {
        return (StatusCode::BAD_REQUEST, Json(json!({"ok":false,"error":"bad id"}))).into_response();
    }
    // A profile the server has never seen is locked before its first upload; whether the
    // person may add it is decided by the upload (a group manager may, a member may not).
    let brand_new = !meta_map().contains_key(&id) && who.role == team_acl::Role::Manager;
    if !brand_new && profile_level(&who, &load_acl(), &meta_map(), &id) < Level::Use {
        audit(&who, "lock", &id, false, "no access");
        return not_found();
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
    let Some(who) = authenticate(&headers, &state.token) else { return unauthorized() };
    if !safe_id(&id) {
        return (StatusCode::BAD_REQUEST, Json(json!({"ok":false,"error":"bad id"}))).into_response();
    }
    let brand_new = !meta_map().contains_key(&id) && who.role == team_acl::Role::Manager;
    if !brand_new && profile_level(&who, &load_acl(), &meta_map(), &id) < Level::Use {
        return not_found();
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
    let Some(who) = authenticate(&headers, &state.token) else { return unauthorized() };
    if !safe_id(&id) {
        return (StatusCode::BAD_REQUEST, Json(json!({"ok":false,"error":"bad id"}))).into_response();
    }
    if profile_level(&who, &load_acl(), &meta_map(), &id) < Level::Use {
        return not_found();
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
    let Some(who) = authenticate(&headers, &state.token) else { return unauthorized() };
    if !safe_id(&id) {
        return (StatusCode::BAD_REQUEST, Json(json!({"ok":false,"error":"bad id"}))).into_response();
    }
    if body.len() > MAX_BUNDLE_BYTES {
        return (StatusCode::PAYLOAD_TOO_LARGE, Json(json!({"ok":false,"error":"bundle too large"}))).into_response();
    }
    let holder = headers.get("x-sync-holder").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    // What this upload does to the profile decides what it needs.
    let Some((new_profile, new_proxy)) = read_bundle_meta(&body) else {
        return (StatusCode::BAD_REQUEST, Json(json!({"ok":false,"error":"not a profile bundle"}))).into_response();
    };
    let new_folder = team_acl::folder_of(&new_profile);
    let new_sig = team_acl::protected_signature(&new_profile, &new_proxy);
    let new_sig_np = team_acl::protected_signature_without_proxy(&new_profile);
    {
        let acl = load_acl();
        let old = meta_map().get(&id).cloned();
        let live = old.as_ref().filter(|m| !m.get("deleted").and_then(|d| d.as_bool()).unwrap_or(false));
        // Live profiles already in a folder: it is not free to claim.
        let taken = |folder: &str| meta_map().iter().any(|(k, m)| {
            k != &id && m.get("folder").and_then(|f| f.as_str()) == Some(folder)
                && !m.get("deleted").and_then(|d| d.as_bool()).unwrap_or(false)
        });
        // Where may this person put a profile? Admin: anywhere. Group manager: a folder
        // they manage, or a brand-new one (which then becomes theirs). Member: nowhere.
        let may_place = |folder: &str| -> bool {
            who.is_privileged()
                || (who.role == team_acl::Role::Manager && !folder.is_empty()
                    && (acl.level(&who, folder) == Level::Full
                        || (!acl.owners.contains_key(folder) && !acl.folders.contains_key(folder) && !acl.trashed.contains_key(folder) && !taken(folder))))
        };
        match live {
            None => {
                if !may_place(&new_folder) {
                    audit(&who, "add", &id, false, "not allowed to add");
                    return forbidden("add");
                }
            }
            Some(m) => {
                let old_folder = m.get("folder").and_then(|f| f.as_str()).unwrap_or("");
                let lvl = acl.level(&who, old_folder);
                if lvl < Level::Use {
                    return not_found();
                }
                if old_folder != new_folder && (lvl < Level::Full || !may_place(&new_folder)) {
                    audit(&who, "move", &id, false, "folder change");
                    return forbidden("move");
                }
                let changed = m.get("sig").and_then(|x| x.as_str()).map(|old_sig| old_sig != new_sig).unwrap_or(false);
                if changed && lvl < Level::Edit {
                    // The one thing "use" access may change: turn the proxy off. Compared
                    // against the copy the server holds when it predates `sig_np`.
                    let old_np = m.get("sig_np").and_then(|x| x.as_str()).map(String::from).or_else(|| {
                        let bytes = std::fs::read(bundles_dir().ok()?.join(format!("{id}.zip"))).ok()?;
                        let (old_profile, _) = read_bundle_meta(&bytes)?;
                        Some(team_acl::protected_signature_without_proxy(&old_profile))
                    });
                    let only_proxy_turned_off = new_proxy.is_null() && old_np.as_deref() == Some(new_sig_np.as_str());
                    if !only_proxy_turned_off {
                        audit(&who, "edit", &id, false, "config change without edit access");
                        return forbidden("edit");
                    }
                }
            }
        }
    }
    if !new_folder.is_empty() {
        let _g = acl_lock();
        let mut acl = load_acl();
        acl.claim_folder(&new_folder, &who);
        let _ = save_acl(&acl);
    }
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
    meta_map.insert(id.clone(), json!({"updatedAt": now_iso, "updatedBy": holder, "sizeBytes": body.len(), "folder": new_folder, "sig": new_sig, "sig_np": new_sig_np}));
    let _ = save_json_atomic(&meta_path, &meta);
    // refresh lock TTL
    if let Some(map) = locks.as_object_mut() {
        map.insert(id.clone(), json!({"holder": holder, "acquiredAt": existing.get("acquiredAt").cloned().unwrap_or(json!(now_ms())), "expiresAt": now_ms() + lock_ttl_ms()}));
        let _ = save_json_atomic(&locks_path, &locks);
    }
    publish_event(&id);
    Json(json!({"ok": true, "sizeBytes": body.len()})).into_response()
}

/// A profile deleted on some machine: drop its bundle and lock and leave a
/// tombstone (with a fresh `updatedAt`) so the other machines learn about it. A
/// later upload of the same profile (a restore from the trash) overwrites it.
async fn delete_profile(
    headers: HeaderMap,
    axum::extract::State(state): axum::extract::State<Arc<ServerState>>,
    AxumPath(id): AxumPath<String>,
    body: axum::body::Bytes,
) -> Response {
    let Some(who) = authenticate(&headers, &state.token) else { return unauthorized() };
    if !safe_id(&id) {
        return (StatusCode::BAD_REQUEST, Json(json!({"ok":false,"error":"bad id"}))).into_response();
    }
    let (folder, known) = {
        let meta = meta_map();
        (meta.get(&id).and_then(|m| m.get("folder")).and_then(|f| f.as_str()).unwrap_or("").to_string(), meta.contains_key(&id))
    };
    let lvl = profile_level(&who, &load_acl(), &meta_map(), &id);
    if lvl < Level::Use && known {
        return not_found();
    }
    if lvl < Level::Full {
        audit(&who, "delete", &id, false, "no delete access");
        return forbidden("delete");
    }
    audit(&who, "delete", &id, true, "");
    let by = serde_json::from_slice::<Value>(&body)
        .ok()
        .and_then(|v| v.get("holder").and_then(|h| h.as_str().map(|s| s.to_string())))
        .unwrap_or_default();
    if let Ok(dir) = bundles_dir() {
        let _ = std::fs::remove_file(dir.join(format!("{id}.zip")));
    }
    if let Ok(lp) = locks_path() {
        let mut locks = load_json(&lp, json!({}));
        if let Some(m) = locks.as_object_mut() {
            m.remove(&id);
        }
        let _ = save_json_atomic(&lp, &locks);
    }
    if let Ok(mp) = meta_path() {
        let mut meta = load_json(&mp, json!({}));
        if let Some(m) = meta.as_object_mut() {
            m.insert(id.clone(), json!({
                "deleted": true,
                "folder": folder,
                "updatedAt": time_format(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()),
                "updatedBy": by,
            }));
        }
        let _ = save_json_atomic(&mp, &meta);
    }
    publish_event(&id);
    Json(json!({"ok": true})).into_response()
}

// ---- shared library (extensions, custom fingerprints) ----

fn safe_kind(kind: &str) -> bool {
    kind == "fingerprints"
}

fn library_dir(kind: &str) -> Result<PathBuf> {
    let d = server_data_dir()?.join("library").join(kind);
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

async fn library_list(
    headers: HeaderMap,
    axum::extract::State(state): axum::extract::State<Arc<ServerState>>,
    AxumPath(kind): AxumPath<String>,
) -> Response {
    let Some(_who) = authenticate(&headers, &state.token) else { return unauthorized() };
    if !safe_kind(&kind) {
        return (StatusCode::BAD_REQUEST, Json(json!({"ok":false,"error":"bad kind"}))).into_response();
    }
    let mut items: Vec<Value> = Vec::new();
    if let Ok(dir) = library_dir(&kind) {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if let Some(id) = name.strip_suffix(".bin") {
                    let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                    items.push(json!({"id": id, "sizeBytes": size}));
                }
            }
        }
    }
    Json(json!({"ok": true, "items": items})).into_response()
}

async fn library_get(
    headers: HeaderMap,
    axum::extract::State(state): axum::extract::State<Arc<ServerState>>,
    AxumPath((kind, id)): AxumPath<(String, String)>,
) -> Response {
    let Some(_who) = authenticate(&headers, &state.token) else { return unauthorized() };
    if !safe_kind(&kind) || !safe_id(&id) {
        return (StatusCode::BAD_REQUEST, Json(json!({"ok":false,"error":"bad request"}))).into_response();
    }
    let path = match library_dir(&kind) {
        Ok(d) => d.join(format!("{id}.bin")),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok":false,"error":e.to_string()}))).into_response(),
    };
    match tokio::fs::read(&path).await {
        Ok(bytes) => (StatusCode::OK, Body::from(bytes)).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, Json(json!({"ok":false,"error":"not found"}))).into_response(),
    }
}

async fn library_put(
    headers: HeaderMap,
    axum::extract::State(state): axum::extract::State<Arc<ServerState>>,
    AxumPath((kind, id)): AxumPath<(String, String)>,
    body: axum::body::Bytes,
) -> Response {
    let Some(who) = authenticate(&headers, &state.token) else { return unauthorized() };
    if !safe_kind(&kind) || !safe_id(&id) {
        return (StatusCode::BAD_REQUEST, Json(json!({"ok":false,"error":"bad request"}))).into_response();
    }
    if !who.is_privileged() {
        return forbidden("add");
    }
    if body.len() > MAX_BUNDLE_BYTES {
        return (StatusCode::PAYLOAD_TOO_LARGE, Json(json!({"ok":false,"error":"too large"}))).into_response();
    }
    let path = match library_dir(&kind) {
        Ok(d) => d.join(format!("{id}.bin")),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok":false,"error":e.to_string()}))).into_response(),
    };
    if let Err(e) = tokio::fs::write(&path, &body).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok":false,"error":e.to_string()}))).into_response();
    }
    publish_event(&format!("lib:{kind}/{id}"));
    Json(json!({"ok": true})).into_response()
}

// ---- members & folder sharing ----

async fn me(
    headers: HeaderMap,
    axum::extract::State(state): axum::extract::State<Arc<ServerState>>,
) -> Response {
    let Some(who) = authenticate(&headers, &state.token) else { return unauthorized() };
    let acl = load_acl();
    let folders: Value = if who.is_privileged() {
        Value::Null
    } else {
        json!(acl.folders.iter().filter_map(|(f, m)| m.get(&who.id).map(|_| (f.clone(), acl.level(&who, f).as_str().to_string()))).collect::<std::collections::BTreeMap<_, _>>())
    };
    Json(json!({"ok": true, "id": who.id, "name": who.name, "role": who.role.as_str(), "privileged": who.is_privileged(), "isServerAdmin": who.is_server_admin(), "folders": folders})).into_response()
}

fn admin_only(headers: &HeaderMap, state: &ServerState) -> Result<Identity, Response> {
    match authenticate(headers, &state.token) {
        None => Err(unauthorized()),
        Some(w) if w.role == team_acl::Role::Admin => Ok(w),
        Some(_) => Err(forbidden("admin")),
    }
}

/// Admin or group manager: those who can share folders.
fn staff_only(headers: &HeaderMap, state: &ServerState) -> Result<Identity, Response> {
    match authenticate(headers, &state.token) {
        None => Err(unauthorized()),
        Some(w) if w.role != team_acl::Role::Member => Ok(w),
        Some(_) => Err(forbidden("manage")),
    }
}

fn err500(e: impl ToString) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok":false,"error":e.to_string()}))).into_response()
}

/// Everyone (admin sees all; a manager sees the members they can share with).
async fn admin_members(
    headers: HeaderMap,
    axum::extract::State(state): axum::extract::State<Arc<ServerState>>,
) -> Response {
    let who = match staff_only(&headers, &state) { Ok(w) => w, Err(r) => return r };
    let acl = load_acl();
    let members: Vec<Value> = acl.members.iter()
        .filter(|m| who.is_privileged() || m.role == "member" || m.id == who.id)
        .map(|m| json!({"id": m.id, "name": m.name, "role": m.role, "disabled": m.disabled, "createdAt": m.created_at}))
        .collect();
    Json(json!({"ok": true, "members": members})).into_response()
}

/// Add a member, or with `id` change one (name, role, disabled, `rotate`).
/// A new or rotated token is returned once and stored only as a hash.
async fn admin_member_put(
    headers: HeaderMap,
    axum::extract::State(state): axum::extract::State<Arc<ServerState>>,
    body: axum::body::Bytes,
) -> Response {
    let who = match admin_only(&headers, &state) { Ok(w) => w, Err(r) => return r };
    let v: Value = serde_json::from_slice(&body).unwrap_or(json!({}));
    let _g = acl_lock();
    let mut acl = load_acl();
    let role = match v.get("role").and_then(|r| r.as_str()) { Some("admin") => "admin", Some("manager") => "manager", _ => "member" };
    let mut token = Value::Null;
    let id = if let Some(id) = v.get("id").and_then(|x| x.as_str()) {
        let Some(m) = acl.members.iter_mut().find(|m| m.id == id) else { return not_found() };
        let was_admin = m.role == "admin";
        // Two admins can't touch each other's rank, and neither may reach for
        // "disable" as a side door to the same thing — only the server token
        // promotes, demotes, disables, or deletes an admin.
        let touches_admin_rank = (v.get("role").is_some() && (role == "admin" || was_admin))
            || (was_admin && v.get("disabled").and_then(|x| x.as_bool()).unwrap_or(false));
        if touches_admin_rank && !who.is_server_admin() {
            audit(&who, "member", &id, false, "only the server token ranks admins");
            return forbidden("admin-rank");
        }
        // Rotating your own token kills the very session making this request:
        // the new token is only ever handed back as an invite code to copy
        // elsewhere, never applied to this device, so a self-rotate leaves the
        // admin logged in with a token that no longer works. This was already
        // explicitly ruled out as a feature (a superior issues the code, not
        // yourself) — this closes the same door reached through the regular
        // "Cấp lại mã" button instead of a dedicated self-service endpoint.
        if v.get("rotate").and_then(|x| x.as_bool()).unwrap_or(false) && id == who.id {
            audit(&who, "member", &id, false, "cannot rotate your own token");
            return forbidden("self-rotate");
        }
        if let Some(n) = v.get("name").and_then(|x| x.as_str()) { if !n.trim().is_empty() { m.name = n.trim().to_string(); } }
        if v.get("role").is_some() { m.role = role.to_string(); }
        if let Some(d) = v.get("disabled").and_then(|x| x.as_bool()) { m.disabled = d; }
        if v.get("rotate").and_then(|x| x.as_bool()).unwrap_or(false) {
            let t = team_acl::new_token();
            m.token_hash = team_acl::hash_token(&t);
            token = json!(t);
        }
        id.to_string()
    } else {
        let name = v.get("name").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        if name.is_empty() {
            return (StatusCode::BAD_REQUEST, Json(json!({"ok":false,"error":"name required"}))).into_response();
        }
        // A retried "Tạo mã" after some later step failed (e.g. Tailscale
        // rejecting the key) would otherwise silently create a second member
        // with the same name each time, since this call on its own always
        // succeeded — only visible after reopening the app.
        if acl.members.iter().any(|m| m.name.eq_ignore_ascii_case(&name)) {
            return (StatusCode::BAD_REQUEST, Json(json!({"ok":false,"error":"a member with this name already exists"}))).into_response();
        }
        // Making a brand-new admin is the same "who gets to rank an admin" question.
        if role == "admin" && !who.is_server_admin() {
            audit(&who, "member", &name, false, "only the server token ranks admins");
            return forbidden("admin-rank");
        }
        let t = team_acl::new_token();
        let id = uuid::Uuid::new_v4().simple().to_string();
        acl.members.push(team_acl::Member {
            id: id.clone(), name, role: role.into(), token_hash: team_acl::hash_token(&t), disabled: false,
            created_at: time_format(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()),
        });
        token = json!(t);
        id
    };
    // A person made a plain member can no longer manage anything.
    if let Some(m) = acl.members.iter().find(|m| m.id == id).cloned() {
        if m.role == "member" {
            for g in acl.folders.values_mut() {
                if g.get(&id).map(|l| l == "manage").unwrap_or(false) { g.insert(id.clone(), "use".into()); }
            }
        }
    }
    if let Err(e) = save_acl(&acl) { return err500(e); }
    audit(&who, "member", &id, true, "");
    Json(json!({"ok": true, "id": id, "token": token})).into_response()
}

async fn admin_member_delete(
    headers: HeaderMap,
    axum::extract::State(state): axum::extract::State<Arc<ServerState>>,
    AxumPath(id): AxumPath<String>,
) -> Response {
    let who = match admin_only(&headers, &state) { Ok(w) => w, Err(r) => return r };
    let _g = acl_lock();
    let mut acl = load_acl();
    if acl.members.iter().any(|m| m.id == id && m.role == "admin") && !who.is_server_admin() {
        audit(&who, "member-remove", &id, false, "only the server token ranks admins");
        return forbidden("admin-rank");
    }
    acl.remove_member(&id);
    if let Err(e) = save_acl(&acl) { return err500(e); }
    audit(&who, "member-remove", &id, true, "");
    Json(json!({"ok": true})).into_response()
}

/// The folders `who` may share: all of them for the admin, the ones they manage for a group manager.
/// Each carries who currently has access and at what level.
async fn admin_folders(
    headers: HeaderMap,
    axum::extract::State(state): axum::extract::State<Arc<ServerState>>,
) -> Response {
    let who = match staff_only(&headers, &state) { Ok(w) => w, Err(r) => return r };
    let acl = load_acl();
    let mut names: std::collections::BTreeSet<String> = acl.folders.keys().cloned().collect();
    names.extend(acl.owners.keys().cloned());
    let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
    for m in meta_map().values() {
        if m.get("deleted").and_then(|d| d.as_bool()).unwrap_or(false) { continue; }
        if let Some(f) = m.get("folder").and_then(|f| f.as_str()).filter(|f| !f.is_empty()) {
            names.insert(f.to_string());
            *counts.entry(f.to_string()).or_default() += 1;
        }
    }
    let folders: Vec<Value> = names.into_iter().filter(|f| acl.level(&who, f) == Level::Full).map(|f| json!({
        "name": f, "profiles": counts.get(&f).copied().unwrap_or(0), "access": acl.folders.get(&f).cloned().unwrap_or_default(),
        "canDelete": acl.may_delete_folder(&who, &f),
        // Who made it: "admin", or the manager's name.
        "createdBy": match acl.owners.get(&f).map(String::as_str) {
            None | Some("admin") => json!({"role": "admin", "name": ""}),
            Some(id) => json!({"role": "manager", "name": acl.members.iter().find(|m| m.id == id).map(|m| m.name.clone()).unwrap_or_default()}),
        },
    })).collect();
    Json(json!({"ok": true, "folders": folders})).into_response()
}

/// Make an empty folder (`{ "name": "..." }`); a group manager who does so manages it.
async fn admin_folder_create(
    headers: HeaderMap,
    axum::extract::State(state): axum::extract::State<Arc<ServerState>>,
    body: axum::body::Bytes,
) -> Response {
    let who = match staff_only(&headers, &state) { Ok(w) => w, Err(r) => return r };
    let v: Value = serde_json::from_slice(&body).unwrap_or(json!({}));
    let name = v.get("name").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
    if name.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({"ok":false,"error":"name required"}))).into_response();
    }
    let _g = acl_lock();
    let mut acl = load_acl();
    let in_use = acl.owners.contains_key(&name) || acl.folders.contains_key(&name) || acl.trashed.contains_key(&name)
        || meta_map().values().any(|m| m.get("folder").and_then(|f| f.as_str()) == Some(name.as_str()));
    if in_use {
        return (StatusCode::CONFLICT, Json(json!({"ok":false,"error":"folder exists"}))).into_response();
    }
    acl.claim_folder(&name, &who);
    if let Err(e) = save_acl(&acl) { return err500(e); }
    audit(&who, "folder-create", &name, true, "");
    Json(json!({"ok": true})).into_response()
}

/// `{ "memberId": "...", "level": "none" | "use" | "manage" }`. The admin shares any folder
/// with anyone (only managers can manage); a group manager shares the folders they
/// manage with members, for use.
async fn admin_folder_access(
    headers: HeaderMap,
    axum::extract::State(state): axum::extract::State<Arc<ServerState>>,
    AxumPath(folder): AxumPath<String>,
    body: axum::body::Bytes,
) -> Response {
    let who = match staff_only(&headers, &state) { Ok(w) => w, Err(r) => return r };
    let v: Value = serde_json::from_slice(&body).unwrap_or(json!({}));
    let member = v.get("memberId").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let mut level = Level::parse(v.get("level").and_then(|x| x.as_str()).unwrap_or("none"));
    let _g = acl_lock();
    let mut acl = load_acl();
    if acl.level(&who, &folder) != Level::Full {
        return not_found();
    }
    let Some(target) = acl.members.iter().find(|m| m.id == member).cloned() else { return not_found() };
    if target.role != "manager" && level == Level::Full { level = Level::Use; }
    if !who.is_privileged() && (target.role != "member" || level == Level::Full) {
        audit(&who, "share", &format!("{folder}:{}", target.name), false, "manager may only share for use with members");
        return forbidden("manage");
    }
    acl.set_access(&folder, &member, level);
    if let Err(e) = save_acl(&acl) { return err500(e); }
    audit(&who, "share", &format!("{folder} -> {}", target.name), true, level.as_str());
    // Whoever gained or lost the folder should see it at once.
    for (id, m) in meta_map() {
        if m.get("folder").and_then(|f| f.as_str()) == Some(folder.as_str()) { publish_event(&id); }
    }
    Json(json!({"ok": true})).into_response()
}

/// Delete an empty folder: it goes to the folder trash for 30 days, with its sharing.
/// Only the admin or the manager who made it; one handed down from above cannot be deleted.
async fn admin_folder_delete(
    headers: HeaderMap,
    axum::extract::State(state): axum::extract::State<Arc<ServerState>>,
    AxumPath(folder): AxumPath<String>,
) -> Response {
    let who = match staff_only(&headers, &state) { Ok(w) => w, Err(r) => return r };
    let _g = acl_lock();
    let mut acl = load_acl();
    if acl.level(&who, &folder) != Level::Full { return not_found(); }
    if !acl.may_delete_folder(&who, &folder) {
        audit(&who, "folder-delete", &folder, false, "created by someone above");
        return forbidden("folder-owner");
    }
    let has_profiles = meta_map().values().any(|m| m.get("folder").and_then(|f| f.as_str()) == Some(folder.as_str())
        && !m.get("deleted").and_then(|d| d.as_bool()).unwrap_or(false));
    if has_profiles {
        return (StatusCode::CONFLICT, Json(json!({"ok":false,"error":"folder not empty"}))).into_response();
    }
    acl.purge_expired_folders(now_ms());
    acl.trash_folder(&folder, &who, now_ms());
    if let Err(e) = save_acl(&acl) { return err500(e); }
    audit(&who, "folder-delete", &folder, true, "to trash");
    Json(json!({"ok": true})).into_response()
}

/// Deleted folders still within their 30 days (the admin sees all, a manager the ones they made).
async fn admin_folder_trash(
    headers: HeaderMap,
    axum::extract::State(state): axum::extract::State<Arc<ServerState>>,
) -> Response {
    let who = match staff_only(&headers, &state) { Ok(w) => w, Err(r) => return r };
    let _g = acl_lock();
    let mut acl = load_acl();
    let before = acl.trashed.len();
    acl.purge_expired_folders(now_ms());
    if acl.trashed.len() != before { let _ = save_acl(&acl); }
    let keep_ms = team_acl::FOLDER_TRASH_DAYS * 24 * 3600 * 1000;
    let items: Vec<Value> = acl.trashed.iter()
        .filter(|(_, t)| who.is_privileged() || (t.owner == who.id && !t.by_admin))
        .map(|(name, t)| json!({
            "name": name, "deletedBy": t.deleted_by,
            "daysLeft": (keep_ms.saturating_sub(now_ms().saturating_sub(t.deleted_at_ms)) + 86_399_999) / 86_400_000,
        })).collect();
    Json(json!({"ok": true, "items": items})).into_response()
}

async fn admin_folder_restore(
    headers: HeaderMap,
    axum::extract::State(state): axum::extract::State<Arc<ServerState>>,
    AxumPath(folder): AxumPath<String>,
) -> Response {
    let who = match staff_only(&headers, &state) { Ok(w) => w, Err(r) => return r };
    let _g = acl_lock();
    let mut acl = load_acl();
    let owner_ok = acl.trashed.get(&folder).map(|t| who.is_privileged() || (t.owner == who.id && !t.by_admin)).unwrap_or(false);
    if !owner_ok { return not_found(); }
    if !acl.restore_folder(&folder) {
        return (StatusCode::CONFLICT, Json(json!({"ok":false,"error":"name taken"}))).into_response();
    }
    if let Err(e) = save_acl(&acl) { return err500(e); }
    audit(&who, "folder-restore", &folder, true, "");
    Json(json!({"ok": true})).into_response()
}

/// Delete a trashed folder for good.
async fn admin_folder_purge(
    headers: HeaderMap,
    axum::extract::State(state): axum::extract::State<Arc<ServerState>>,
    AxumPath(folder): AxumPath<String>,
) -> Response {
    let who = match staff_only(&headers, &state) { Ok(w) => w, Err(r) => return r };
    let _g = acl_lock();
    let mut acl = load_acl();
    let owner_ok = acl.trashed.get(&folder).map(|t| who.is_privileged() || (t.owner == who.id && !t.by_admin)).unwrap_or(false);
    if !owner_ok { return not_found(); }
    acl.trashed.remove(&folder);
    if let Err(e) = save_acl(&acl) { return err500(e); }
    audit(&who, "folder-purge", &folder, true, "");
    Json(json!({"ok": true})).into_response()
}

async fn admin_audit(
    headers: HeaderMap,
    axum::extract::State(state): axum::extract::State<Arc<ServerState>>,
) -> Response {
    if let Err(r) = admin_only(&headers, &state) { return r; }
    let text = server_data_dir().ok().and_then(|d| std::fs::read_to_string(d.join("audit.log")).ok()).unwrap_or_default();
    let lines: Vec<Value> = text.lines().rev().take(200).filter_map(|l| serde_json::from_str(l).ok()).collect();
    Json(json!({"ok": true, "entries": lines})).into_response()
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

/// Refuses an unauthenticated request before its body is read (see `big_upload`).
async fn require_token(
    axum::extract::State(state): axum::extract::State<Arc<ServerState>>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    if authenticate(req.headers(), &state.token).is_none() {
        return unauthorized();
    }
    next.run(req).await
}

/// A PUT route that may carry a whole profile. axum cuts every request body at
/// 2 MB by default, which is far below what a logged-in profile holds (local
/// storage, IndexedDB, cookies): the server dropped the connection on every
/// such upload, so a profile that had been logged into could never be saved and
/// the next open restored the older, logged-out copy. The raised limit is only
/// for these upload routes, and a caller must show a valid token first, so an
/// anonymous peer cannot make the server buffer gigabytes.
fn big_upload<H, T>(handler: H, state: &Arc<ServerState>) -> axum::routing::MethodRouter<Arc<ServerState>>
where
    H: axum::handler::Handler<T, Arc<ServerState>>,
    T: 'static,
{
    axum::routing::put(handler)
        .layer(axum::extract::DefaultBodyLimit::max(MAX_BUNDLE_BYTES))
        .layer(axum::middleware::from_fn_with_state(state.clone(), require_token))
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
        .route("/profiles/:id/bundle", get(get_bundle).merge(big_upload(put_bundle, &state)))
        .route("/events/wait", get(wait_events))
        .route("/profiles/:id/delete", post(delete_profile))
        .route("/me", get(me))
        .route("/admin/members", get(admin_members).put(admin_member_put))
        .route("/admin/members/:id/delete", post(admin_member_delete))
        .route("/admin/folders", get(admin_folders).put(admin_folder_create))
        .route("/admin/folders/trash", get(admin_folder_trash))
        .route("/admin/folders/:folder/delete", post(admin_folder_delete))
        .route("/admin/folders/:folder/restore", post(admin_folder_restore))
        .route("/admin/folders/:folder/purge", post(admin_folder_purge))
        .route("/admin/folders/:folder/access", axum::routing::put(admin_folder_access))
        .route("/admin/audit", get(admin_audit))
        .route("/library/:kind", get(library_list))
        .route("/library/:kind/:id", get(library_get).merge(big_upload(library_put, &state)))
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

    Ok(actual_port)
}

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
        use std::os::windows::process::CommandExt;
        if let Ok(out) = std::process::Command::new("tailscale")
            .args(["ip", "-4"])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW — no console flash
            .output()
        {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if s.starts_with("100.") { return Some(s); }
        }
    }
    None
}

#[cfg(test)]
mod permission_tests {
    use super::*;
    use std::io::Write;

    fn bundle(folder: &str, ua: &str) -> Vec<u8> {
        let profile = json!({"_meta": {"folder": folder, "name": "p"}, "navigator": {"user_agent": ua}});
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let o = zip::write::SimpleFileOptions::default();
            z.start_file("profile.json", o).unwrap();
            z.write_all(profile.to_string().as_bytes()).unwrap();
            z.start_file("proxy.json", o).unwrap();
            z.write_all(b"null").unwrap();
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    fn bundle_with_proxy(folder: &str, ua: &str, proxy: Value) -> Vec<u8> {
        let profile = json!({"_meta": {"folder": folder, "name": "p"}, "navigator": {"user_agent": ua}});
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let o = zip::write::SimpleFileOptions::default();
            z.start_file("profile.json", o).unwrap();
            z.write_all(profile.to_string().as_bytes()).unwrap();
            z.start_file("proxy.json", o).unwrap();
            z.write_all(proxy.to_string().as_bytes()).unwrap();
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    async fn status(c: &reqwest::Client, tok: &str, method: &str, url: String, body: Option<Vec<u8>>, holder: &str) -> u16 {
        let mut r = match method { "GET" => c.get(url), "PUT" => c.put(url), _ => c.post(url) }.bearer_auth(tok).header("x-sync-holder", holder);
        if let Some(b) = body { r = r.body(b); } else if method == "POST" { r = r.json(&json!({"holder": holder})); }
        r.send().await.unwrap().status().as_u16()
    }

    async fn put_json(c: &reqwest::Client, tok: &str, url: String, body: Value) -> (u16, Value) {
        let r = c.put(url).bearer_auth(tok).json(&body).send().await.unwrap();
        (r.status().as_u16(), r.json().await.unwrap_or(Value::Null))
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn folder_permissions_are_enforced_by_the_server() {
        let _g = crate::cloud_sync::TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = std::env::temp_dir().join(format!("hir-acl-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        crate::store::set_data_root(Some(tmp.clone()));
        let admin = "admin-token-123456";
        let port = start(0, admin.to_string()).await.expect("start");
        let base = format!("http://127.0.0.1:{port}");
        let c = reqwest::Client::new();

        // People: a group manager (g), a member (u), another manager (h).
        let mut tok = std::collections::HashMap::new();
        let mut ids = std::collections::HashMap::new();
        for (n, role) in [("g", "manager"), ("h", "manager"), ("u", "member")] {
            let (_, r) = put_json(&c, admin, format!("{base}/admin/members"), json!({"name": n, "role": role})).await;
            tok.insert(n, r["token"].as_str().unwrap().to_string());
            ids.insert(n, r["id"].as_str().unwrap().to_string());
        }
        let up = |who: &str, id: &str, folder: &str, ua: &str| {
            let (c, base, t) = (c.clone(), base.clone(), who.to_string());
            let (id, b) = (id.to_string(), bundle(folder, ua));
            async move {
                let l = status(&c, &t, "POST", format!("{base}/profiles/{id}/lock"), None, "m").await;
                if l != 200 { return l; }
                let r = status(&c, &t, "PUT", format!("{base}/profiles/{id}/bundle"), Some(b), "m").await;
                status(&c, &t, "POST", format!("{base}/profiles/{id}/unlock"), None, "m").await;
                r
            }
        };

        // Admin creates folder A and gives g the management of it.
        assert_eq!(put_json(&c, admin, format!("{base}/admin/folders"), json!({"name": "A"})).await.0, 200);
        assert_eq!(put_json(&c, admin, format!("{base}/admin/folders/A/access"), json!({"memberId": ids["g"], "level": "manage"})).await.0, 200);
        assert_eq!(up(admin, "pa", "A", "ua1").await, 200);
        assert_eq!(up(admin, "pb", "B", "ua1").await, 200);

        // g manages A: adds, edits, and creates a folder of their own; B is invisible.
        assert_eq!(up(&tok["g"], "pa2", "A", "x").await, 200, "manager adds into a folder they manage");
        assert_eq!(up(&tok["g"], "pa2", "A", "y").await, 200, "and edits it");
        assert_eq!(up(&tok["g"], "pg", "G", "x").await, 200, "a new folder becomes theirs");
        assert_eq!(up(&tok["g"], "pb2", "B", "x").await, 403, "not into somebody else's folder");
        assert_eq!(status(&c, &tok["g"], "GET", format!("{base}/profiles/pb/bundle"), None, "").await, 404);
        assert_eq!(up(&tok["g"], "pa", "G", "ua1").await, 200, "moving between their own folders");
        assert_eq!(up(&tok["g"], "pa", "B", "ua1").await, 403, "but not into one they do not manage");
        // Another manager sees neither of g's folders.
        let list: Value = c.get(format!("{base}/profiles")).bearer_auth(&tok["h"]).send().await.unwrap().json().await.unwrap();
        assert!(list["profiles"].as_array().unwrap().is_empty());
        assert_eq!(up(&tok["h"], "ph", "G", "x").await, 403, "G already has an owner");
        // The admin sees everything.
        let list: Value = c.get(format!("{base}/profiles")).bearer_auth(admin).send().await.unwrap().json().await.unwrap();
        assert_eq!(list["profiles"].as_array().unwrap().len(), 4);

        // g shares A with u (use only); managers cannot hand out management.
        let (code, _) = put_json(&c, &tok["g"], format!("{base}/admin/folders/A/access"), json!({"memberId": ids["u"], "level": "use"})).await;
        assert_eq!(code, 200);
        assert_eq!(put_json(&c, &tok["g"], format!("{base}/admin/folders/A/access"), json!({"memberId": ids["h"], "level": "use"})).await.0, 403);
        assert_eq!(put_json(&c, &tok["g"], format!("{base}/admin/folders/B/access"), json!({"memberId": ids["u"], "level": "use"})).await.0, 404);
        let f: Value = c.get(format!("{base}/admin/folders")).bearer_auth(&tok["g"]).send().await.unwrap().json().await.unwrap();
        let names: Vec<&str> = f["folders"].as_array().unwrap().iter().map(|x| x["name"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["A", "G"]);

        // u: sees only A's profiles, may open them, cannot change/delete/add.
        let list: Value = c.get(format!("{base}/profiles")).bearer_auth(&tok["u"]).send().await.unwrap().json().await.unwrap();
        let seen: Vec<&str> = list["profiles"].as_array().unwrap().iter().map(|p| p["id"].as_str().unwrap()).collect();
        assert_eq!(seen, vec!["pa2"]);
        assert_eq!(status(&c, &tok["u"], "POST", format!("{base}/profiles/pa2/lock"), None, "u").await, 200);
        assert_eq!(status(&c, &tok["u"], "PUT", format!("{base}/profiles/pa2/bundle"), Some(bundle("A", "y")), "u").await, 200, "saving the session");
        assert_eq!(status(&c, &tok["u"], "PUT", format!("{base}/profiles/pa2/bundle"), Some(bundle("A", "hacked")), "u").await, 403);
        assert_eq!(status(&c, &tok["u"], "POST", format!("{base}/profiles/pa2/unlock"), None, "u").await, 200);
        assert_eq!(status(&c, &tok["u"], "POST", format!("{base}/profiles/pa2/delete"), None, "u").await, 403);
        assert_eq!(up(&tok["u"], "new", "A", "x").await, 404, "members cannot add");
        assert_eq!(status(&c, &tok["u"], "GET", format!("{base}/admin/folders"), None, "").await, 403);
        assert_eq!(status(&c, &tok["u"], "PUT", format!("{base}/library/fingerprints/x"), Some(b"x".to_vec()), "u").await, 403);
        // A member can never hold "manage" even if asked.
        put_json(&c, admin, format!("{base}/admin/folders/A/access"), json!({"memberId": ids["u"], "level": "manage"})).await;
        assert_eq!(status(&c, &tok["u"], "POST", format!("{base}/profiles/pa2/delete"), None, "u").await, 403);

        // g deletes inside their folder; only the admin manages people.
        assert_eq!(status(&c, &tok["g"], "POST", format!("{base}/profiles/pa2/delete"), None, "g").await, 200);
        assert_eq!(status(&c, &tok["g"], "PUT", format!("{base}/admin/members"), Some(b"{}".to_vec()), "").await, 403);

        // One group manager may run several groups when the admin hands them over: g now also manages B.
        assert_eq!(up(&tok["g"], "pb", "B", "z").await, 404, "not yet: B is invisible to g");
        assert_eq!(put_json(&c, admin, format!("{base}/admin/folders/B/access"), json!({"memberId": ids["g"], "level": "manage"})).await.0, 200);
        assert_eq!(up(&tok["g"], "pb", "B", "z").await, 200, "g edits a profile in B");
        assert_eq!(up(&tok["g"], "pb3", "B", "z").await, 200, "and adds one");
        assert_eq!(up(&tok["g"], "pa3", "A", "z").await, 200, "still manages A");
        let f: Value = c.get(format!("{base}/admin/folders")).bearer_auth(&tok["g"]).send().await.unwrap().json().await.unwrap();
        let names: Vec<&str> = f["folders"].as_array().unwrap().iter().map(|x| x["name"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["A", "B", "G"]);
        assert_eq!(status(&c, &tok["g"], "POST", format!("{base}/admin/folders/B/delete"), None, "").await, 403, "co-managing does not include deleting the admin's folder");

        // Folder trash: only whoever made a folder (or the admin) may delete it; it keeps 30 days.
        assert_eq!(status(&c, &tok["g"], "POST", format!("{base}/admin/folders/A/delete"), None, "").await, 403, "A came from the admin");
        assert_eq!(status(&c, &tok["g"], "POST", format!("{base}/admin/folders/G/delete"), None, "").await, 409, "G still holds a profile");
        assert_eq!(status(&c, &tok["g"], "POST", format!("{base}/profiles/pa/delete"), None, "g").await, 200);
        assert_eq!(status(&c, &tok["g"], "POST", format!("{base}/profiles/pg/delete"), None, "g").await, 200);
        assert_eq!(status(&c, &tok["g"], "POST", format!("{base}/admin/folders/G/delete"), None, "").await, 200);
        let t: Value = c.get(format!("{base}/admin/folders/trash")).bearer_auth(&tok["g"]).send().await.unwrap().json().await.unwrap();
        assert_eq!(t["items"][0]["name"], "G");
        assert_eq!(t["items"][0]["daysLeft"], 30);
        assert_eq!(status(&c, &tok["h"], "POST", format!("{base}/admin/folders/G/restore"), None, "").await, 404, "not theirs");
        assert_eq!(status(&c, &tok["g"], "POST", format!("{base}/admin/folders/G/restore"), None, "").await, 200);
        assert_eq!(status(&c, admin, "POST", format!("{base}/admin/folders/G/delete"), None, "").await, 200, "the admin may delete any");
        assert_eq!(status(&c, &tok["g"], "POST", format!("{base}/admin/folders/G/restore"), None, "").await, 404, "deleted from above: gone for them");
        assert_eq!(status(&c, admin, "POST", format!("{base}/admin/folders/G/restore"), None, "").await, 200, "the admin can still restore it");

        // Disabling locks someone out at once; the audit trail names who did what.
        put_json(&c, admin, format!("{base}/admin/members"), json!({"id": ids["u"], "disabled": true})).await;
        assert_eq!(status(&c, &tok["u"], "GET", format!("{base}/profiles"), None, "").await, 401);
        let a: Value = c.get(format!("{base}/admin/audit")).bearer_auth(admin).send().await.unwrap().json().await.unwrap();
        assert!(a["entries"].as_array().unwrap().iter().any(|e| e["action"] == "edit" && e["who"] == "u" && e["ok"] == false));

        let _ = stop();
        crate::store::set_data_root(None);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// A named "admin" member is a full equal of whoever holds the server's own
    /// token: same privileges everywhere, but with their own separate key so
    /// nobody has to share or remember the one token. Only an existing admin
    /// (never a manager) may create one.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_named_admin_member_is_a_full_equal_of_the_server_token() {
        let _g = crate::cloud_sync::TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = std::env::temp_dir().join(format!("hir-acl-admin-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        crate::store::set_data_root(Some(tmp.clone()));
        let admin = "admin-token-123456";
        let port = start(0, admin.to_string()).await.expect("start");
        let base = format!("http://127.0.0.1:{port}");
        let c = reqwest::Client::new();

        // Only the true admin may mint one — a manager trying is refused.
        let (_, mgr) = put_json(&c, admin, format!("{base}/admin/members"), json!({"name": "Manager", "role": "manager"})).await;
        let mgr_tok = mgr["token"].as_str().unwrap().to_string();
        assert_eq!(put_json(&c, &mgr_tok, format!("{base}/admin/members"), json!({"name": "Sneaky", "role": "admin"})).await.0, 403);

        let (code, r) = put_json(&c, admin, format!("{base}/admin/members"), json!({"name": "Second Admin", "role": "admin"})).await;
        assert_eq!(code, 200);
        let second_tok = r["token"].as_str().unwrap().to_string();
        let second_id = r["id"].as_str().unwrap().to_string();

        // Sees everything without ever being granted a single folder, same as the server token.
        let up = |who: &str, id: &str, folder: &str| {
            let (c, base, t) = (c.clone(), base.clone(), who.to_string());
            let (id, folder) = (id.to_string(), folder.to_string());
            async move {
                assert_eq!(status(&c, &t, "POST", format!("{base}/profiles/{id}/lock"), None, "m").await, 200);
                assert_eq!(status(&c, &t, "PUT", format!("{base}/profiles/{id}/bundle"), Some(bundle(&folder, "ua")), "m").await, 200);
                status(&c, &t, "POST", format!("{base}/profiles/{id}/unlock"), None, "m").await
            }
        };
        assert_eq!(up(&second_tok, "pz", "Untouched").await, 200);
        assert_eq!(status(&c, &second_tok, "POST", format!("{base}/profiles/pz/delete"), None, "").await, 200);

        // Manages members exactly like the server token — except ranking another
        // admin, which stays the server token's call alone. Not even minting a
        // brand-new admin: that is the same "who gets to be admin" question.
        assert_eq!(put_json(&c, &second_tok, format!("{base}/admin/members"), json!({"name": "Third", "role": "manager"})).await.0, 200, "ranking non-admins is fine");
        assert_eq!(put_json(&c, &second_tok, format!("{base}/admin/members"), json!({"name": "Fourth", "role": "admin"})).await.0, 403, "minting a peer admin is not");
        assert_eq!(put_json(&c, &second_tok, format!("{base}/admin/members"), json!({"id": second_id, "role": "manager"})).await.0, 403, "not even demoting themselves via this door");
        assert_eq!(put_json(&c, &second_tok, format!("{base}/admin/members"), json!({"id": second_id, "disabled": true})).await.0, 403, "or disabling as a side door to the same thing");
        assert_eq!(put_json(&c, &second_tok, format!("{base}/admin/members"), json!({"id": second_id, "name": "Renamed OK"})).await.0, 200, "non-rank edits (name) still work on themselves");
        assert_eq!(status(&c, &second_tok, "POST", format!("{base}/admin/members/{second_id}/delete"), None, "").await, 403, "and can't delete themselves out from under it either");

        // Only the server token ranks an admin: demoted back to member, loses admin reach at once.
        put_json(&c, admin, format!("{base}/admin/members"), json!({"id": second_id, "role": "member"})).await;
        assert_eq!(status(&c, &second_tok, "GET", format!("{base}/admin/members"), None, "").await, 403);

        let _ = stop();
        crate::store::set_data_root(None);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// A retried "Tạo mã" after a later step (minting a Tailscale key) failed
    /// used to silently create a second member with the same name each time,
    /// since this call alone always succeeded — only visible after reopening
    /// the app. The server now refuses it outright.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_member_cannot_share_a_name_with_an_existing_one() {
        let _g = crate::cloud_sync::TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = std::env::temp_dir().join(format!("hir-acl-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        crate::store::set_data_root(Some(tmp.clone()));
        let admin = "admin-token-123456";
        let port = start(0, admin.to_string()).await.expect("start");
        let base = format!("http://127.0.0.1:{port}");
        let c = reqwest::Client::new();

        assert_eq!(put_json(&c, admin, format!("{base}/admin/members"), json!({"name": "CuongPC", "role": "admin"})).await.0, 200);
        let (code, body) = put_json(&c, admin, format!("{base}/admin/members"), json!({"name": "CuongPC", "role": "member"})).await;
        assert_eq!(code, 400);
        assert_eq!(body["error"], "a member with this name already exists");
        // Case-insensitive: "cuongpc" is the same clash to a human reading the list.
        assert_eq!(put_json(&c, admin, format!("{base}/admin/members"), json!({"name": "cuongpc", "role": "member"})).await.0, 400);

        let _ = stop();
        crate::store::set_data_root(None);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Someone with only "use" access may turn a profile's proxy off (connect
    /// directly) and nothing else: not switch to another proxy, and not change
    /// the configuration in the same breath.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_use_only_member_may_turn_the_proxy_off_and_nothing_else() {
        let _g = crate::cloud_sync::TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = std::env::temp_dir().join(format!("hir-acl-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        crate::store::set_data_root(Some(tmp.clone()));
        let admin = "admin-token-123456";
        let port = start(0, admin.to_string()).await.expect("start");
        let base = format!("http://127.0.0.1:{port}");
        let c = reqwest::Client::new();

        let (_, r) = put_json(&c, admin, format!("{base}/admin/members"), json!({"name": "u", "role": "member"})).await;
        let (u_tok, u_id) = (r["token"].as_str().unwrap().to_string(), r["id"].as_str().unwrap().to_string());
        assert_eq!(put_json(&c, admin, format!("{base}/admin/folders"), json!({"name": "A"})).await.0, 200);
        assert_eq!(put_json(&c, admin, format!("{base}/admin/folders/A/access"), json!({"memberId": u_id, "level": "use"})).await.0, 200);

        let proxy = json!({"id": "px1", "name": "px", "kind": "socks5", "host": "1.2.3.4", "port": 1080, "username": "", "password": "", "country": "", "notes": ""});
        let other = json!({"id": "px2", "name": "px2", "kind": "socks5", "host": "5.6.7.8", "port": 1080, "username": "", "password": "", "country": "", "notes": ""});
        // One profile the server knows with sig_np, one as an older server stored it (without).
        for id in ["pp", "legacy"] {
            assert_eq!(status(&c, admin, "POST", format!("{base}/profiles/{id}/lock"), None, "m").await, 200);
            assert_eq!(status(&c, admin, "PUT", format!("{base}/profiles/{id}/bundle"), Some(bundle_with_proxy("A", "ua1", proxy.clone())), "m").await, 200);
            assert_eq!(status(&c, admin, "POST", format!("{base}/profiles/{id}/unlock"), None, "m").await, 200);
        }
        let meta_path = meta_path().unwrap();
        let mut meta = load_json(&meta_path, json!({}));
        meta["legacy"].as_object_mut().unwrap().remove("sig_np");
        save_json_atomic(&meta_path, &meta).unwrap();

        let put = |id: &'static str, body: Vec<u8>| {
            let (c, base, t) = (c.clone(), base.clone(), u_tok.clone());
            async move {
                assert_eq!(status(&c, &t, "POST", format!("{base}/profiles/{id}/lock"), None, "u").await, 200);
                let r = status(&c, &t, "PUT", format!("{base}/profiles/{id}/bundle"), Some(body), "u").await;
                status(&c, &t, "POST", format!("{base}/profiles/{id}/unlock"), None, "u").await;
                r
            }
        };
        assert_eq!(put("pp", bundle_with_proxy("A", "ua1", other.clone())).await, 403, "another proxy is an edit");
        assert_eq!(put("pp", bundle_with_proxy("A", "hacked", Value::Null)).await, 403, "config change hiding behind a proxy-off");
        assert_eq!(put("pp", bundle_with_proxy("A", "ua1", Value::Null)).await, 200, "turning the proxy off is allowed");
        assert_eq!(put("legacy", bundle_with_proxy("A", "hacked", Value::Null)).await, 403, "same check for a profile stored by an older server");
        assert_eq!(put("legacy", bundle_with_proxy("A", "ua1", Value::Null)).await, 200, "and the proxy-off works for it too");
        // Once off, putting a proxy back is again an edit.
        assert_eq!(put("pp", bundle_with_proxy("A", "ua1", proxy.clone())).await, 403, "switching one on is not");

        let _ = stop();
        crate::store::set_data_root(None);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The upload routes accept bodies far beyond axum's 2 MB default (a logged-in
    /// profile is megabytes), but only from a caller who has shown a valid token —
    /// an anonymous peer is turned away before the server reads any of it.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn big_uploads_need_a_token_before_the_body_is_read() {
        let _g = crate::cloud_sync::TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = std::env::temp_dir().join(format!("hir-acl-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        crate::store::set_data_root(Some(tmp.clone()));
        let admin = "admin-token-123456";
        let port = start(0, admin.to_string()).await.expect("start");
        let base = format!("http://127.0.0.1:{port}");
        let c = reqwest::Client::new();
        let big = vec![7u8; 5 * 1024 * 1024];

        for url in [format!("{base}/profiles/p1/bundle"), format!("{base}/library/fingerprints/f1")] {
            let anon = c.put(&url).body(big.clone()).send().await;
            // 401 (or the connection dropped once the server had said no): never accepted.
            assert!(anon.map(|r| r.status().as_u16() == 401).unwrap_or(true), "{url}: anonymous upload");
            let wrong = c.put(&url).bearer_auth("wrong-token-000").body(big.clone()).send().await;
            assert!(wrong.map(|r| r.status().as_u16() == 401).unwrap_or(true), "{url}: wrong token");
        }
        // With the token the size is no longer the problem (this body is not a
        // bundle, so the profile route answers 400 rather than 413).
        let r = c.put(format!("{base}/profiles/p1/bundle")).bearer_auth(admin).header("x-sync-holder", "m").body(big).send().await.unwrap();
        assert_eq!(r.status().as_u16(), 400);

        let _ = stop();
        crate::store::set_data_root(None);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// A named admin rotating their OWN token through the regular "Cấp lại mã"
    /// button (not the dedicated, already-rejected self-service endpoint)
    /// invalidates the very session making the request, since the new token is
    /// only ever handed back as an invite code — it never becomes this
    /// device's own token. That silently locked the admin out (reported as a
    /// confusing "wrong sync token" 401 on the next unrelated action) until
    /// they reconnected with a fresh code.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_member_cannot_rotate_their_own_token() {
        let _g = crate::cloud_sync::TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = std::env::temp_dir().join(format!("hir-acl-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        crate::store::set_data_root(Some(tmp.clone()));
        let admin = "admin-token-123456";
        let port = start(0, admin.to_string()).await.expect("start");
        let base = format!("http://127.0.0.1:{port}");
        let c = reqwest::Client::new();

        let (_, r) = put_json(&c, admin, format!("{base}/admin/members"), json!({"name": "A", "role": "admin"})).await;
        let a_tok = r["token"].as_str().unwrap().to_string();
        let a_id = r["id"].as_str().unwrap().to_string();

        let (code, body) = put_json(&c, &a_tok, format!("{base}/admin/members"), json!({"id": a_id, "rotate": true})).await;
        assert_eq!(code, 403);
        assert_eq!(body["reason"], "self-rotate");
        // Their old token still works — the rotate never took effect.
        assert_eq!(status(&c, &a_tok, "GET", format!("{base}/admin/members"), None, "").await, 200);
        // The server token (a superior) rotating it FOR them still works fine.
        assert_eq!(put_json(&c, admin, format!("{base}/admin/members"), json!({"id": a_id, "rotate": true})).await.0, 200);

        let _ = stop();
        crate::store::set_data_root(None);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
