// ShardX Launcher — Tauri backend.

mod profile_icon;
mod api;
mod bookmarks;
mod cloud_sync;
mod license;
mod cookies;
mod extensions;
mod fingerprints;
mod gpu_caps;
mod launch;
mod mcp_setup;
mod migrate;
mod process;
mod profile;
mod proxy;
mod psapi;
mod runtime;
mod settings;
mod store;
mod sync_bus;
mod autostart;
mod automation;
mod cdp;
mod requests;
mod db;
mod modguard;
mod runner;
mod wasm;
mod trash;
mod team_acl;
mod team_server;
mod team_invite;
mod tailscale;

use serde_json::Value;

/// App handle set in `run()` setup; lets the axum API reach a webview window.
static APP_HANDLE: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();

pub fn app_handle() -> Option<&'static tauri::AppHandle> {
    APP_HANDLE.get()
}

/// Launcher's own webview window (for monitor queries); None when headless.
pub fn main_window() -> Option<tauri::WebviewWindow> {
    use tauri::Manager;
    let app = APP_HANDLE.get()?;
    app.get_webview_window("main")
        .or_else(|| app.webview_windows().into_values().next())
}

/// Tell any open UI window that the on-disk store changed out-of-band — i.e. a
/// profile/proxy created or removed through the automation API or MCP, which
/// writes straight to disk without the React state ever knowing.  The view
/// listens for `store-changed` and reloads, so the new items appear without an
/// app restart.  `kind` ("profiles" | "proxies" | "automation") is informational; the UI
/// reloads both lists regardless.  No-op when headless (no window).
/// A warning the user has to see — shown as a toast. stderr is not a place a
/// user looks, and a profile silently running on the host's clock is worth an
/// interruption.
pub fn notify_warning(text: impl Into<String>) {
    use tauri::Emitter;
    let text = text.into();
    eprintln!("[launcher] WARNING: {text}");
    if let Some(w) = main_window() {
        let _ = w.emit("launcher-warning", text);
    }
}

pub fn notify_store_changed(kind: &str) {
    use tauri::Emitter;
    if let Some(w) = main_window() {
        let _ = w.emit("store-changed", kind);
    }
}

// ---- MCP server download ----

/// Download MCP server source into `<dir>/mcp`; user manages registration.
#[tauri::command]
async fn mcp_download(dir: String) -> Result<String, String> {
    mcp_setup::download_mcp(std::path::Path::new(&dir))
        .await
        .map(|p| p.display().to_string())
        .map_err(|e| e.to_string())
}

// ---- Profiles ----

#[tauri::command]
fn profile_list() -> Result<Vec<profile::ProfileMeta>, String> {
    profile::list_all().map_err(|e| e.to_string())
}

#[tauri::command]
fn profile_get(id: String) -> Result<Value, String> {
    let mut stored = profile::load_raw(&id).map_err(|e| e.to_string())?;
    // Backfill gpu_preset_id for legacy profiles by matching webgl.renderer.
    if stored.meta.gpu_preset_id.is_none() {
        if let Some(gid) = infer_gpu_preset_id(&stored.config) {
            stored.meta.gpu_preset_id = Some(gid);
            let _ = profile::save_raw(&mut stored);
        }
    }
    serde_json::to_value(stored).map_err(|e| e.to_string())
}

/// Recover library fingerprint id by matching webgl.renderer (+ screen if ambiguous).
fn infer_gpu_preset_id(config: &serde_json::Map<String, Value>) -> Option<String> {
    let renderer = config.get("webgl")?.get("renderer")?.as_str()?;
    let scr = config.get("screen");
    let sw = scr.and_then(|s| s.get("width")).and_then(|v| v.as_i64());
    let sh = scr.and_then(|s| s.get("height")).and_then(|v| v.as_i64());

    let entries = fingerprints::list_all().ok()?;
    let mut renderer_match: Option<String> = None;
    for e in &entries {
        let er = e
            .payload
            .get("webgl")
            .and_then(|w| w.get("renderer"))
            .and_then(|v| v.as_str());
        if er != Some(renderer) {
            continue;
        }
        let es = e.payload.get("screen");
        let ew = es.and_then(|s| s.get("width")).and_then(|v| v.as_i64());
        let eh = es.and_then(|s| s.get("height")).and_then(|v| v.as_i64());
        if sw.is_some() && ew == sw && eh == sh {
            return Some(e.id.clone());
        }
        renderer_match.get_or_insert_with(|| e.id.clone());
    }
    renderer_match
}

// ---- Realistic Sec-CH-UA-Platform-Version pools (spread per profile) ----

// macOS Sonoma 14.x, Sequoia 15.x, Tahoe 26.x.
const MACOS_PLATFORM_VERSIONS: &[&str] = &[
    "14.6.1", "14.7", "14.7.1", "14.7.2",
    "15.4", "15.4.1", "15.5", "15.6", "15.6.1", "15.7",
    "26.0", "26.0.1", "26.1",
];

// Win 10 21H1+ ("10.0.0"), Win 11 21H2..25H2 ("13"–"17"); weighted to 22H2/23H2/24H2.
const WINDOWS_PLATFORM_VERSIONS: &[&str] = &[
    "10.0.0",
    "13.0.0",
    "14.0.0", "14.0.0", "14.0.0",
    "15.0.0", "15.0.0", "15.0.0", "15.0.0",
    "16.0.0", "16.0.0", "16.0.0",
    "17.0.0",
];

// LTS kernels + current mainline.
const LINUX_PLATFORM_VERSIONS: &[&str] = &[
    "5.15.0", "6.1.0", "6.5.0",
    "6.6.0", "6.8.0", "6.10.0", "6.11.0", "6.12.0",
    "6.14.0", "6.15.0", "6.16.0",
];

/// Write a random platform_version into navigator + client_hints; unknown platforms left alone.
pub(crate) fn randomize_platform_version(payload: &mut serde_json::Map<String, Value>) {
    let platform = payload
        .get("navigator")
        .and_then(|n| n.get("platform"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let pool: &[&str] = match platform {
        "macOS"   => MACOS_PLATFORM_VERSIONS,
        "Windows" => WINDOWS_PLATFORM_VERSIONS,
        "Linux"   => LINUX_PLATFORM_VERSIONS,
        _         => return,
    };
    let pick_idx = (uuid::Uuid::new_v4().as_bytes()[0] as usize) % pool.len();
    let version = pool[pick_idx].to_string();

    if let Some(nav) = payload.get_mut("navigator").and_then(|v| v.as_object_mut()) {
        nav.insert("platform_version".into(), Value::String(version.clone()));
    }
    if let Some(ch) = payload.get_mut("client_hints").and_then(|v| v.as_object_mut()) {
        ch.insert("platform_version".into(), Value::String(version));
    }
}

/// Realistic (hardware_concurrency, deviceMemory) combos per Mac model id.
fn mac_hw_configs(model: &str) -> Option<&'static [(u32, u32)]> {
    Some(match model {
        "mac-m1-air13" | "mac-m1-mbp13" | "mac-m1-imac24" => &[(8, 8), (8, 16)],
        "mac-m1-pro-mbp14" | "mac-m1-pro-mbp16" => &[(8, 16), (10, 16), (10, 32)],
        "mac-m1-max-mbp14" | "mac-m1-max-mbp16" => &[(10, 32)],
        "mac-m2-air13" | "mac-m2-air15" | "mac-m2-mbp13" => &[(8, 8), (8, 16)],
        "mac-m2-pro-mbp14" | "mac-m2-pro-mbp16" => &[(10, 16), (12, 16), (12, 32)],
        "mac-m2-max-mbp14" | "mac-m2-max-mbp16" => &[(12, 32)],
        "mac-m3-air13" | "mac-m3-air15" | "mac-m3-mbp14" | "mac-m3-imac24" => {
            &[(8, 8), (8, 16)]
        }
        "mac-m3-pro-mbp14" | "mac-m3-pro-mbp16" => &[(11, 16), (12, 16), (12, 32)],
        "mac-m3-max-mbp14" | "mac-m3-max-mbp16" => &[(14, 32), (16, 32)],
        "mac-m4-air13" | "mac-m4-air15" | "mac-m4-mbp14" | "mac-m4-imac24" => {
            &[(10, 16), (10, 32)]
        }
        "mac-m4-pro-mbp14" | "mac-m4-pro-mbp16" => &[(12, 16), (14, 16), (14, 32)],
        "mac-m4-max-mbp14" | "mac-m4-max-mbp16" => &[(14, 32), (16, 32)],
        "mac-m5-mbp14" => &[(10, 16), (10, 32)],
        _ => return None,
    })
}

/// Host logical CPU count (counts SMT threads); fallback 8.
fn host_logical_cores() -> u32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(8)
}

/// Host physical RAM in GiB, best-effort per OS.
fn host_ram_gb() -> Option<u32> {
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()?;
        let bytes: u64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
        return Some((bytes / (1024 * 1024 * 1024)) as u32);
    }
    #[cfg(target_os = "linux")]
    {
        let s = std::fs::read_to_string("/proc/meminfo").ok()?;
        let kb: u64 = s
            .lines()
            .find(|l| l.starts_with("MemTotal:"))?
            .split_whitespace()
            .nth(1)?
            .parse()
            .ok()?;
        return Some((kb / (1024 * 1024)) as u32);
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // 0x08000000 = CREATE_NO_WINDOW — suppress the brief console flash a GUI
        // app gets when shelling out to a console-subsystem binary.
        let out = std::process::Command::new("wmic")
            .args(["ComputerSystem", "get", "TotalPhysicalMemory"])
            .creation_flags(0x08000000)
            .output()
            .ok()?;
        let txt = String::from_utf8_lossy(&out.stdout);
        let bytes: u64 = txt.lines().filter_map(|l| l.trim().parse::<u64>().ok()).next()?;
        return Some((bytes / (1024 * 1024 * 1024)) as u32);
    }
    #[allow(unreachable_code)]
    None
}

/// Physical RAM rounded to Chrome's {8,16,32} deviceMemory bucket; unknown → 16.
fn host_ram_bucket_gb() -> u32 {
    match host_ram_gb() {
        Some(gb) if gb >= 32 => 32,
        Some(gb) if gb >= 16 => 16,
        Some(_) => 8,
        None => 16,
    }
}

/// Pick (hardware_concurrency, device_memory): Mac → curated table, Win/Linux → host-bracketed.
pub(crate) fn randomize_hardware(payload: &mut serde_json::Map<String, Value>) {
    let model = payload
        .get("_meta")
        .and_then(|m| m.get("gpu_preset_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let platform = payload
        .get("navigator")
        .and_then(|n| n.get("platform"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let pick8 = || uuid::Uuid::new_v4().as_bytes()[0] as usize;

    let (cores, mem): (u32, u32) = if let Some(pool) = mac_hw_configs(model) {
        pool[pick8() % pool.len()]
    } else if platform == "Windows" || platform == "Linux" {
        let c = host_logical_cores();
        // Real x86 logical-core counts (SMT + Intel hybrid); bracket host within [C-4, C+2].
        const X86_CORES: [u32; 9] = [4, 6, 8, 12, 16, 20, 24, 28, 32];
        let lo = c.saturating_sub(4);
        let hi = c + 2;
        let cand: Vec<u32> = X86_CORES
            .into_iter()
            .filter(|&n| n >= lo && n <= hi)
            .collect();
        let cores = if cand.is_empty() {
            X86_CORES
                .into_iter()
                .min_by_key(|&n| (n as i64 - c as i64).abs())
                .unwrap()
        } else {
            cand[pick8() % cand.len()]
        };
        // deviceMemory: core-tied floor and host-RAM ceiling.
        let real = host_ram_bucket_gb();
        let floor = if cores >= 12 { 16 } else { 8 };
        let mem_cand: Vec<u32> = [8u32, 16, 32]
            .into_iter()
            .filter(|&m| m >= floor && m <= real)
            .collect();
        let mem = if mem_cand.is_empty() {
            real
        } else {
            mem_cand[pick8() % mem_cand.len()]
        };
        (cores, mem)
    } else {
        return;
    };

    if let Some(nav) = payload.get_mut("navigator").and_then(|v| v.as_object_mut()) {
        nav.insert("hardware_concurrency".into(), Value::from(cores));
        nav.insert("device_memory".into(), Value::from(mem));
    }
}

/// Clamp profile.screen to the real display when it's smaller than the FP claim.
/// A profile keeps the screen it declares while the real display can hold it.
pub fn clamp_screen_to_real_display(
    window: &tauri::WebviewWindow,
    payload: &mut serde_json::Map<String, Value>,
) {
    let Some(monitor) = window
        .primary_monitor()
        .ok()
        .flatten()
        .or_else(|| window.current_monitor().ok().flatten())
    else {
        eprintln!("[launcher] display: no monitor info — screen clamp skipped");
        return;
    };
    let scale = monitor.scale_factor();
    if scale <= 0.0 {
        eprintln!("[launcher] display: bad scale_factor {scale} — screen clamp skipped");
        return;
    }
    let phys = monitor.size();
    let real_w = (phys.width as f64 / scale).round() as i64;
    let real_h = (phys.height as f64 / scale).round() as i64;
    eprintln!(
        "[launcher] display: name={:?} physical={}x{} scale={} -> logical={}x{}",
        monitor.name(), phys.width, phys.height, scale, real_w, real_h
    );
    if real_w <= 0 || real_h <= 0 {
        return;
    }

    let Some(scr) = payload.get("screen").and_then(|v| v.as_object()) else {
        eprintln!("[launcher] display: profile has no `screen` block — clamp skipped");
        return;
    };
    let fp_w = scr.get("width").and_then(|v| v.as_i64()).unwrap_or(0);
    let fp_h = scr.get("height").and_then(|v| v.as_i64()).unwrap_or(0);
    eprintln!("[launcher] display: fingerprint screen={fp_w}x{fp_h}");
    if fp_w <= 0 || fp_h <= 0 {
        return;
    }
    // A screen the profile declares is kept whenever the real display can hold
    // it; the clamp exists for the other case, where a window simply cannot be
    // bigger than the monitor it opens on.
    //
    // This used to be the macOS rule only, and Windows/Linux overwrote the
    // declared screen with the host display on every start. That handed every
    // profile on one machine the SAME high-entropy pair — on a 5120x1440
    // monitor, all of them said 5120x1440 — which is the opposite of what a
    // per-profile screen is for, and it ignored what the profile's own API
    // caller had asked for.
    if real_w >= fp_w && real_h >= fp_h {
        eprintln!(
            "[launcher] display: real {real_w}x{real_h} >= fp {fp_w}x{fp_h} — keeping FP screen"
        );
        return;
    }

    // Preserve FP menubar/dock insets for avail_*.
    let fp_avail_w = scr.get("avail_width").and_then(|v| v.as_i64()).unwrap_or(fp_w);
    let fp_avail_h = scr.get("avail_height").and_then(|v| v.as_i64()).unwrap_or(fp_h);
    let chrome_w = (fp_w - fp_avail_w).max(0);
    let chrome_h = (fp_h - fp_avail_h).max(0);
    let avail_w = (real_w - chrome_w).max(1);
    let avail_h = (real_h - chrome_h).max(1);

    if let Some(scr_mut) = payload.get_mut("screen").and_then(|v| v.as_object_mut()) {
        scr_mut.insert("width".into(), Value::from(real_w));
        scr_mut.insert("height".into(), Value::from(real_h));
        scr_mut.insert("avail_width".into(), Value::from(avail_w));
        scr_mut.insert("avail_height".into(), Value::from(avail_h));
        scr_mut.insert("device_pixel_ratio".into(), Value::from(scale));
    }
    // Keep window inside the avail area; a profile with no window block gets one,
    // otherwise the browser falls back to Chromium's own small default size.
    let win_slot = payload
        .entry("window")
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if let Some(win) = win_slot.as_object_mut() {
        win.insert("outer_width".into(), Value::from(avail_w));
        win.insert("inner_width".into(), Value::from(avail_w));
        let outer_h = (avail_h - 1).max(1);
        win.insert("outer_height".into(), Value::from(outer_h));
        win.insert("inner_height".into(), Value::from((outer_h - 87).max(1)));
    }
    eprintln!(
        "[launcher] display: CLAMPED screen to real {real_w}x{real_h} \
         (avail {avail_w}x{avail_h}, dpr {scale}) — FP claimed {fp_w}x{fp_h}"
    );
}

#[tauri::command]
fn profile_save(
    window: tauri::WebviewWindow,
    payload: Value,
) -> Result<profile::ProfileMeta, String> {
    // UI saves enrich new profiles; the API persists verbatim.
    save_profile_core(Some(&window), payload, true)
}

/// Enrich a new profile in place: platform_version, hardware, screen clamp.
pub fn enrich_new_config(
    window: Option<&tauri::WebviewWindow>,
    obj: &mut serde_json::Map<String, Value>,
) {
    randomize_platform_version(obj);
    randomize_hardware(obj);
    if let Some(w) = window {
        clamp_screen_to_real_display(w, obj);
    }
}

/// Core of `profile_save` callable without Tauri context; `enrich=false` stores verbatim.
pub fn save_profile_core(
    window: Option<&tauri::WebviewWindow>,
    payload: Value,
    enrich: bool,
) -> Result<profile::ProfileMeta, String> {
    let mut payload = payload;

    let is_new = payload
        .get("_meta")
        .and_then(|m| m.get("id"))
        .and_then(|v| v.as_str())
        .map(|s| s.is_empty())
        .unwrap_or(true);
    if is_new && enrich {
        if let Some(obj) = payload.as_object_mut() {
            enrich_new_config(window, obj);
        }
    }

    // The editor rebuilds the whole profile from its own form, so a save on top
    // of a change made elsewhere (the API, a second window) would revert it.
    // A payload that carries the rev it was opened at is checked against disk;
    // one that carries none is an internal caller and passes.
    if !is_new {
        let meta = payload.get("_meta");
        let id = meta
            .and_then(|m| m.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if let Some(sent) = meta.and_then(|m| m.get("rev")).and_then(|v| v.as_u64()) {
            let on_disk = profile::current_rev(id);
            if sent != on_disk {
                return Err(format!(
                    "This profile changed after you opened it (rev {on_disk}, you have {sent}) \
                     — probably through the API or another window. Reopen it and apply your \
                     changes to the current version."
                ));
            }
        }
    }

    let mut stored: profile::StoredProfile =
        serde_json::from_value(payload).map_err(|e| e.to_string())?;
    profile::save_raw(&mut stored).map_err(|e| e.to_string())?;
    let name = stored
        .config
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("(unnamed)")
        .to_string();
    let notes = stored
        .config
        .get("notes")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok(profile::ProfileMeta {
        id: stored.meta.id,
        name,
        notes,
        proxy_id: stored.meta.proxy_id,
        last_launched_at: stored.meta.last_launched_at,
        created_at: stored.meta.created_at,
        pinned: stored.meta.pinned,
        folder: stored.meta.folder,
        total_runtime_ms: stored.meta.total_runtime_ms,
        color: stored.meta.color,
        extensions: stored.meta.extensions,
        mobile: profile::claims_mobile(&stored.config),
        android_media: false,
    })
}

/// Into the trash for a week; only the files carrying the account are kept.
#[tauri::command]
fn profile_delete(id: String) -> Result<(), String> {
    trash::move_to_trash(&id).map(|_| ()).map_err(|e| e.to_string())
}

// ---- Automation ----

/// Whether this build has the automation section compiled in. Always present,
/// so the UI can ask before it renders anything.
#[tauri::command]
fn automation_available() -> bool {
    cfg!(feature = "automation")
}

#[tauri::command]
fn automation_list() -> Result<Vec<automation::Project>, String> {
    automation::list().map_err(|e| e.to_string())
}

#[tauri::command]
fn automation_create(name: String) -> Result<automation::Project, String> {
    automation::create(&name).map_err(|e| e.to_string())
}

#[tauri::command]
fn automation_save(project: automation::Project) -> Result<automation::Project, String> {
    automation::save(project).map_err(|e| e.to_string())
}

#[tauri::command]
fn automation_delete(id: String) -> Result<(), String> {
    automation::delete(&id).map_err(|e| e.to_string())
}

#[tauri::command]
fn automation_duplicate(id: String) -> Result<automation::Project, String> {
    automation::duplicate(&id).map_err(|e| e.to_string())
}

/// Starts a profile WITH automation on, and attaches. The ordinary UI launch
/// deliberately leaves CDP off, so the studio needs its own door.
#[tauri::command]
async fn automation_launch(app: tauri::AppHandle, profile_id: String) -> Result<u32, String> {
    #[cfg(feature = "automation")]
    {
        if migrate::in_progress() {
            return Err("profiles are being moved — try again when that finishes".into());
        }
        // CDP cannot be turned on for a live process, so attaching to one opened
        // without it would give a focused window with no frames and no control.
        if is_profile_running(&profile_id)
            && process::Tracker::shared().cdp(&profile_id).is_none()
        {
            return Err(
                "This profile is already open without debugging. Close it, then open it here."
                    .into(),
            );
        }
        let b = bus().await?;
        // (enable_cdp, headless) — the studio needs a visible window with CDP on.
        let out = launch::launch_profile_synced(&profile_id, true, false, None, b.port, &b.token)
            .await
            .map_err(|e| e.to_string())?;
        automation_attach(app, profile_id).await?;
        return Ok(out.pid);
    }
    #[cfg(not(feature = "automation"))]
    {
        let _ = (app, profile_id);
        Err("automation is not compiled into this build".into())
    }
}

/// Attaches to a profile already running with CDP on; frames arrive as
/// `automation:frame`. Only bodies are gated — Tauri's command list takes no `#[cfg]`.
#[tauri::command]
async fn automation_attach(app: tauri::AppHandle, profile_id: String) -> Result<(), String> {
    #[cfg(feature = "automation")]
    {
        use tauri::Emitter;
        let info = process::Tracker::shared()
            .cdp(&profile_id)
            .ok_or_else(|| "that profile is not running with automation on".to_string())?;
        let handle = app.clone();
        let nav_handle = app.clone();
        let ev_handle = app.clone();
        let ev_profile = profile_id.clone();
        return cdp::attach_with(
            profile_id,
            info.web_socket_debugger_url,
            move |frame| {
                let _ = handle.emit("automation:frame", frame);
            },
            move |profile_id, url| {
                let _ = nav_handle.emit(
                    "automation:navigated",
                    serde_json::json!({ "profile_id": profile_id, "url": url }),
                );
            },
            // Surface the interceptor's paused-request events to the studio so
            // it can answer them with Traffic.resolve.
            move |method, params| {
                let topic = match method.as_str() {
                    "Traffic.requestPaused" => "automation:traffic-paused",
                    "Traffic.requestObserved" => "automation:traffic-observed",
                    _ => return,
                };
                let _ = ev_handle.emit(
                    topic,
                    serde_json::json!({ "profile_id": ev_profile, "params": params }),
                );
            },
        )
        .await
        .map_err(|e| e.to_string());
    }
    #[cfg(not(feature = "automation"))]
    {
        let _ = (app, profile_id);
        Err("automation is not compiled into this build".into())
    }
}

#[tauri::command]
fn automation_detach(profile_id: String) {
    #[cfg(feature = "automation")]
    cdp::detach(&profile_id);
    #[cfg(not(feature = "automation"))]
    let _ = profile_id;
}

#[tauri::command]
fn automation_attached(profile_id: String) -> bool {
    #[cfg(feature = "automation")]
    return cdp::is_attached(&profile_id);
    #[cfg(not(feature = "automation"))]
    {
        let _ = profile_id;
        false
    }
}

#[tauri::command]
async fn automation_screencast(
    profile_id: String,
    on: bool,
    width: u32,
    height: u32,
) -> Result<(), String> {
    #[cfg(feature = "automation")]
    {
        return if on {
            cdp::start_screencast(&profile_id, width, height).await
        } else {
            cdp::stop_screencast(&profile_id).await
        }
        .map_err(|e| e.to_string());
    }
    #[cfg(not(feature = "automation"))]
    {
        let _ = (profile_id, on, width, height);
        Err("automation is not compiled into this build".into())
    }
}

/// What this desktop lets the launcher do with windows: browsers still place
/// themselves over X11/XWayland, but under Wayland our own panels cannot.
#[tauri::command]
fn automation_display() -> serde_json::Value {
    #[cfg(target_os = "linux")]
    {
        let wayland = std::env::var("WAYLAND_DISPLAY").is_ok()
            || std::env::var("XDG_SESSION_TYPE")
                .map(|v| v.eq_ignore_ascii_case("wayland"))
                .unwrap_or(false);
        // The same condition launch.rs pins --ozone-platform=x11 on.
        let x_display = std::env::var_os("DISPLAY").is_some();
        let browser_placement = !wayland || x_display;
        let panels = !wayland;
        let note = if !browser_placement {
            "This is a Wayland session with no X display for the browser to fall back to, so it \
             runs as a Wayland window: it cannot place itself, arranging browsers does nothing, \
             and the launcher cannot keep the Fleet window above the others either. Install \
             XWayland, or log in with an Xorg session. Recording and running still work — the \
             live view comes over the debugging connection, not off the screen."
        } else if !panels {
            "This is a Wayland session. Browsers still arrange themselves, because they run \
             through XWayland, but the launcher cannot keep its own Fleet window above the \
             others or place it — a Wayland application is not allowed to. Log in with an Xorg \
             session if you need that."
        } else {
            ""
        };
        return serde_json::json!({
            "server": if wayland { "wayland" } else { "x11" },
            "limited": !browser_placement || !panels,
            "note": note,
            "browser_placement": browser_placement,
            "panels": panels,
        });
    }
    #[cfg(not(target_os = "linux"))]
    serde_json::json!({
        "server": "native",
        "limited": false,
        "note": "",
        "browser_placement": true,
        "panels": true,
    })
}

// ---- Modules ----

/// Every TLS/HTTP2 fingerprint the request steps can wear. Read off the
/// library, so it stays right when the library is updated.
#[tauri::command]
fn automation_tls_fingerprints() -> Vec<String> {
    #[cfg(feature = "automation")]
    return requests::fingerprints();
    #[cfg(not(feature = "automation"))]
    Vec::new()
}

#[tauri::command]
fn automation_modules() -> Result<Vec<serde_json::Value>, String> {
    #[cfg(feature = "automation")]
    return wasm::list()
        .map(|v| v.into_iter().filter_map(|m| serde_json::to_value(m).ok()).collect())
        .map_err(|e| e.to_string());
    #[cfg(not(feature = "automation"))]
    Ok(Vec::new())
}

#[tauri::command]
fn automation_module_install(path: String) -> Result<serde_json::Value, String> {
    #[cfg(feature = "automation")]
    return wasm::install(&path)
        .map(|m| serde_json::to_value(m).unwrap_or_default())
        .map_err(|e| e.to_string());
    #[cfg(not(feature = "automation"))]
    {
        let _ = path;
        Err("automation is not compiled into this build".into())
    }
}

#[tauri::command]
fn automation_module_remove(id: String) -> Result<(), String> {
    #[cfg(feature = "automation")]
    return wasm::remove(&id).map_err(|e| e.to_string());
    #[cfg(not(feature = "automation"))]
    {
        let _ = id;
        Ok(())
    }
}


/// What a module asks to be allowed to call, and what it was allowed.
#[tauri::command]
fn automation_module_permissions(id: String) -> Result<serde_json::Value, String> {
    #[cfg(feature = "automation")]
    return Ok(serde_json::json!({
        "asks": wasm::manifest_of(&id),
        "granted": wasm::grant_for(&id),
    }));
    #[cfg(not(feature = "automation"))]
    {
        let _ = id;
        Err("automation is not compiled into this build".into())
    }
}

/// Records what the operator allowed this module to call.
#[tauri::command]
fn automation_module_grant(
    id: String,
    modules: Vec<String>,
    flows: Vec<String>,
) -> Result<serde_json::Value, String> {
    #[cfg(feature = "automation")]
    return wasm::set_grant(&id, modules, flows)
        .map(|g| serde_json::to_value(g).unwrap_or_default())
        .map_err(|e| e.to_string());
    #[cfg(not(feature = "automation"))]
    {
        let _ = (id, modules, flows);
        Err("automation is not compiled into this build".into())
    }
}

#[tauri::command]
fn automation_modules_dir() -> Result<String, String> {
    #[cfg(feature = "automation")]
    return wasm::modules_dir()
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| e.to_string());
    #[cfg(not(feature = "automation"))]
    Err("automation is not compiled into this build".into())
}

// ---- Export / import ----

#[tauri::command]
fn automation_export(project_id: String) -> Result<serde_json::Value, String> {
    automation::export(&project_id)
        .map(|b| serde_json::to_value(b).unwrap_or_default())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn automation_import(bundle: serde_json::Value) -> Result<automation::Project, String> {
    let parsed: automation::Bundle =
        serde_json::from_value(bundle).map_err(|e| format!("that is not a project bundle: {e}"))?;
    automation::import(parsed).map_err(|e| e.to_string())
}

/// Starts the project. Answers as soon as the run is under way; progress is
/// read back with `automation_run_status`.
#[tauri::command]
async fn automation_run(project_id: String) -> Result<(), String> {
    #[cfg(feature = "automation")]
    {
        return runner::start(&project_id).await.map_err(|e| e.to_string());
    }
    #[cfg(not(feature = "automation"))]
    {
        let _ = project_id;
        Err("automation is not compiled into this build".into())
    }
}

#[tauri::command]
fn automation_run_stop(project_id: String) {
    #[cfg(feature = "automation")]
    runner::stop(&project_id);
    #[cfg(not(feature = "automation"))]
    let _ = project_id;
}

#[tauri::command]
fn automation_run_status(project_id: String) -> Option<serde_json::Value> {
    #[cfg(feature = "automation")]
    return runner::status(&project_id).and_then(|s| serde_json::to_value(s).ok());
    #[cfg(not(feature = "automation"))]
    {
        let _ = project_id;
        None
    }
}

/// Every run going right now — what the fleet window shows.
#[tauri::command]
fn automation_fleet() -> Vec<serde_json::Value> {
    #[cfg(feature = "automation")]
    return runner::all()
        .into_iter()
        .filter_map(|s| serde_json::to_value(s).ok())
        .collect();
    #[cfg(not(feature = "automation"))]
    Vec::new()
}

/// Opens (or re-focuses) the fleet window: one row per browser in a run.
/// `async` for the same reason as `helper_show` — a webview built on the main
/// thread deadlocks Windows.
#[tauri::command]
async fn automation_fleet_window(app: tauri::AppHandle) {
    use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
    if let Some(w) = app.get_webview_window("fleet") {
        let _ = w.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(&app, "fleet", WebviewUrl::App("index.html#/?fleet=1".into()))
        .title("Hir-Login Fleet")
        .inner_size(460.0, 420.0)
        .min_inner_size(360.0, 240.0)
        .always_on_top(true)
        .build();
    if let Err(e) = built {
        eprintln!("[launcher] fleet window unavailable: {e}");
    }
}

/// Resolves the element under a viewport point, natively. A recorded step stores
/// what this returns, so replay finds the element again at a different window size.
#[tauri::command]
async fn automation_pick(
    profile_id: String,
    x: f64,
    y: f64,
) -> Result<serde_json::Value, String> {
    #[cfg(feature = "automation")]
    {
        return cdp::pick_element(&profile_id, x, y)
            .await
            .map(|p| serde_json::to_value(p).unwrap_or_default())
            .map_err(|e| e.to_string());
    }
    #[cfg(not(feature = "automation"))]
    {
        let _ = (profile_id, x, y);
        Err("automation is not compiled into this build".into())
    }
}

/// One raw CDP call against the attached page. The studio drives every page
/// action through the Motion domain, and this is the only door.
#[tauri::command]
async fn automation_call(
    profile_id: String,
    method: String,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    #[cfg(feature = "automation")]
    {
        return cdp::page_call(&profile_id, &method, params)
            .await
            .map_err(|e| e.to_string());
    }
    #[cfg(not(feature = "automation"))]
    {
        let _ = (profile_id, method, params);
        Err("automation is not compiled into this build".into())
    }
}

// ---- Trash ----

#[tauri::command]
fn trash_list() -> Result<Vec<trash::TrashEntry>, String> {
    trash::list().map_err(|e| e.to_string())
}

#[tauri::command]
fn trash_restore(id: String) -> Result<profile::ProfileMeta, String> {
    trash::restore(&id).map_err(|e| e.to_string())
}

#[tauri::command]
fn trash_purge(id: String) -> Result<(), String> {
    trash::purge(&id).map_err(|e| e.to_string())
}

/// Empties the trash for good; returns how many went.
#[tauri::command]
fn trash_empty() -> Result<usize, String> {
    let entries = trash::list().map_err(|e| e.to_string())?;
    let n = entries.len();
    for e in entries {
        trash::purge(&e.id).map_err(|e| e.to_string())?;
    }
    Ok(n)
}

// ---- Extensions ----

#[tauri::command]
fn extension_list() -> Result<Vec<extensions::ExtensionEntry>, String> {
    extensions::list().map_err(|e| e.to_string())
}

/// Returns what went in, so one bad file in a multi-select loses only itself.
#[tauri::command]
fn extension_import(paths: Vec<String>) -> Result<Vec<extensions::ExtensionEntry>, String> {
    let mut out = Vec::new();
    let mut errs = Vec::new();
    for p in &paths {
        match extensions::import(std::path::Path::new(p)) {
            Ok(e) => out.push(e),
            Err(e) => errs.push(format!("{p}: {e}")),
        }
    }
    if out.is_empty() && !errs.is_empty() {
        return Err(errs.join("; "));
    }
    Ok(out)
}

/// Import from a Web Store link, a bare extension id, or a direct .crx / .zip
/// URL — the launcher fetches the file itself.
#[tauri::command]
async fn extension_import_url(url: String) -> Result<extensions::ExtensionEntry, String> {
    extensions::import_url(&url).await.map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn extension_delete(id: String) -> Result<(), String> {
    extensions::delete(&id).map_err(|e| e.to_string())
}

// ---- Bookmarks ----

#[tauri::command]
fn bookmark_list() -> Result<Vec<bookmarks::Bookmark>, String> {
    bookmarks::list().map_err(|e| e.to_string())
}

#[tauri::command]
fn bookmark_save(entry: bookmarks::Bookmark) -> Result<bookmarks::Bookmark, String> {
    bookmarks::save(entry).map_err(|e| e.to_string())
}

#[tauri::command]
fn bookmark_delete(id: String) -> Result<(), String> {
    bookmarks::delete(&id).map_err(|e| e.to_string())
}

// ---- Data root ----

#[derive(serde::Serialize)]
struct DataRootInfo {
    path: String,
    /// False while the data still lives in the config dir.
    custom: bool,
    migrating: bool,
}

#[tauri::command]
fn data_root_get() -> Result<DataRootInfo, String> {
    let s = settings::load().map_err(|e| e.to_string())?;
    let path = store::data_root().map_err(|e| e.to_string())?;
    Ok(DataRootInfo {
        path: path.display().to_string(),
        custom: s.data_root.is_some(),
        migrating: migrate::in_progress(),
    })
}

/// Progress goes out as `data-migration` events; nothing launches until done.
#[tauri::command]
async fn data_root_migrate(app: tauri::AppHandle, path: String) -> Result<u64, String> {
    if !process::Tracker::shared().running().is_empty() {
        return Err("close every running profile first".into());
    }
    let dst = std::path::PathBuf::from(&path);
    tauri::async_runtime::spawn_blocking(move || migrate::run(&app, &dst))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn profile_bind_proxy(profile_id: String, proxy_id: Option<String>) -> Result<(), String> {
    let mut p = profile::load_raw(&profile_id).map_err(|e| e.to_string())?;
    p.meta.proxy_id = proxy_id;
    profile::save_raw(&mut p).map_err(|e| e.to_string())
}

#[tauri::command]
fn profile_clone(id: String) -> Result<profile::ProfileMeta, String> {
    profile::clone_profile(&id).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, serde::Deserialize)]
struct BulkRow {
    name: String,
    #[serde(default)]
    folder: String,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    proxy: String,
    #[serde(default)]
    color: String,
    /// "http" | "https" | "socks5" (default) — only matters when `proxy` has no
    /// scheme prefix of its own.
    #[serde(default)]
    kind: String,
}

#[derive(Debug, Clone, serde::Serialize)]
struct BulkCreateItem {
    index: usize,
    ok: bool,
    id: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct BulkParseRow {
    row: usize,
    name: String,
    folder: String,
    notes: String,
    proxy: String,
    color: String,
    kind: String,
    error: Option<String>,
}

fn is_valid_hex_color(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() { return true; }
    let h = t.strip_prefix('#').unwrap_or(t);
    h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit())
}

fn normalize_color(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() { return None; }
    let h = if t.starts_with('#') { t.to_string() } else { format!("#{t}") };
    if is_valid_hex_color(&h) { Some(h.to_lowercase()) } else { None }
}

fn parse_csv_rows(text: &str) -> Vec<BulkParseRow> {
    let text = text.trim_start_matches('\u{feff}');
    // Records end at a newline outside quotes, so a quoted note may hold one.
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    for ch in text.chars() {
        match ch {
            '"' => { in_q = !in_q; cur.push(ch); }
            '\n' if !in_q => { lines.push(std::mem::take(&mut cur)); }
            '\r' => {}
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() {
        lines.push(cur);
    }
    let Some(first) = lines.first() else { return Vec::new() };
    // Excel in a Vietnamese/European locale saves ";" (or tabs) rather than ",".
    let delim = [',', ';', '\t']
        .into_iter()
        .max_by_key(|d| first.matches(*d).count())
        .filter(|d| first.contains(*d))
        .unwrap_or(',');
    let rows: Vec<Vec<String>> = lines.iter().map(|l| split_delimited_line(l, delim)).collect();
    bulk_rows_from_table(rows)
}

fn split_delimited_line(line: &str, delim: char) -> Vec<String> {
    let mut cols = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '"' {
            if in_q && chars.peek() == Some(&'"') { cur.push('"'); chars.next(); }
            else { in_q = !in_q; }
        } else if ch == delim && !in_q {
            cols.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(ch);
        }
    }
    cols.push(cur.trim().to_string());
    // strip surrounding quotes
    for c in &mut cols {
        if c.len() >= 2 && c.starts_with('"') && c.ends_with('"') {
            *c = c[1..c.len()-1].replace("\"\"", "\"");
        }
    }
    cols
}

fn xml_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(semi) = rest.find(';').filter(|n| *n <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let ent = &rest[1..semi];
        let decoded = match ent {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "amp" => Some('&'),
            // Non-ASCII text (Vietnamese) is often written as &#250; / &#xFA;.
            _ => ent
                .strip_prefix("#x")
                .or_else(|| ent.strip_prefix("#X"))
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| ent.strip_prefix('#').and_then(|d| d.parse::<u32>().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Text of every `<t>` run inside `xml`, joined (a rich-text cell has several).
fn xml_text_runs(xml: &str) -> String {
    let mut out = String::new();
    let mut rest = xml;
    while let Some(a) = rest.find("<t") {
        let after = &rest[a + 2..];
        // `<t>` or `<t xml:space="preserve">`, not `<tag…` (e.g. `<tr`).
        let Some(first) = after.chars().next() else { break };
        if first != '>' && first != ' ' {
            rest = after;
            continue;
        }
        let Some(gt) = after.find('>') else { break };
        if after[..gt].ends_with('/') {
            rest = &after[gt + 1..];
            continue;
        }
        let body = &after[gt + 1..];
        let Some(end) = body.find("</t>") else { break };
        out.push_str(&xml_unescape(&body[..end]));
        rest = &body[end + 4..];
    }
    out
}

/// "C" -> 2, "AB" -> 27, from a cell reference like "C12".
fn col_index(cell_ref: &str) -> usize {
    let mut n = 0usize;
    for c in cell_ref.chars().take_while(|c| c.is_ascii_alphabetic()) {
        n = n * 26 + (c.to_ascii_uppercase() as usize - 'A' as usize + 1);
    }
    n.saturating_sub(1)
}

fn xml_attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("{name}=\"");
    let i = tag.find(&key)? + key.len();
    let j = tag[i..].find('"')?;
    Some(&tag[i..i + j])
}

/// Rows of cell text from a worksheet's XML. Cells are placed by their own
/// reference: Excel leaves blank cells out entirely, so counting the cells
/// present would slide every later column one to the left.
fn xlsx_sheet_rows(sheet_xml: &str, shared: &[String]) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut rest = sheet_xml;
    while let Some(a) = rest.find("<row") {
        rest = &rest[a..];
        let Some(open_end) = rest.find('>') else { break };
        // A self-closing <row .../> has no cells.
        if rest[..open_end].ends_with('/') {
            rest = &rest[open_end + 1..];
            rows.push(Vec::new());
            continue;
        }
        let Some(end_tag) = rest.find("</row>") else { break };
        let row_xml = &rest[open_end + 1..end_tag];
        rest = &rest[end_tag + 6..];

        let mut cols: Vec<String> = Vec::new();
        let mut r2 = row_xml;
        let mut next_col = 0usize;
        while let Some(c) = r2.find("<c") {
            let after = &r2[c + 2..];
            let Some(first) = after.chars().next() else { break };
            if first != ' ' && first != '>' && first != '/' {
                r2 = after;
                continue;
            }
            let Some(tag_end) = after.find('>') else { break };
            let tag = &after[..tag_end];
            let self_closing = tag.ends_with('/');
            let (inner, remaining) = if self_closing {
                ("", &after[tag_end + 1..])
            } else {
                match after[tag_end + 1..].find("</c>") {
                    Some(e) => (&after[tag_end + 1..tag_end + 1 + e], &after[tag_end + 1 + e + 4..]),
                    None => break,
                }
            };
            r2 = remaining;
            let idx = xml_attr(tag, "r").map(col_index).unwrap_or(next_col);
            next_col = idx + 1;
            let kind = xml_attr(tag, "t").unwrap_or("");
            let value = if kind == "inlineStr" {
                xml_text_runs(inner)
            } else if let (Some(v0), Some(v1)) = (inner.find("<v>"), inner.find("</v>")) {
                let raw = xml_unescape(&inner[v0 + 3..v1]);
                if kind == "s" {
                    raw.trim().parse::<usize>().ok().and_then(|i| shared.get(i).cloned()).unwrap_or_default()
                } else {
                    raw
                }
            } else {
                String::new()
            };
            if cols.len() <= idx {
                cols.resize(idx + 1, String::new());
            }
            cols[idx] = value;
        }
        rows.push(cols);
    }
    rows
}

fn parse_xlsx_rows(path: &std::path::Path) -> Result<Vec<BulkParseRow>, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("không đọc được .xlsx (zip): {e}"))?;
    let mut shared: Vec<String> = Vec::new();
    if let Ok(mut f) = zip.by_name("xl/sharedStrings.xml") {
        let mut s = String::new();
        let _ = f.read_to_string(&mut s);
        for si in s.split("<si").skip(1) {
            // `<si>` or `<si …>`; skip look-alikes such as `<sst`.
            let body = si.split("</si>").next().unwrap_or("");
            shared.push(xml_text_runs(body));
        }
    }
    let mut sheet_xml = String::new();
    let mut found = false;
    for name in ["xl/worksheets/sheet1.xml", "xl/worksheets/sheet.xml"] {
        if let Ok(mut f) = zip.by_name(name) {
            let _ = f.read_to_string(&mut sheet_xml);
            found = true;
            break;
        }
    }
    if !found {
        for i in 0..zip.len() {
            let n = zip.by_index(i).map(|f| f.name().to_string()).unwrap_or_default();
            if n.starts_with("xl/worksheets/sheet") && n.ends_with(".xml") {
                if let Ok(mut f) = zip.by_name(&n) {
                    let _ = f.read_to_string(&mut sheet_xml);
                    found = true;
                    break;
                }
            }
        }
    }
    if !found || sheet_xml.is_empty() {
        return Err("không tìm thấy sheet trong .xlsx".into());
    }
    let rows = xlsx_sheet_rows(&sheet_xml, &shared);
    Ok(bulk_rows_from_table(rows))
}

/// Shared by CSV and XLSX: a header row (name/folder/notes/proxy/color, in any
/// order and language) or, without one, the columns in that fixed order.
fn bulk_rows_from_table(rows: Vec<Vec<String>>) -> Vec<BulkParseRow> {
    if rows.is_empty() {
        return Vec::new();
    }
    let header: Vec<String> = rows[0].iter().map(|h| h.trim().trim_start_matches('\u{feff}').to_lowercase()).collect();
    let ci = |names: &[&str]| header.iter().position(|h| names.contains(&h.as_str()));
    let name_i = ci(&["name", "tên", "ten"]);
    let folder_i = ci(&["folder", "thư mục", "thu muc", "group"]);
    let notes_i = ci(&["notes", "note", "ghi chú", "ghi chu"]);
    let proxy_i = ci(&["proxy"]);
    let color_i = ci(&["color", "màu", "mau"]);
    let kind_i = ci(&["kind", "loại proxy", "loai proxy", "proxy type", "protocol", "loại", "loai"]);
    let has_header = name_i.is_some() || proxy_i.is_some() || notes_i.is_some();
    let start = if has_header { 1 } else { 0 };
    let mut out = Vec::new();
    for (idx, cols) in rows.iter().enumerate().skip(start) {
        if cols.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        let at = |i: usize| cols.get(i).map(|s| s.trim().to_string()).unwrap_or_default();
        let g = |opt: Option<usize>| opt.map(&at).unwrap_or_default();
        let (name, folder, notes, proxy, color, kind) = if has_header {
            (g(name_i), g(folder_i), g(notes_i), g(proxy_i), g(color_i), g(kind_i))
        } else {
            (at(0), at(1), at(2), at(3), at(4), at(5))
        };
        let mut err: Option<String> = None;
        if name.is_empty() {
            err = Some("thiếu name".into());
        } else if !color.is_empty() && !is_valid_hex_color(&color) && !is_valid_hex_color(&format!("#{color}")) {
            err = Some("color phải dạng #rrggbb".into());
        } else if !proxy.is_empty() && proxy::parse_single_with_kind(&proxy, proxy::ProxyKind::parse(&kind)).is_none() {
            err = Some("proxy không hợp lệ".into());
        }
        out.push(BulkParseRow { row: idx + 1, name, folder, notes, proxy, color, kind, error: err });
    }
    out
}

/// A ready-to-fill Excel sheet: header row (with the folder column), two example rows.
fn build_bulk_template_xlsx() -> Result<Vec<u8>, String> {
    use std::io::Write;
    let rows: [[&str; 6]; 3] = [
        ["Tên", "Thư mục", "Ghi chú", "Proxy", "Loại proxy", "Màu"],
        ["FB 01", "Shop A", "Nick chạy quảng cáo", "socks5://user:pass@1.2.3.4:1080", "socks5", "#8b5cf6"],
        ["FB 02", "Shop B", "", "1.2.3.4:8080:user:pass", "http", "#22c55e"],
    ];
    let esc = |t: &str| t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let mut sheet = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><cols><col min="1" max="1" width="18" customWidth="1"/><col min="2" max="2" width="18" customWidth="1"/><col min="3" max="3" width="28" customWidth="1"/><col min="4" max="4" width="38" customWidth="1"/><col min="5" max="5" width="14" customWidth="1"/><col min="6" max="6" width="12" customWidth="1"/></cols><sheetData>"#);
    for (r, row) in rows.iter().enumerate() {
        sheet.push_str(&format!(r#"<row r="{}">"#, r + 1));
        for (c, v) in row.iter().enumerate() {
            if v.is_empty() { continue; }
            let col = (b'A' + c as u8) as char;
            sheet.push_str(&format!(r#"<c r="{col}{}" t="inlineStr"><is><t xml:space="preserve">{}</t></is></c>"#, r + 1, esc(v)));
        }
        sheet.push_str("</row>");
    }
    sheet.push_str("</sheetData></worksheet>");
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let o = zip::write::SimpleFileOptions::default();
        let mut add = |name: &str, body: &str| -> Result<(), String> {
            z.start_file(name, o).map_err(|e| e.to_string())?;
            z.write_all(body.as_bytes()).map_err(|e| e.to_string())
        };
        add("[Content_Types].xml", r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#)?;
        add("_rels/.rels", r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#)?;
        add("xl/workbook.xml", r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Profiles" sheetId="1" r:id="rId1"/></sheets></workbook>"#)?;
        add("xl/_rels/workbook.xml.rels", r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#)?;
        add("xl/worksheets/sheet1.xml", &sheet)?;
        z.finish().map_err(|e| e.to_string())?;
    }
    Ok(buf.into_inner())
}

#[tauri::command]
fn bulk_template_save(path: String) -> Result<(), String> {
    std::fs::write(&path, build_bulk_template_xlsx()?).map_err(|e| e.to_string())
}

#[tauri::command]
fn bulk_parse_file(path: String) -> Result<Vec<BulkParseRow>, String> {
    let p = std::path::Path::new(&path);
    if !p.exists() { return Err("file không tồn tại".into()); }
    let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
    if ext == "xlsx" {
        parse_xlsx_rows(p)
    } else {
        let text = std::fs::read_to_string(p).map_err(|e| e.to_string())?;
        Ok(parse_csv_rows(&text))
    }
}

#[tauri::command]
fn profile_bulk_create(rows: Vec<BulkRow>) -> Result<Vec<BulkCreateItem>, String> {
    let fps = fingerprints::list_all().map_err(|e| e.to_string())?;
    if fps.is_empty() { return Err("chưa có fingerprint nào — hãy thêm fingerprint trước".into()); }
    let mut out = Vec::new();
    for (idx, r) in rows.into_iter().enumerate() {
        if r.name.trim().is_empty() {
            out.push(BulkCreateItem { index: idx, ok: false, id: None, error: Some("thiếu name".into()) });
            continue;
        }
        if !r.color.trim().is_empty() && normalize_color(&r.color).is_none() {
            out.push(BulkCreateItem { index: idx, ok: false, id: None, error: Some("color phải dạng #rrggbb".into()) });
            continue;
        }
        let proxy_id: Option<String> = if r.proxy.trim().is_empty() {
            None
        } else {
            match proxy::parse_single_with_kind(r.proxy.trim(), proxy::ProxyKind::parse(&r.kind)) {
                Some(entry) => match proxy::upsert_dedup(entry) {
                    Ok(e) => Some(e.id),
                    Err(e) => {
                        out.push(BulkCreateItem { index: idx, ok: false, id: None, error: Some(format!("proxy lỗi: {e}")) });
                        continue;
                    }
                },
                None => {
                    out.push(BulkCreateItem { index: idx, ok: false, id: None, error: Some("proxy không hợp lệ".into()) });
                    continue;
                }
            }
        };
        // pick random fingerprint
        let pick = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut h = DefaultHasher::new();
            r.name.hash(&mut h);
            idx.hash(&mut h);
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos().hash(&mut h);
            (h.finish() as usize) % fps.len()
        };
        let tpl_id = fps[pick].id.clone();
        let mut merged = match merge_library_fingerprint(&tpl_id) {
            Ok(m) => m,
            Err(e) => {
                out.push(BulkCreateItem { index: idx, ok: false, id: None, error: Some(e) });
                continue;
            }
        };
        merged.insert("name".into(), Value::String(r.name.trim().to_string()));
        merged.insert("notes".into(), Value::String(r.notes.clone()));
        if let Some(col) = normalize_color(&r.color) {
            if let Some(meta) = merged.get_mut("_meta").and_then(|v| v.as_object_mut()) {
                meta.insert("color".into(), Value::String(col));
            }
        }
        if let Some(pid) = proxy_id.clone() {
            if let Some(meta) = merged.get_mut("_meta").and_then(|v| v.as_object_mut()) {
                meta.insert("proxy_id".into(), Value::String(pid));
            }
        }
        if !r.folder.trim().is_empty() {
            if let Some(meta) = merged.get_mut("_meta").and_then(|v| v.as_object_mut()) {
                meta.insert("folder".into(), Value::String(r.folder.trim().to_string()));
            }
        }
        // enrich picks random platform/hw/noise seed
        enrich_new_config(None, &mut merged);
        ensure_default_noise(&mut merged);
        match save_profile_core(None, Value::Object(merged), false) {
            Ok(meta) => out.push(BulkCreateItem { index: idx, ok: true, id: Some(meta.id), error: None }),
            Err(e) => out.push(BulkCreateItem { index: idx, ok: false, id: None, error: Some(e) }),
        }
    }
    Ok(out)
}

/// Import profiles verbatim under fresh ids; returns the count.
#[tauri::command]
fn profile_import(payloads: Vec<Value>) -> Result<usize, String> {
    let mut n = 0;
    for mut payload in payloads {
        if let Some(obj) = payload.as_object_mut() {
            match obj.get_mut("_meta").and_then(|m| m.as_object_mut()) {
                Some(meta) => {
                    meta.insert("id".into(), Value::String(String::new()));
                }
                None => {
                    obj.insert("_meta".into(), serde_json::json!({ "id": "" }));
                }
            }
        }
        save_profile_core(None, payload, false)?;
        n += 1;
    }
    Ok(n)
}

/// Export profiles as a folder-per-profile bundle under `dest` — carry the
/// whole folder to another machine and import it back with `profile_import_folder`.
#[tauri::command]
fn profile_export_folder(ids: Vec<String>, dest: String) -> Result<usize, String> {
    profile::export_bundle(&ids, std::path::Path::new(&dest)).map_err(|e| e.to_string())
}

/// Import every subfolder of `src` as a profile: a folder written by
/// `profile_export_folder` round-trips its config + browser data; a bare
/// folder of raw browser data (from another install) is adopted as-is and
/// given a random library fingerprint.
#[tauri::command]
fn profile_import_folder(src: String) -> Result<usize, String> {
    profile::import_bundle(std::path::Path::new(&src)).map_err(|e| e.to_string())
}

// ---- Clipboard (via tauri-plugin-clipboard-manager; webview navigator.clipboard throws) ----

#[tauri::command]
fn clipboard_write(app: tauri::AppHandle, text: String) -> Result<(), String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}

#[tauri::command]
fn clipboard_read(app: tauri::AppHandle) -> Result<String, String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard().read_text().map_err(|e| e.to_string())
}

#[tauri::command]
fn profile_set_pin(id: String, pinned: bool) -> Result<(), String> {
    profile::set_pin(&id, pinned).map_err(|e| e.to_string())
}

#[tauri::command]
fn profile_set_folder(id: String, folder: String) -> Result<(), String> {
    profile::set_folder(&id, &folder).map_err(|e| e.to_string())
}

/// Rename folder (retag profiles); returns count.
#[tauri::command]
fn folder_rename(old: String, new: String) -> Result<usize, String> {
    profile::rename_folder(&old, &new).map_err(|e| e.to_string())
}

/// Delete folder; `delete_profiles` true → remove, false → unfile.
#[tauri::command]
fn folder_delete(folder: String, delete_profiles: bool) -> Result<usize, String> {
    profile::delete_folder(&folder, delete_profiles).map_err(|e| e.to_string())
}

/// Host OS in fingerprint-library vocabulary (macOS/Windows/Linux).
#[tauri::command]
fn host_platform() -> String {
    match std::env::consts::OS {
        "macos" => "macOS",
        "windows" => "Windows",
        "linux" => "Linux",
        other => other,
    }
    .to_string()
}

#[tauri::command]
fn profile_create_from_template(
    window: tauri::WebviewWindow,
    template_id: String,
) -> Result<profile::ProfileMeta, String> {
    create_from_fingerprint_core(Some(&window), &template_id)
}

/// Merge library fingerprint into fresh profile map; tz/lang/geo set to "auto" sentinel.
pub fn merge_library_fingerprint(
    template_id: &str,
) -> Result<serde_json::Map<String, Value>, String> {
    let entry = fingerprints::get(template_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("unknown fingerprint id: {template_id}"))?;

    let mut merged = serde_json::Map::new();
    merged.insert(
        "_meta".into(),
        serde_json::json!({
            "id": "",
            "proxy_id": null,
            "last_launched_at": null,
            "gpu_preset_id": entry.id,
        }),
    );
    if let Some(o) = entry.payload.as_object() {
        for (k, v) in o {
            if k == "_meta" { continue; }
            merged.insert(k.clone(), v.clone());
        }
    }

    // launch-time resolver fills tz/lang/geo from the bound proxy
    merged.insert("timezone".into(), Value::String("auto".into()));
    if let Some(nav) = merged.get_mut("navigator").and_then(|v| v.as_object_mut()) {
        nav.insert("language".into(), Value::String("auto".into()));
        nav.remove("accept_language");
        nav.remove("languages");
    }
    merged.insert("geolocation".into(), serde_json::json!({ "mode": "auto" }));
    Ok(merged)
}

/// Build + persist a profile from a library fingerprint id (UI template path).
pub fn create_from_fingerprint_core(
    window: Option<&tauri::WebviewWindow>,
    template_id: &str,
) -> Result<profile::ProfileMeta, String> {
    let merged = merge_library_fingerprint(template_id)?;
    save_profile_core(window, Value::Object(merged), true)
}

/// Produce uniquified fingerprint config WITHOUT persisting (API get-new-fingerprint).
pub fn build_fingerprint_config(
    window: Option<&tauri::WebviewWindow>,
    template_id: &str,
) -> Result<serde_json::Map<String, Value>, String> {
    let mut merged = merge_library_fingerprint(template_id)?;
    enrich_new_config(window, &mut merged);
    ensure_default_noise(&mut merged);
    Ok(merged)
}

/// Add the UI's default noise block (every vector present, disabled, seed 0 —
/// the sentinel `save_raw` fills per-profile) when a config carries none, so
/// API/SDK profiles match UI profiles and get a unique seed instead of none.
pub fn ensure_default_noise(cfg: &mut serde_json::Map<String, Value>) {
    if cfg.contains_key("noise") {
        return;
    }
    cfg.insert(
        "noise".into(),
        serde_json::json!({
            "canvas":       { "enabled": false, "seed": 0 },
            "webgl":        { "enabled": false, "seed": 0, "intensity": 0 },
            "audio":        { "enabled": false, "seed": 0 },
            "client_rects": { "enabled": false, "seed": 0, "max_offset": 0 },
            "sensors":      { "enabled": false, "seed": 0 },
            "fonts":        { "enabled": false, "seed": 0 }
        }),
    );
}

#[derive(serde::Serialize)]
pub struct PresetEnrichPicks {
    pub hardware_concurrency: u32,
    pub device_memory: u32,
    pub platform_version: Option<String>,
}

/// Editor preview: draw a fresh hw + platform_version triple from the same tables save uses.
#[tauri::command]
fn enrich_picks_for_preset(preset_id: String) -> Result<PresetEnrichPicks, String> {
    let entry = fingerprints::get(&preset_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("unknown fingerprint id: {preset_id}"))?;
    let platform = entry
        .payload
        .get("navigator")
        .and_then(|n| n.get("platform"))
        .and_then(|v| v.as_str())
        .unwrap_or("macOS")
        .to_string();
    let mut payload = serde_json::Map::new();
    payload.insert(
        "_meta".into(),
        serde_json::json!({ "gpu_preset_id": preset_id }),
    );
    payload.insert(
        "navigator".into(),
        serde_json::json!({ "platform": platform }),
    );
    // Mirror enrich_new_config order: platform_version first, then hardware.
    randomize_platform_version(&mut payload);
    randomize_hardware(&mut payload);
    let nav = payload
        .get("navigator")
        .and_then(|v| v.as_object())
        .ok_or("internal: navigator missing after randomize")?;
    let cores = nav
        .get("hardware_concurrency")
        .and_then(|v| v.as_u64())
        .ok_or("internal: hardware_concurrency missing")? as u32;
    let mem = nav
        .get("device_memory")
        .and_then(|v| v.as_u64())
        .ok_or("internal: device_memory missing")? as u32;
    let pv = nav
        .get("platform_version")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    Ok(PresetEnrichPicks {
        hardware_concurrency: cores,
        device_memory: mem,
        platform_version: pv,
    })
}

// ---- Fingerprint library ----

#[tauri::command]
fn fingerprint_list() -> Result<Vec<fingerprints::LibraryEntry>, String> {
    fingerprints::list_all().map_err(|e| e.to_string())
}

/// What this machine's GPU can actually do. Cached; `force` re-asks the engine.
/// Slow on the first call — it starts the engine off-screen — so the UI asks once.
#[tauri::command]
async fn gpu_caps(force: bool) -> Result<gpu_caps::HostGlCaps, String> {
    gpu_caps::probe(force).await.map_err(|e| e.to_string())
}

/// Whether the machine can wear each library fingerprint, keyed by id. Kept out of
/// fingerprint_list() so that stays fast; an empty map means "not known", not "all fine".
#[tauri::command]
async fn gpu_caps_compat(
) -> Result<std::collections::HashMap<String, gpu_caps::Compat>, String> {
    let caps = match gpu_caps::probe(false).await {
        Ok(c) => c,
        Err(_) => return Ok(Default::default()),
    };
    let entries = fingerprints::list_all().map_err(|e| e.to_string())?;
    Ok(entries
        .into_iter()
        .map(|e| (e.id, gpu_caps::compat(&e.payload, &caps)))
        .collect())
}

#[tauri::command]
fn fingerprint_get(id: String) -> Result<Option<fingerprints::LibraryEntry>, String> {
    fingerprints::get(&id).map_err(|e| e.to_string())
}

#[tauri::command]
fn fingerprint_import(json_text: String, id_hint: Option<String>) -> Result<fingerprints::LibraryEntry, String> {
    fingerprints::import(&json_text, id_hint).map_err(|e| e.to_string())
}

/// Bulk-import every `.json` file in a chosen folder as a library entry.
#[tauri::command]
fn fingerprint_import_folder(dir: String) -> Result<usize, String> {
    fingerprints::import_folder(std::path::Path::new(&dir)).map_err(|e| e.to_string())
}

#[tauri::command]
fn fingerprint_delete(id: String) -> Result<(), String> {
    fingerprints::delete(&id).map_err(|e| e.to_string())
}

/// Path to fingerprint library dir (UI "Open library folder").
#[tauri::command]
fn fingerprint_dir() -> Result<String, String> {
    store::fingerprints_dir()
        .map(|p| p.display().to_string())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn read_text_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}

// ---- Process tracker ----

#[tauri::command]
fn process_list() -> Vec<process::RunningProfile> {
    process::Tracker::shared().running()
}

#[tauri::command]
async fn process_kill(profile_id: String) -> Result<bool, String> {
    process::Tracker::shared()
        .kill(&profile_id)
        .await
        .map_err(|e| e.to_string())
}

// ---- Proxies ----

#[tauri::command]
fn proxy_list() -> Result<Vec<proxy::ProxyEntry>, String> {
    // Newest-first display order; internal paths still read raw on-disk order.
    let mut list = proxy::list().map_err(|e| e.to_string())?;
    list.reverse();
    Ok(list)
}

#[tauri::command]
fn proxy_save(entry: proxy::ProxyEntry) -> Result<proxy::ProxyEntry, String> {
    proxy::upsert(entry).map_err(|e| e.to_string())
}

#[tauri::command]
fn proxy_delete(id: String) -> Result<(), String> {
    proxy::delete(&id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn proxy_check(entry: proxy::ProxyEntry) -> Result<u128, String> {
    proxy::probe(&entry).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn proxy_check_udp(entry: proxy::ProxyEntry) -> Result<u128, String> {
    proxy::probe_udp(&entry).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn proxy_geo(entry: proxy::ProxyEntry, provider: Option<String>) -> Result<proxy::GeoInfo, String> {
    proxy::geo_check(&entry, provider).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn proxy_full_test(entry: proxy::ProxyEntry) -> Result<proxy::TestSnapshot, String> {
    proxy::full_test(&entry).await.map_err(|e| e.to_string())
}

#[tauri::command]
fn proxy_history(id: String) -> Result<Vec<proxy::TestSnapshot>, String> {
    proxy::history(&id).map_err(|e| e.to_string())
}

#[tauri::command]
fn proxy_last_test(id: String) -> Option<proxy::TestSnapshot> {
    proxy::latest_test(&id)
}

#[tauri::command]
fn proxy_bulk_import(text: String, kind: String) -> Result<usize, String> {
    let default_kind = match kind.as_str() {
        "http" => proxy::ProxyKind::Http,
        "https" => proxy::ProxyKind::Https,
        _ => proxy::ProxyKind::Socks5,
    };
    let parsed = proxy::parse_bulk(&text, default_kind);
    proxy::bulk_save(parsed).map_err(|e| e.to_string())
}

/// Parse bulk-import text without saving (preview list with per-row test).
#[tauri::command]
fn proxy_bulk_parse(text: String, kind: String) -> Vec<proxy::ProxyEntry> {
    let default_kind = match kind.as_str() {
        "http" => proxy::ProxyKind::Http,
        "https" => proxy::ProxyKind::Https,
        _ => proxy::ProxyKind::Socks5,
    };
    proxy::parse_bulk(&text, default_kind)
}

/// Persist pre-tested proxies (bulk dialog).
#[tauri::command]
fn proxy_bulk_save(entries: Vec<proxy::ProxyEntry>) -> Result<usize, String> {
    proxy::bulk_save(entries).map_err(|e| e.to_string())
}

// ---- Launcher ----

#[tauri::command]
async fn launch(profile_id: String) -> Result<u32, String> {
    // UI launches: no CDP, headed. The bus goes along even with no group so the
    // page helper has somewhere to report.
    if migrate::in_progress() {
        return Err("profiles are being moved — try again when that finishes".into());
    }
    let b = bus().await?;
    launch::launch_profile_synced(&profile_id, false, false, None, b.port, &b.token)
        .await
        .map(|o| o.pid)
        .map_err(|e| e.to_string())
}

// ---- Window synchronisation ----

/// The synchronisation bus, started lazily and shared: the port stays closed
/// for a user who never groups profiles.
static BUS: tokio::sync::OnceCell<std::sync::Arc<sync_bus::Bus>> =
    tokio::sync::OnceCell::const_new();

pub(crate) async fn bus() -> Result<std::sync::Arc<sync_bus::Bus>, String> {
    BUS.get_or_try_init(|| async {
        // Fresh per run: tells a browser this launcher started it rather than
        // anything else on the machine.
        let token = uuid::Uuid::new_v4().simple().to_string();
        sync_bus::Bus::start(token).await.map_err(|e| e.to_string())
    })
    .await
    .cloned()
}

/// Opens (or re-focuses) the floating control panel for a group. Same bundle,
/// addressed by hash — a 60px strip does not warrant its own vite entry point.
fn open_sync_panel(app: &tauri::AppHandle, group: &str) {
    use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

    if let Some(w) = app.get_webview_window("sync-panel") {
        let _ = w.set_focus();
        return;
    }
    let url = format!("index.html#/?syncPanel={group}");
    let built = WebviewWindowBuilder::new(app, "sync-panel", WebviewUrl::App(url.into()))
        .title("Hir-Login Sync")
        .inner_size(360.0, 168.0)
        .resizable(true)
        .min_inner_size(280.0, 120.0)
        .resizable(false)
        .always_on_top(true)
        .decorations(false)
        .skip_taskbar(true)
        .build();
    if let Err(e) = built {
        // Not fatal — the group is synchronising, it just has no panel.
        eprintln!("[launcher] sync panel unavailable: {e}");
    }
}

#[tauri::command]
async fn sync_launch(
    app: tauri::AppHandle,
    profile_ids: Vec<String>,
    group: Option<String>,
) -> Result<String, String> {
    if profile_ids.len() < 2 {
        return Err("a group needs at least two profiles".into());
    }
    // A phone profile turns a mirrored mouse press into a touch and a desktop one
    // does not, so refuse a mixed group before anything is launched.
    let mut mobile: Vec<String> = Vec::new();
    let mut desktop: Vec<String> = Vec::new();
    for id in &profile_ids {
        let stored = profile::load_raw(id).map_err(|e| format!("{id}: {e}"))?;
        let name = stored
            .config
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(id.as_str())
            .to_string();
        if profile::claims_mobile(&stored.config) {
            mobile.push(name);
        } else {
            desktop.push(name);
        }
    }
    if !mobile.is_empty() && !desktop.is_empty() {
        return Err(format!(
            "a sync group must be all-mobile or all-desktop — mobile: {}; desktop: {}",
            mobile.join(", "),
            desktop.join(", ")
        ));
    }
    // Phones of one size only: a handset window IS its screen and cannot be resized,
    // and a mirrored press carries a fraction of the viewport, so widths must match.
    if desktop.is_empty() {
        let mut sizes: Vec<(String, String)> = Vec::new();
        for id in &profile_ids {
            let stored = profile::load_raw(id).map_err(|e| format!("{id}: {e}"))?;
            let name = stored
                .config
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(id.as_str())
                .to_string();
            let size = match profile::claimed_screen(&stored.config) {
                Some((w, h)) => format!("{w}x{h}"),
                None => "unknown".to_string(),
            };
            sizes.push((name, size));
        }
        let distinct: std::collections::BTreeSet<&str> =
            sizes.iter().map(|(_, s)| s.as_str()).collect();
        if distinct.len() > 1 {
            let listed: Vec<String> = sizes
                .iter()
                .map(|(n, s)| format!("{n} ({s})"))
                .collect();
            return Err(format!(
                "a mobile sync group must be all one screen size — {}",
                listed.join(", ")
            ));
        }
    }
    let group = group.unwrap_or_else(|| "fleet".to_string());
    let b = bus().await?;

    let mut failed: Vec<String> = Vec::new();
    for id in &profile_ids {
        if let Err(e) = launch::launch_profile_synced(
            id, false, false, Some(&group), b.port, &b.token).await {
            failed.push(format!("{id}: {e}"));
        }
    }
    if failed.len() == profile_ids.len() {
        return Err(format!("nothing launched — {}", failed.join("; ")));
    }
    // A partial launch is still a usable group; just say what did not make it.
    if !failed.is_empty() {
        eprintln!("[launcher] sync group '{group}': {} failed — {}",
                  failed.len(), failed.join("; "));
    }
    open_sync_panel(&app, &group);
    Ok(group)
}

#[tauri::command]
async fn sync_status(group: String) -> Result<sync_bus::GroupStatus, String> {
    Ok(bus().await?.status(&group))
}

#[tauri::command]
async fn sync_set_paused(group: String, paused: bool) -> Result<(), String> {
    bus().await?.set_paused(&group, paused);
    Ok(())
}

/// Lays the group's windows out on the primary display's work area — under the
/// menu bar or behind the dock means moving them by hand anyway.
#[tauri::command]
async fn sync_arrange(
    app: tauri::AppHandle,
    group: String,
    layout: sync_bus::Layout,
) -> Result<(), String> {
    let monitor = app
        .primary_monitor()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no display".to_string())?;
    let scale = monitor.scale_factor();
    let pos = monitor.position().to_logical::<i32>(scale);
    let size = monitor.size().to_logical::<i32>(scale);
    // Margin for the menu bar; browsers report logical pixels, as SetBounds wants.
    let top = if cfg!(target_os = "macos") { 28 } else { 0 };
    bus().await?.arrange(
        &group,
        layout,
        (pos.x, pos.y + top, size.width, size.height - top),
    );
    Ok(())
}

/// Asks every window in the group to close; the panel goes with them.
#[tauri::command]
async fn sync_stop(group: String) -> Result<(), String> {
    bus().await?.stop(&group);
    Ok(())
}

/// Holds one profile out of the group — a captcha, a different password.
#[tauri::command]
async fn sync_set_excluded(
    group: String,
    profile: String,
    excluded: bool,
) -> Result<(), String> {
    bus().await?.set_excluded(&group, &profile, excluded);
    Ok(())
}

/// Every profile whose current page has something the helper could fill.
#[tauri::command]
async fn helper_profiles() -> Result<Vec<String>, String> {
    let s = settings::load().map_err(|e| e.to_string())?;
    if !s.helper_enabled {
        return Ok(Vec::new());
    }
    Ok(bus().await?.helper_profiles(&s.helper_triggers))
}

/// What the helper found in one profile.
#[tauri::command]
async fn helper_fields(profile: String) -> Result<serde_json::Value, String> {
    Ok(bus()
        .await?
        .helper_fields(&profile)
        .unwrap_or(serde_json::Value::Null))
}

/// The operator accepted the offer; nothing fills without this. In a group every
/// member fills with its own person — the command travels, the data does not.
#[tauri::command]
async fn helper_fill(profile: String) -> Result<usize, String> {
    let b = bus().await?;
    match b.group_of(&profile) {
        Some(group) => Ok(b.fill_group(&group)),
        None => {
            b.fill(&profile);
            Ok(1)
        }
    }
}

/// Opens (or re-focuses) the helper panel for one profile.
fn open_helper_panel(app: &tauri::AppHandle, profile: &str) {
    use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
    if let Some(w) = app.get_webview_window("helper-panel") {
        let _ = w.set_focus();
        return;
    }
    let url = format!("index.html#/?helperPanel={profile}");
    if let Err(e) = WebviewWindowBuilder::new(app, "helper-panel", WebviewUrl::App(url.into()))
        .title("Hir Helper")
        .inner_size(300.0, 150.0)
        .resizable(false)
        .always_on_top(true)
        .decorations(false)
        .skip_taskbar(true)
        // Unfocused: it appears mid-form, and stealing the keyboard then is
        // worse than not appearing.
        .focused(false)
        .build()
    {
        eprintln!("[launcher] helper panel unavailable: {e}");
    }
}

/// `async` is load-bearing. A sync command runs on the main thread, and building
/// a webview there deadlocks on Windows: WebView2 needs the message loop this
/// command is sitting on, so the panel comes up white and the whole launcher
/// stops answering. An async command runs off that thread and the builder hands
/// the work to the loop properly.
#[tauri::command]
async fn helper_show(app: tauri::AppHandle, profile: String) -> Result<(), String> {
    open_helper_panel(&app, &profile);
    Ok(())
}

#[tauri::command]
fn helper_close(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window("helper-panel") {
        let _ = w.close();
    }
    Ok(())
}

/// The operator closed the panel themselves — a refusal about this page.
/// `helper_close` is the other case: the page moved on, which silences nothing.
#[tauri::command]
async fn helper_dismiss(app: tauri::AppHandle, profile: String) -> Result<(), String> {
    use tauri::Manager;
    bus().await?.helper_dismiss(&profile);
    if let Some(w) = app.get_webview_window("helper-panel") {
        let _ = w.close();
    }
    Ok(())
}

/// Closes the floating panel; the panel calls it once the group is empty.
#[tauri::command]
fn sync_close_panel(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window("sync-panel") {
        let _ = w.close();
    }
    Ok(())
}

// ---- Cookies ----

/// True if profile has a running browser process.
pub fn is_profile_running(profile_id: &str) -> bool {
    process::Tracker::shared()
        .running()
        .iter()
        .any(|r| r.profile_id == profile_id)
}

#[tauri::command]
fn cookies_export(profile_id: String) -> Result<Vec<cookies::Cookie>, String> {
    cookies::export(&profile_id).map_err(|e| e.to_string())
}

/// Export cookies to a user-picked path; returns count written.
#[tauri::command]
fn cookies_export_to_file(profile_id: String, path: String) -> Result<usize, String> {
    let cookies = cookies::export(&profile_id).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(&cookies).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(cookies.len())
}

#[tauri::command]
fn cookies_import(profile_id: String, cookies: Vec<cookies::Cookie>) -> Result<usize, String> {
    // Running browser would clobber the import on exit.
    if is_profile_running(&profile_id) {
        return Err("stop the profile before importing cookies".into());
    }
    cookies::import(&profile_id, &cookies).map_err(|e| e.to_string())
}

// ---- License activation ----

/// Whether this machine already has a valid local activation. Offline check.
#[tauri::command]
fn license_status() -> bool {
    license::is_activated()
}

/// Redeems a key against Supabase; writes the local activation record on
/// success so this machine never needs the network for this again.
#[tauri::command]
async fn license_activate(key: String) -> Result<(), String> {
    license::activate(&key).await.map_err(|e| e.to_string())
}

/// For the Settings page: this machine's key, device id, and whatever
/// customer info has been submitted. None before activation.
#[tauri::command]
fn license_info() -> Option<license::LicenseInfo> {
    license::local_info()
}

/// Sends/updates the customer's name, phone/Zalo and email for this
/// machine's license. Callable again later to fix a typo.
#[tauri::command]
async fn license_submit_info(name: String, phone: String, email: String) -> Result<(), String> {
    license::submit_customer_info(&name, &phone, &email)
        .await
        .map_err(|e| e.to_string())
}

// ---- Settings ----

#[tauri::command]
fn settings_get() -> Result<settings::Settings, String> {
    settings::load().map_err(|e| e.to_string())
}

/// The primary monitor in CSS pixels, so the editor can offer resolutions and
/// refuse the ones this machine cannot actually show. None when there is no
/// monitor to ask (headless), and the editor then offers the full list.
#[tauri::command]
fn host_screen(window: tauri::WebviewWindow) -> Option<(i64, i64)> {
    let monitor = window
        .primary_monitor()
        .ok()
        .flatten()
        .or_else(|| window.current_monitor().ok().flatten())?;
    let scale = monitor.scale_factor();
    if scale <= 0.0 {
        return None;
    }
    let phys = monitor.size();
    let w = (phys.width as f64 / scale).round() as i64;
    let h = (phys.height as f64 / scale).round() as i64;
    (w > 0 && h > 0).then_some((w, h))
}

/// Why the settings file could not be read, for the banner. None = it reads fine.
#[tauri::command]
fn settings_load_error() -> Option<String> {
    settings::load_error()
}

#[tauri::command]
fn settings_save(mut value: settings::Settings) -> Result<(), String> {
    // Saving on top of a file we could not read would write the defaults this
    // form was filled from over whatever the file actually held — the data
    // root among them, which is where every profile lives. The banner says
    // the file was not read; until it is fixed or moved aside, nothing here
    // gets written.
    if let Some(err) = settings::load_error() {
        return Err(format!(
            "Settings were not saved: the file could not be read, so what is on \
             screen are defaults, not your settings. Writing them would lose \
             whatever the file holds — including where your profiles live. Fix \
             or delete it first. ({err})"
        ));
    }
    // Owned by the migration, not the form — which round-trips the whole struct
    // and would reset it while the data sits on another disk.
    if let Ok(cur) = settings::load() {
        value.data_root = cur.data_root;
        // Owned by team_server_start/stop, not the form: the form has no such
        // field, so saving any setting used to reset it to "off" and the server
        // stayed down after the next launch.
        value.server_host = cur.server_host;
        if value.api_secret.is_empty() {
            value.api_secret = cur.api_secret;
        }
    }
    settings::save(&value).map_err(|e| e.to_string())
}

/// Settings page "Test connection": proves the server URL + token work and
/// shows what it currently knows, before the operator relies on it.
#[tauri::command]
async fn team_sync_list() -> Result<Vec<cloud_sync::RemoteProfileStatus>, String> {
    cloud_sync::list_remote().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn team_sync_pull() -> Result<usize, String> {
    cloud_sync::pull_missing().await.map_err(|e| e.to_string())
}

// ---- Team Server (embedded sync server) + Invite codes ----

#[tauri::command]
async fn team_server_start(port: u16, token: String) -> Result<u16, String> {
    let actual = team_server::start(port, token.clone()).await.map_err(|e| e.to_string())?;
    // Persist so setup() auto-resumes after reboot.
    if let Ok(mut s) = settings::load() {
        s.server_host.enabled = true;
        s.server_host.port = actual;
        s.server_host.token = Some(token.clone());
        s.sync.token = Some(token.clone());
        // keep existing sync url sane if possible
        if s.sync.server_url.is_none() {
            let ip = team_server::tailscale_ip().unwrap_or_else(|| "127.0.0.1".into());
            s.sync.server_url = Some(format!("http://{ip}:{actual}"));
            s.sync.enabled = true;
        }
        let _ = settings::save(&s);
    }
    Ok(actual)
}

#[tauri::command]
fn team_server_stop() -> Result<(), String> {
    team_server::stop().map_err(|e| e.to_string())?;
    if let Ok(mut s) = settings::load() {
        s.server_host.enabled = false;
        let _ = settings::save(&s);
    }
    Ok(())
}

#[tauri::command]
fn team_server_status() -> Value {
    if let Some((port, _)) = team_server::running_info() {
        let ip = team_server::tailscale_ip();
        serde_json::json!({"running": true, "port": port, "tailscale_ip": ip})
    } else {
        serde_json::json!({"running": false})
    }
}

#[tauri::command]
fn team_invite_generate(server_url: String, token: String) -> String {
    team_invite::generate_invite_code(&server_url, &token)
}

#[tauri::command]
fn team_invite_generate_with_auth(server_url: String, token: String, auth_key: String) -> String {
    let ak = if auth_key.trim().is_empty() { None } else { Some(auth_key.trim()) };
    team_invite::generate_invite_code_with_auth(&server_url, &token, ak)
}

#[tauri::command]
fn team_invite_parse(code: String) -> Result<Value, String> {
    let (url, token, _auth) = team_invite::parse_invite_code(&code).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({"url": url, "token": token}))
}

/// A profile was saved or created here: send it to the team now.
#[tauri::command]
fn sync_kick() {
    cloud_sync::kick();
}

#[tauri::command]
async fn team_admin(method: String, path: String, body: Option<Value>) -> Result<Value, String> {
    if !(path == "/me" || path.starts_with("/admin/")) {
        return Err("bad path".into());
    }
    cloud_sync::admin_call(&method, &path, body).await.map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn sync_activity() -> cloud_sync::SyncActivity {
    cloud_sync::activity()
}

#[tauri::command]
fn autostart_get() -> bool { autostart::is_enabled() }

#[tauri::command]
fn autostart_set(enabled: bool) -> Result<(), String> { autostart::set_enabled(enabled).map_err(|e| e.to_string()) }

#[tauri::command]
fn tailscale_status() -> Value {
    serde_json::json!({
        "installed": tailscale::is_installed(),
        "connected": tailscale::is_connected(),
        "ip": tailscale::tailscale_ip(),
    })
}

#[tauri::command]
fn tailscale_oauth_get() -> Value {
    let s = settings::load().unwrap_or_default();
    match s.server_host.tailscale_oauth {
        Some(o) => serde_json::json!({"client_id": o.client_id, "has_secret": !o.client_secret.is_empty(), "tag": o.tag}),
        None => serde_json::json!({"client_id": "", "has_secret": false, "tag": ""}),
    }
}

#[tauri::command]
fn tailscale_oauth_set(client_id: String, client_secret: String, tag: String) -> Result<(), String> {
    let mut s = settings::load().map_err(|e| e.to_string())?;
    s.server_host.tailscale_oauth = Some(settings::TailscaleOauth { client_id, client_secret, tag });
    settings::save(&s).map_err(|e| e.to_string())
}

/// Forget the saved OAuth Client entirely — e.g. before pasting a replacement one.
/// This only clears what Hir-Login stored locally; it does not touch anything on
/// Tailscale's side (the client itself, or any key already minted, keeps working).
#[tauri::command]
fn tailscale_oauth_clear() -> Result<(), String> {
    let mut s = settings::load().map_err(|e| e.to_string())?;
    s.server_host.tailscale_oauth = None;
    settings::save(&s).map_err(|e| e.to_string())
}

/// Mint a fresh reusable Tailscale auth key via the saved OAuth client. Good for
/// 90 days (Tailscale's own cap); called fresh each time an invite code is made.
#[tauri::command]
async fn tailscale_create_key(description: String) -> Result<String, String> {
    let s = settings::load().map_err(|e| e.to_string())?;
    let o = s.server_host.tailscale_oauth.ok_or_else(|| "chưa cấu hình OAuth Client".to_string())?;
    tailscale::create_auth_key(&o.client_id, &o.client_secret, &o.tag, &description, 90 * 24 * 3600)
        .await
        .map_err(|e| e.to_string())
}

/// Every Tailscale key in the tailnet, for the in-app "quản lý key" list.
#[tauri::command]
async fn tailscale_list_keys() -> Result<Vec<tailscale::KeyMeta>, String> {
    let s = settings::load().map_err(|e| e.to_string())?;
    let o = s.server_host.tailscale_oauth.ok_or_else(|| "chưa cấu hình OAuth Client".to_string())?;
    tailscale::list_keys(&o.client_id, &o.client_secret).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn tailscale_revoke_key(id: String) -> Result<(), String> {
    let s = settings::load().map_err(|e| e.to_string())?;
    let o = s.server_host.tailscale_oauth.ok_or_else(|| "chưa cấu hình OAuth Client".to_string())?;
    tailscale::revoke_key(&o.client_id, &o.client_secret, &id).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn team_invite_join(code: String) -> Result<Value, String> {
    let (url, token, auth_key) = team_invite::parse_invite_code(&code).map_err(|e| e.to_string())?;
    // Auto-join Tailscale if auth key is embedded and not yet connected.
    // Hard failure blocks joining even though the binary path was wrong — the
    // /health probe will give a clearer "không kết nối được" if the tailnet
    // is really unreachable, so the join is best-effort.
    let mut join_err: Option<String> = None;
    if let Some(ak) = auth_key.as_deref().filter(|s| !s.is_empty()) {
        // `tailscale up` blocks on a network round-trip to the coordination server
        // (can be several seconds); run it off the async runtime so a slow join
        // never stalls other commands or the background sync loop.
        let installed = tokio::task::spawn_blocking(tailscale::is_installed).await.unwrap_or(false);
        let connected = tokio::task::spawn_blocking(tailscale::is_connected).await.unwrap_or(false);
        if !connected {
            if installed {
                let ak_owned = ak.to_string();
                let joined = tokio::task::spawn_blocking(move || tailscale::join_with_auth_key(&ak_owned)).await;
                match joined {
                    Ok(Ok(())) => { tokio::time::sleep(std::time::Duration::from_secs(2)).await; }
                    Ok(Err(e)) => {
                        join_err = Some(e.to_string());
                        eprintln!("[launcher] tailscale join failed (will still try /health): {join_err:?}");
                    }
                    Err(e) => {
                        join_err = Some(format!("tailscale join panicked: {e}"));
                    }
                }
            } else {
                join_err = Some("chưa cài Tailscale".into());
            }
        }
    }
    // Verify connectivity before saving
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = match client
        .get(format!("{}/health", url.trim_end_matches('/')))
        .bearer_auth(&token)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            if let Some(je) = join_err {
                return Err(format!("không kết nối được tới máy chủ: {e} (Tailscale: {je})"));
            }
            return Err(format!("không kết nối được tới máy chủ: {e}"));
        }
    };
    if !resp.status().is_success() {
        return Err(format!("máy chủ trả về lỗi: {}", resp.status()));
    }
    if let Some(je) = join_err {
        eprintln!("[launcher] connected despite join warning: {je}");
    }
    // Save to settings
    let mut s = settings::load().map_err(|e| e.to_string())?;
    s.sync.enabled = true;
    s.sync.server_url = Some(url.clone());
    s.sync.token = Some(token.clone());
    settings::save(&s).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({"url": url, "token": token}))
}

// ---- Automation API ----

/// API connection info: base URL + permanent Bearer JWT (no raw key exposed).
#[tauri::command]
fn api_info() -> Result<Value, String> {
    let s = settings::ensure_secret().map_err(|e| e.to_string())?;
    let token = api::long_lived_token(&s.api_secret)?;
    Ok(serde_json::json!({
        "enabled": s.api_enabled,
        "port": s.api_port,
        "base_url": format!("http://127.0.0.1:{}", s.api_port),
        "token": token,
    }))
}

/// Rotate API secret; live-swap on running server invalidates prior tokens.
#[tauri::command]
fn api_regenerate_token() -> Result<Value, String> {
    let mut s = settings::load().map_err(|e| e.to_string())?;
    s.api_secret = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    settings::save(&s).map_err(|e| e.to_string())?;
    api::set_secret(&s.api_secret);
    let token = api::long_lived_token(&s.api_secret)?;
    Ok(serde_json::json!({
        "enabled": s.api_enabled,
        "port": s.api_port,
        "base_url": format!("http://127.0.0.1:{}", s.api_port),
        "token": token,
    }))
}

// ---- ProxyShard billing API ----

/// Saved billing-API key (empty string when unset).
#[tauri::command]
fn ps_get_key() -> Result<String, String> {
    psapi::get_key().map_err(|e| e.to_string())
}

#[tauri::command]
fn ps_set_key(key: String) -> Result<(), String> {
    psapi::set_key(key).map_err(|e| e.to_string())
}

/// Account profile (email, active_orders, wallet_balance cents) — also acts
/// as the "is the key valid?" probe.
#[tauri::command]
async fn ps_me() -> Result<Value, String> {
    psapi::call("GET", "/user/api/me", &[], None)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn ps_orders(status: String, offset: Option<i64>, limit: Option<i64>) -> Result<Value, String> {
    let mut q = vec![("status".to_string(), status)];
    if let Some(o) = offset {
        q.push(("offset".into(), o.to_string()));
    }
    if let Some(l) = limit {
        q.push(("limit".into(), l.to_string()));
    }
    psapi::call("GET", "/user/api/orders", &q, None)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn ps_order(id: i64) -> Result<Value, String> {
    psapi::call("GET", &format!("/user/api/orders/{id}"), &[], None)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn ps_active(order_id: i64) -> Result<Value, String> {
    psapi::call(
        "GET",
        "/user/api/proxies/active",
        &[("order_id".into(), order_id.to_string())],
        None,
    )
    .await
    .map_err(|e| e.to_string())
}

/// Pull an order's active proxies into the local proxy list. Returns count added.
#[tauri::command]
async fn ps_import_order(order_id: i64, kind: String) -> Result<usize, String> {
    psapi::import_order_proxies(order_id, kind)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn ps_products() -> Result<Value, String> {
    psapi::call("GET", "/user/api/proxies/products", &[], None)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn ps_available_count() -> Result<Value, String> {
    psapi::call("GET", "/user/api/proxies/available-count", &[], None)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn ps_resi_isps(
    tier: String,
    country: String,
    region: String,
    city: String,
) -> Result<Value, String> {
    // Registered in every configuration — the handler list is one literal and
    // cannot be gated per entry — so say so when the code behind it is absent.
    #[cfg(not(feature = "automation"))]
    {
        let _ = (tier, country, region, city);
        return Err("this build has no ProxyShard support".into());
    }
    #[cfg(feature = "automation")]
    psapi::resi_isps(&tier, &country, &region, &city)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn ps_calculate(
    product: String,
    location: Option<String>,
    cycle: Option<String>,
    quantity: Option<i64>,
    promo_code: Option<String>,
    addons_json: Option<String>,
) -> Result<Value, String> {
    let mut q = vec![("product".to_string(), product)];
    if let Some(v) = location.filter(|s| !s.is_empty()) {
        q.push(("location".into(), v));
    }
    if let Some(v) = cycle.filter(|s| !s.is_empty()) {
        q.push(("cycle".into(), v));
    }
    if let Some(v) = quantity {
        q.push(("quantity".into(), v.to_string()));
    }
    if let Some(v) = promo_code.filter(|s| !s.is_empty()) {
        q.push(("promo_code".into(), v));
    }
    // JSON array of add-ons, e.g. [{"addon_key":"p0f_slots","qty":5}].
    // reqwest URL-encodes the value.
    if let Some(v) = addons_json.filter(|s| !s.is_empty()) {
        q.push(("addons_json".into(), v));
    }
    psapi::call("GET", "/user/api/orders/calculate", &q, None)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn ps_purchase(body: Value) -> Result<Value, String> {
    psapi::call("POST", "/user/api/orders/purchase", &[], Some(body))
        .await
        .map_err(|e| e.to_string())
}

/// Buy extra GB of residential traffic for an order.
#[tauri::command]
async fn ps_add_bandwidth(id: i64, amount: i64, promo_code: Option<String>) -> Result<Value, String> {
    let mut body = serde_json::json!({ "amount": amount });
    if let Some(p) = promo_code.filter(|s| !s.is_empty()) {
        body["promo_code"] = Value::String(p);
    }
    psapi::call(
        "POST",
        &format!("/user/api/orders/{id}/add-bandwidth"),
        &[],
        Some(body),
    )
    .await
    .map_err(|e| e.to_string())
}

/// Account-owner traffic for a residential proxy type ("standart" | "premium").
#[tauri::command]
async fn ps_profile_traffic(proxy_type: String) -> Result<Value, String> {
    psapi::call(
        "GET",
        "/user/api/proxies/profile",
        &[("proxy_type".into(), proxy_type)],
        None,
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn ps_renew(id: i64) -> Result<Value, String> {
    psapi::call("POST", &format!("/user/api/orders/{id}/renew"), &[], None)
        .await
        .map_err(|e| e.to_string())
}

/// Residential location reference data (for the proxy generator).
#[tauri::command]
async fn ps_countries(proxy_type: String) -> Result<Value, String> {
    psapi::call(
        "GET",
        "/user/api/proxies/countries",
        &[("proxy_type".into(), proxy_type)],
        None,
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn ps_regions(proxy_type: String, country_code: String) -> Result<Value, String> {
    psapi::call(
        "GET",
        "/user/api/proxies/regions",
        &[
            ("proxy_type".into(), proxy_type),
            ("country_code".into(), country_code),
        ],
        None,
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn ps_cities(proxy_type: String, country_code: String, region_code: String) -> Result<Value, String> {
    psapi::call(
        "GET",
        "/user/api/proxies/cities",
        &[
            ("proxy_type".into(), proxy_type),
            ("country_code".into(), country_code),
            ("region_code".into(), region_code),
        ],
        None,
    )
    .await
    .map_err(|e| e.to_string())
}

/// Assign OS-fingerprint signatures to proxy IPs (consumes p0f slots).
/// `items` is an array of `{ ip, signature }`.
#[tauri::command]
async fn ps_signature_set(order_id: i64, items: Value) -> Result<Value, String> {
    psapi::call(
        "POST",
        &format!("/user/api/orders/{order_id}/signature/set"),
        &[],
        Some(serde_json::json!({ "items": items })),
    )
    .await
    .map_err(|e| e.to_string())
}

/// Set/clear an order's tag.
#[tauri::command]
async fn ps_set_tag(id: i64, tag: String) -> Result<Value, String> {
    psapi::call(
        "POST",
        &format!("/user/api/orders/{id}/tag"),
        &[],
        Some(serde_json::json!({ "tag": tag })),
    )
    .await
    .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Bring the main window back from the tray / minimized state and focus it.
fn show_main_window(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Mirrors Tauri's own `Menu::default()`, with one change: the native "Quit"
/// item is a plain `MenuItem` (`app_quit`) instead of `PredefinedMenuItem::quit`.
/// The predefined one calls `[NSApp terminate:]` straight through AppKit,
/// which ends the process immediately and never reaches Rust at all — not
/// `RunEvent::ExitRequested`, nothing — so Cmd+Q could never be made to wait
/// for a running profile. Routing it through our own menu item means Cmd+Q
/// (its accelerator) reaches `on_menu_event` like any other menu click.
fn build_app_menu(app: &tauri::AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu};
    let pkg_info = app.package_info();
    let config = app.config();
    let about_metadata = AboutMetadata {
        name: Some(pkg_info.name.clone()),
        version: Some(pkg_info.version.to_string()),
        copyright: config.bundle.copyright.clone(),
        authors: config.bundle.publisher.clone().map(|p| vec![p]),
        ..Default::default()
    };

    let window_menu = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
            #[cfg(target_os = "macos")]
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, None)?,
        ],
    )?;

    let help_menu = Submenu::with_items(
        app,
        "Help",
        true,
        &[
            #[cfg(not(target_os = "macos"))]
            &PredefinedMenuItem::about(app, None, Some(about_metadata.clone()))?,
        ],
    )?;

    // "Cmd" only means Command on macOS; elsewhere it's the Super/Windows key,
    // which isn't the accelerator anyone expects here, so it's mac-only.
    #[cfg(target_os = "macos")]
    let quit_accelerator = Some("Cmd+Q");
    #[cfg(not(target_os = "macos"))]
    let quit_accelerator: Option<&str> = None;
    #[cfg(not(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    )))]
    let quit_item = MenuItem::with_id(app, "app_quit", "Quit Hir-Login", true, quit_accelerator)?;

    Menu::with_items(
        app,
        &[
            #[cfg(target_os = "macos")]
            &Submenu::with_items(
                app,
                pkg_info.name.clone(),
                true,
                &[
                    &PredefinedMenuItem::about(app, None, Some(about_metadata))?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::services(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::hide(app, None)?,
                    &PredefinedMenuItem::hide_others(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &quit_item,
                ],
            )?,
            #[cfg(not(any(
                target_os = "linux",
                target_os = "dragonfly",
                target_os = "freebsd",
                target_os = "netbsd",
                target_os = "openbsd"
            )))]
            &Submenu::with_items(
                app,
                "File",
                true,
                &[
                    &PredefinedMenuItem::close_window(app, None)?,
                    #[cfg(not(target_os = "macos"))]
                    &quit_item,
                ],
            )?,
            &Submenu::with_items(
                app,
                "Edit",
                true,
                &[
                    &PredefinedMenuItem::undo(app, None)?,
                    &PredefinedMenuItem::redo(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::cut(app, None)?,
                    &PredefinedMenuItem::copy(app, None)?,
                    &PredefinedMenuItem::paste(app, None)?,
                    &PredefinedMenuItem::select_all(app, None)?,
                ],
            )?,
            #[cfg(target_os = "macos")]
            &Submenu::with_items(
                app,
                "View",
                true,
                &[&PredefinedMenuItem::fullscreen(app, None)?],
            )?,
            &window_menu,
            &help_menu,
        ],
    )
}

/// Refuse to quit while any profile is still running. Each profile already
/// checks its data back in to Team Sync when its own browser window closes
/// (see `process.rs`) — quitting the app out from under a running profile
/// would skip that and leave its lock stuck for the rest of the team, so the
/// operator is asked to close their profiles first instead.
fn quit_gracefully(app: &tauri::AppHandle) {
    let running = process::Tracker::shared().running();
    eprintln!("[launcher] quit requested; {} profile(s) running", running.len());
    if running.is_empty() {
        app.exit(0);
        return;
    }
    use tauri_plugin_dialog::DialogExt;
    show_main_window(app);
    app.dialog()
        .message(
            "Còn profile đang chạy. Đóng hết trình duyệt của các profile đó trước khi thoát \
             Hir-Login, để dữ liệu được đồng bộ đầy đủ lên Team Sync.",
        )
        .title("Chưa thể thoát")
        .kind(tauri_plugin_dialog::MessageDialogKind::Warning)
        .show(|_| {});
}

pub fn run() {
    tauri::Builder::default()
        // Must be the first plugin: a second launch focuses the running window.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .menu(build_app_menu)
        .on_menu_event(|app, event| {
            if event.id.as_ref() == "app_quit" {
                quit_gracefully(app);
            }
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let to_tray = settings::load().map(|s| s.minimize_to_tray).unwrap_or(true);
                if window.label() == "main" && to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            sync_launch,
            sync_status,
            sync_set_paused,
            sync_arrange,
            sync_stop,
            sync_set_excluded,
            sync_close_panel,
            helper_profiles,
            helper_fields,
            helper_fill,
            helper_show,
            helper_close,
            helper_dismiss,
            profile_list,
            profile_get,
            profile_save,
            profile_delete,
            automation_available,
            automation_list,
            automation_create,
            automation_save,
            automation_delete,
            automation_duplicate,
            automation_launch,
            automation_attach,
            automation_detach,
            automation_attached,
            automation_screencast,
            automation_call,
            automation_pick,
            automation_run,
            automation_run_stop,
            automation_run_status,
            automation_fleet,
            automation_fleet_window,
            automation_display,
            automation_tls_fingerprints,
            automation_modules,
            automation_module_install,
            automation_module_remove,
            automation_module_permissions,
            automation_module_grant,
            automation_modules_dir,
            automation_export,
            automation_import,
            trash_list,
            trash_restore,
            trash_purge,
            trash_empty,
            extension_list,
            extension_import,
            extension_import_url,
            extension_delete,
            bookmark_list,
            bookmark_save,
            bookmark_delete,
            data_root_get,
            data_root_migrate,
            profile_bind_proxy,
            profile_clone,
            profile_import,
            bulk_parse_file,
            bulk_template_save,
            profile_bulk_create,
            profile_export_folder,
            profile_import_folder,
            clipboard_write,
            clipboard_read,
            profile_set_pin,
            profile_set_folder,
            folder_rename,
            folder_delete,
            host_platform,
            profile_create_from_template,
            enrich_picks_for_preset,
            fingerprint_list,
            gpu_caps,
            gpu_caps_compat,
            fingerprint_get,
            fingerprint_import,
            fingerprint_import_folder,
            fingerprint_delete,
            fingerprint_dir,
            read_text_file,
            process_list,
            process_kill,
            proxy_list,
            proxy_save,
            proxy_delete,
            proxy_check,
            proxy_check_udp,
            proxy_geo,
            proxy_full_test,
            proxy_history,
            proxy_last_test,
            proxy_bulk_import,
            proxy_bulk_parse,
            proxy_bulk_save,
            launch,
            settings_get,
            settings_save,
            team_sync_list,
            team_sync_pull,
            team_server_start,
            team_server_stop,
            team_server_status,
            team_invite_generate,
            team_invite_generate_with_auth,
            team_invite_parse,
            team_invite_join,
            tailscale_status,
            tailscale_oauth_get,
            tailscale_oauth_set,
            tailscale_oauth_clear,
            tailscale_create_key,
            tailscale_list_keys,
            tailscale_revoke_key,
            sync_activity,
            team_admin,
            sync_kick,
            autostart_get,
            autostart_set,
            license_status,
            license_activate,
            license_info,
            license_submit_info,
            settings_load_error,
            host_screen,
            api_info,
            api_regenerate_token,
            ps_get_key,
            ps_set_key,
            ps_me,
            ps_orders,
            ps_order,
            ps_active,
            ps_import_order,
            ps_products,
            ps_available_count,
            ps_resi_isps,
            ps_calculate,
            ps_purchase,
            ps_add_bandwidth,
            ps_profile_traffic,
            ps_renew,
            ps_set_tag,
            ps_countries,
            ps_regions,
            ps_cities,
            ps_signature_set,
            cookies_export,
            cookies_export_to_file,
            cookies_import,
            mcp_download,
            runtime::runtime_status,
            runtime::runtime_install,
            runtime::launcher_update_check,
        ])
        .setup(|app| {
            let _ = APP_HANDLE.set(app.handle().clone());

            {
                use tauri::menu::{Menu, MenuItem};
                use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
                let show = MenuItem::with_id(app, "tray_show", "Show Launcher", true, None::<&str>)?;
                let quit = MenuItem::with_id(app, "tray_quit", "Quit", true, None::<&str>)?;
                let menu = Menu::with_items(app, &[&show, &quit])?;
                if let Some(icon) = app.default_window_icon().cloned() {
                    let builder = TrayIconBuilder::with_id("main").icon(icon);
                    // The macOS menu bar wants a stencil: drawn from the icon's
                    // shape alone, so it is dark on a light bar and light on a
                    // dark one instead of staying purple in both.
                    #[cfg(target_os = "macos")]
                    let builder = builder.icon_as_template(true);
                    builder
                        .tooltip("Hir-Login")
                        .menu(&menu)
                        .show_menu_on_left_click(false)
                        .on_menu_event(|app, e| match e.id.as_ref() {
                            "tray_show" => show_main_window(app),
                            "tray_quit" => quit_gracefully(app),
                            _ => {}
                        })
                        .on_tray_icon_event(|tray, e| {
                            if let TrayIconEvent::Click {
                                button: MouseButton::Left,
                                button_state: MouseButtonState::Up,
                                ..
                            } = e
                            {
                                show_main_window(tray.app_handle());
                            }
                        })
                        .build(app)?;
                }
            }

            // Win/Linux: strip native caption since macOS-only titleBarStyle:Overlay leaves it.
            #[cfg(not(target_os = "macos"))]
            {
                use tauri::Manager;
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.set_decorations(false);
                }
            }

            // Migrate already-created profiles' UA + client_hints to the
            // current engine version (independent of the fingerprint seed).
            tauri::async_runtime::spawn(async {
                runtime::ensure_profiles_migrated().await;
            });

            // Ship the extra fingerprint set with the app: it is not part of
            // the set downloaded from the engine CDN, so it would otherwise
            // exist only on the machine that generated it.
            {
                use tauri::Manager;
                if let Ok(res) = app.path().resource_dir() {
                    let ver = app.package_info().version.to_string();
                    match fingerprints::seed_bundled(&res, &ver) {
                        Ok(n) if n > 0 => eprintln!("[launcher] seeded {n} bundled fingerprints"),
                        Ok(_) => {}
                        Err(e) => eprintln!("[launcher] fingerprint seed failed: {e}"),
                    }
                }
            }

            // --minimized flag from autostart: start hidden to tray (if enabled)
            {
                use tauri::Manager;
                let minimized = std::env::args().any(|a| a == "--minimized");
                if minimized {
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.hide();
                    }
                }
            }

            // Point the heavy directories wherever the operator moved them,
            // before anything reads a profile.
            if let Ok(s) = settings::load() {
                if let Some(root) = s.data_root.as_deref().filter(|r| !r.is_empty()) {
                    store::set_data_root(Some(std::path::PathBuf::from(root)));
                }
            }

            // Auto-resume hosted team server if the operator left "Làm máy chủ" on.
            if let Ok(s) = settings::load() {
                if s.server_host.enabled {
                    let token = s.server_host.token.clone()
                        .filter(|t| t.len() >= 8)
                        .or_else(|| s.sync.token.clone().filter(|t| t.len() >= 8));
                    if let Some(token) = token {
                        let port = if s.server_host.port != 0 { s.server_host.port } else { 8787 };
                        tauri::async_runtime::spawn(async move {
                            // The port can still be held for a moment by the instance
                            // that just quit (an update relaunch, a quick restart), so
                            // try a few times before giving up.
                            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                            for attempt in 1..=6 {
                                match team_server::start(port, token.clone()).await {
                                    Ok(actual) => {
                                        eprintln!("[launcher] team server auto-resumed :{actual}");
                                        return;
                                    }
                                    Err(e) if team_server::is_running() => {
                                        let _ = e;
                                        return;
                                    }
                                    Err(e) => {
                                        eprintln!("[launcher] team server auto-resume attempt {attempt} failed: {e}");
                                        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                                    }
                                }
                            }
                        });
                    } else {
                        eprintln!("[launcher] team server auto-resume skipped: no team token saved");
                    }
                }
            }

            // Extensions imported before ids became canonical: merge duplicates.
            std::thread::spawn(|| {
                let n = extensions::canonicalize_all();
                if n > 0 {
                    eprintln!("[launcher] extensions: {n} renamed/merged to canonical ids");
                }
            });

            // Keep profiles in step with the team server without any click.
            tauri::async_runtime::spawn(cloud_sync::run_forever());

            // Trash older than its week.
            match trash::purge_expired() {
                Ok(n) if n > 0 => eprintln!("[launcher] trash: {n} expired profile(s) removed"),
                Ok(_) => {}
                Err(e) => eprintln!("[launcher] trash sweep failed: {e}"),
            }

            // Clean up temporary profiles from crashed runs.
            match profile::purge_temporary() {
                Ok(n) if n > 0 => eprintln!("[launcher] purged {n} stale temporary profile(s)"),
                Ok(_) => {}
                Err(e) => eprintln!("[launcher] temporary purge failed: {e}"),
            }

            // API task on the shared tokio runtime.
            match settings::ensure_secret() {
                Ok(s) if s.api_enabled => {
                    let (secret, port) = (s.api_secret.clone(), s.api_port);
                    tauri::async_runtime::spawn(async move {
                        api::serve(secret, port).await;
                    });
                }
                Ok(_) => eprintln!("[launcher] automation API disabled in settings"),
                Err(e) => eprintln!("[launcher] API secret init failed: {e}"),
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            match event {
                // Cmd+Q / Dock "Quit" / OS shutdown — refuse while a profile
                // is running, same as the tray's Quit item, so this can't
                // bypass Team Sync checkin. Only prevent when we're actually
                // going to block: `api.prevent_exit()` here is unconditional,
                // and this handler's own `app.exit(0)` (the "nothing running"
                // path in `quit_gracefully`) re-enters this same event —
                // preventing that one too would loop forever instead of exiting.
                tauri::RunEvent::ExitRequested { api, .. } => {
                    if !process::Tracker::shared().running().is_empty() {
                        api.prevent_exit();
                        quit_gracefully(app_handle);
                    }
                }
                // Clicking the Dock icon after the window was closed to tray:
                // `.hide()` leaves no visible window, and macOS otherwise does
                // nothing on its own in that case.
                #[cfg(target_os = "macos")]
                tauri::RunEvent::Reopen { .. } => show_main_window(app_handle),
                _ => {}
            }
        });
}

#[cfg(test)]
mod bulk_file_tests {
    use super::*;

    fn dump(rows: &[BulkParseRow]) -> String {
        rows.iter()
            .map(|r| format!("{}|{}|{}|{}|{}|{}|{:?}", r.row, r.name, r.folder, r.notes, r.proxy, r.color, r.error))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn csv_semicolon_bom_and_quoted_delimiters() {
        let text = "\u{feff}name;folder;notes;proxy;color\nCSV 01;Ads;ghi chú;socks5://user:pass@1.2.3.4:1080;#8b5cf6\nCSV 02;;;;\nCSV 03;X;\"có ; và , trong ghi chú\";;\n";
        let rows = parse_csv_rows(text);
        assert_eq!(rows.len(), 3, "{}", dump(&rows));
        assert_eq!(rows[0].proxy, "socks5://user:pass@1.2.3.4:1080");
        assert_eq!(rows[1].name, "CSV 02");
        assert_eq!(rows[1].error, None);
        assert_eq!(rows[2].notes, "có ; và , trong ghi chú");
    }

    #[test]
    fn xml_entities_decode() {
        assert_eq!(xml_unescape("ghi ch&#250; ti&#7871;ng &amp; &#x110;&lt;3"), "ghi chú tiếng & Đ<3");
        assert_eq!(xml_unescape("a & b &unknown; c"), "a & b &unknown; c");
    }

    #[test]
    fn csv_comma_still_works() {
        let rows = parse_csv_rows("name,folder,notes,proxy,color\nFB 01,Ads,via US,http://5.6.7.8:8080,#22c55e\n");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].folder, "Ads");
        assert_eq!(rows[0].error, None);
    }

    /// The Excel template offered for download must read back through the importer.
    #[test]
    fn the_downloadable_template_parses() {
        let path = std::env::temp_dir().join(format!("hir-template-{}.xlsx", uuid::Uuid::new_v4()));
        std::fs::write(&path, build_bulk_template_xlsx().unwrap()).unwrap();
        let rows = parse_xlsx_rows(&path).expect("template parses");
        let _ = std::fs::remove_file(&path);
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0].name.as_str(), rows[0].folder.as_str(), rows[0].color.as_str()), ("FB 01", "Shop A", "#8b5cf6"));
        assert_eq!(rows[0].kind, "socks5");
        assert_eq!(rows[1].notes, "");
        assert_eq!(rows[1].proxy, "1.2.3.4:8080:user:pass");
        assert_eq!(rows[1].kind, "http");
        assert!(rows.iter().all(|r| r.error.is_none()), "{:?}", rows.iter().map(|r| &r.error).collect::<Vec<_>>());
    }

    /// Runs against real Excel-style files when HIR_TEST_XLSX names one.
    #[test]
    fn real_xlsx_file() {
        let Ok(path) = std::env::var("HIR_TEST_XLSX") else { return };
        let rows = parse_xlsx_rows(std::path::Path::new(&path)).expect("parse");
        println!("{}\n", dump(&rows));
    }

    /// Creates profiles from parsed rows inside a throwaway data root (no proxy
    /// rows: those would write the real proxies.json).
    #[test]
    fn bulk_create_makes_profiles() {
        let _g = cloud_sync::TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = std::env::temp_dir().join(format!("hir-bulk-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        store::set_data_root(Some(tmp.clone()));
        let rows = vec![
            BulkRow { name: "Bulk A".into(), folder: "Ads".into(), notes: "n1".into(), proxy: String::new(), color: "#8b5cf6".into(), kind: String::new() },
            BulkRow { name: "Bulk B".into(), folder: String::new(), notes: String::new(), proxy: String::new(), color: "22c55e".into(), kind: String::new() },
            BulkRow { name: "Bulk C".into(), folder: String::new(), notes: String::new(), proxy: String::new(), color: "zzz".into(), kind: String::new() },
            BulkRow { name: "  ".into(), folder: String::new(), notes: String::new(), proxy: String::new(), color: String::new(), kind: String::new() },
        ];
        let res = profile_bulk_create(rows).expect("create");
        let summary: Vec<String> = res.iter().map(|r| format!("{}:{}:{:?}", r.index, r.ok, r.error)).collect();
        println!("{}", summary.join("\n"));
        let list = profile::list_all().unwrap();
        let mut names: Vec<(String, String, String)> = list.iter().map(|p| (p.name.clone(), p.folder.clone(), p.color.clone().unwrap_or_default())).collect();
        names.sort();
        println!("{names:?}");
        store::set_data_root(None);
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(res[0].ok && res[1].ok && !res[2].ok && !res[3].ok, "{summary:?}");
        assert_eq!(list.len(), 2);
        assert!(names.contains(&("Bulk A".into(), "Ads".into(), "#8b5cf6".into())));
        assert!(names.contains(&("Bulk B".into(), String::new(), "#22c55e".into())));
    }
}
