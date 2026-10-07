use std::process::Command;
use std::sync::{Mutex, OnceLock};

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

fn tailscale_bin_cache() -> &'static Mutex<Option<String>> {
    static CELL: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(None))
}

/// Resolving this spawns a `tailscale version` (or, missing from PATH, several
/// candidate paths each probed the same way) just to find the binary — and
/// every one of `is_installed`/`is_connected`/`tailscale_ip` used to do that
/// resolution over again on its own, so one visit to the "Đồng bộ nhóm" tab
/// fired it half a dozen times back to back, each a real process spawn. The
/// path doesn't move during a run, so once found it's kept; "not found" isn't
/// cached, so installing Tailscale while the app is open is still picked up on
/// the next check instead of needing a restart.
fn tailscale_bin() -> Option<String> {
    if let Some(p) = tailscale_bin_cache().lock().unwrap_or_else(|e| e.into_inner()).clone() {
        return Some(p);
    }
    let found = tailscale_bin_uncached();
    if let Some(p) = &found {
        *tailscale_bin_cache().lock().unwrap_or_else(|e| e.into_inner()) = Some(p.clone());
    }
    found
}

fn tailscale_bin_uncached() -> Option<String> {
    // On a Mac the app's own binary comes before whatever `tailscale` is on PATH: a lone CLI
    // from Homebrew has no daemon behind it, so `up` through it never starts the app's tunnel.
    #[cfg(target_os = "macos")]
    {
        let app = "/Applications/Tailscale.app/Contents/MacOS/Tailscale";
        if std::path::Path::new(app).exists() {
            return Some(app.into());
        }
    }
    // Try PATH first
    if cmd("tailscale").arg("version").output().map(|o| o.status.success()).unwrap_or(false) {
        return Some("tailscale".into());
    }
    let home_app = dirs::home_dir().map(|h| h.join("Applications/Tailscale.app/Contents/MacOS/Tailscale").to_string_lossy().into_owned());
    let candidates = [
        "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
        "/opt/homebrew/bin/tailscale",
        "/usr/local/bin/tailscale",
        "C:\\Program Files\\Tailscale\\tailscale.exe",
    ]
    .map(String::from)
    .into_iter()
    .chain(home_app);
    for p in candidates {
        let p = p.as_str();
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

/// The one subprocess call `is_connected`/`tailscale_ip` both need — doing it
/// once and deriving both from the result, instead of each running its own
/// `tailscale ip -4`, is the other half of the "Đồng bộ nhóm tab thấy lag" fix
/// (the cached binary path above is the first half).
fn probe_ip() -> Option<String> {
    let bin = tailscale_bin()?;
    let out = cmd(&bin).args(["ip", "-4"]).output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.starts_with("100.") { Some(s) } else { None }
}

pub fn is_installed() -> bool {
    tailscale_bin().is_some() || has_100_ip()
}

pub fn is_connected() -> bool {
    probe_ip().is_some() || has_100_ip()
}

pub fn tailscale_ip() -> Option<String> {
    probe_ip().or_else(|| {
        #[cfg(unix)]
        {
            if let Ok(out) = Command::new("sh").arg("-c").arg("ifconfig 2>/dev/null | grep -o '100\\.[0-9]*\\.[0-9]*\\.[0-9]*' | head -1").output() {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !s.is_empty() { return Some(s); }
            }
        }
        None
    })
}

/// Everything the "Đồng bộ nhóm" tab's status check needs, from a single probe
/// instead of `is_installed`+`is_connected`+`tailscale_ip` each resolving the
/// binary and shelling out on their own.
pub fn status() -> (bool, bool, Option<String>) {
    let ip = tailscale_ip();
    let installed = ip.is_some() || tailscale_bin().is_some() || has_100_ip();
    let connected = ip.is_some();
    (installed, connected, ip)
}

/// Try to join tailnet using a reusable auth key. Returns Ok(()) on success or already connected.
pub fn join_with_auth_key(auth_key: &str) -> anyhow::Result<()> {
    if is_connected() {
        return Ok(());
    }
    up_with_auth_key(auth_key, false)
}

/// Moves this machine onto the tailnet the key belongs to, even when it is signed in to another
/// one. A machine is in one tailnet at a time, so a Mac that already has its owner's Tailscale
/// account never reaches a team server in a different tailnet until it is re-authenticated.
pub fn switch_with_auth_key(auth_key: &str) -> anyhow::Result<()> {
    up_with_auth_key(auth_key, true)
}

fn up_with_auth_key(auth_key: &str, force_reauth: bool) -> anyhow::Result<()> {
    if auth_key.trim().is_empty() {
        anyhow::bail!("auth key trống");
    }
    let bin = tailscale_bin().ok_or_else(|| anyhow::anyhow!("chưa cài Tailscale — tải tại https://tailscale.com/download"))?;
    // `--reset`: a machine that was `up` before with other flags refuses a bare `up` ("requires
    // mentioning all non-default flags"). `--timeout`: without it `up` waits forever when the
    // Tailscale app is not running or wants an interactive sign-in, and the join hangs.
    let mut args = vec!["up", "--authkey", auth_key.trim(), "--reset", "--timeout=30s"];
    if force_reauth {
        args.push("--force-reauth");
    }
    let out = run_with_timeout(&bin, &args, std::time::Duration::from_secs(45))
        .map_err(|e| anyhow::anyhow!("không chạy được tailscale: {e}"))?;
    // Kept even when `up` says it succeeded: "succeeded" with no address afterwards is the case
    // that needs the raw words to be understood. The key itself is never part of it.
    remember_up(&out);
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let out_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
        anyhow::bail!("{}", explain_up_error(if err.is_empty() { &out_str } else { &err }));
    }
    Ok(())
}

fn last_up() -> &'static Mutex<String> {
    static CELL: OnceLock<Mutex<String>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(String::new()))
}

fn remember_up(out: &std::process::Output) {
    let said = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let one_line: String = said.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(240).collect();
    let code = out.status.code().map(|c| c.to_string()).unwrap_or_else(|| "?".into());
    *last_up().lock().unwrap_or_else(|e| e.into_inner()) = format!("up thoát mã {code}: {}", if one_line.is_empty() { "(không in gì)" } else { &one_line });
}

/// What a failed join leaves to go on: which binary was used and what it says right now. Short
/// enough to sit in a toast and be read out; it is what turns "nothing happened" into a cause.
pub fn diagnose() -> String {
    let Some(bin) = tailscale_bin() else {
        return "không tìm thấy chương trình tailscale".into();
    };
    let said = run_with_timeout(&bin, &["status"], std::time::Duration::from_secs(8))
        .map(|o| {
            let out = String::from_utf8_lossy(&o.stdout).trim().to_string();
            let err = String::from_utf8_lossy(&o.stderr).trim().to_string();
            if out.is_empty() { err } else { out }
        })
        .unwrap_or_else(|e| e.to_string());
    let said: String = said.lines().take(3).collect::<Vec<_>>().join(" | ");
    let said: String = said.chars().take(220).collect();
    let up = last_up().lock().unwrap_or_else(|e| e.into_inner()).clone();
    format!("dùng {bin}; {up}; trạng thái: {}", if said.is_empty() { "(trống)" } else { &said })
}

/// Whether `ip` is this machine or one of its peers in the tailnet it is signed in to — `None`
/// when that cannot be read. Offline peers still count: they are known, just switched off. An
/// address nobody here owns belongs to a different tailnet (or to no one).
pub fn knows_address(ip: &str) -> Option<bool> {
    let bin = tailscale_bin()?;
    let out = run_with_timeout(&bin, &["status", "--json"], std::time::Duration::from_secs(10)).ok()?;
    if !out.status.success() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    Some(addresses_in_status(&v).iter().any(|a| a == ip))
}

fn addresses_in_status(v: &serde_json::Value) -> Vec<String> {
    let ips = |n: &serde_json::Value| -> Vec<String> {
        n.get("TailscaleIPs").and_then(|a| a.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default()
    };
    let mut all = v.get("Self").map(ips).unwrap_or_default();
    if let Some(peers) = v.get("Peer").and_then(|p| p.as_object()) {
        for p in peers.values() {
            all.extend(ips(p));
        }
    }
    all
}

/// `Command::output` with a ceiling: the child is killed when it outlives `limit`.
fn run_with_timeout(program: &str, args: &[&str], limit: std::time::Duration) -> std::io::Result<std::process::Output> {
    use std::io::Read;
    use std::process::Stdio;
    let mut child = cmd(program).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    // Drained on their own threads so a chatty child cannot fill a pipe and stall.
    let mut out_pipe = child.stdout.take();
    let mut err_pipe = child.stderr.take();
    let out_t = std::thread::spawn(move || { let mut b = Vec::new(); if let Some(p) = out_pipe.as_mut() { let _ = p.read_to_end(&mut b); } b });
    let err_t = std::thread::spawn(move || { let mut b = Vec::new(); if let Some(p) = err_pipe.as_mut() { let _ = p.read_to_end(&mut b); } b });
    let started = std::time::Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if started.elapsed() >= limit {
            let _ = child.kill();
            let _ = child.wait();
            let _ = out_t.join();
            let _ = err_t.join();
            return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, format!("tailscale không trả lời sau {} giây", limit.as_secs())));
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
    };
    Ok(std::process::Output { status, stdout: out_t.join().unwrap_or_default(), stderr: err_t.join().unwrap_or_default() })
}

/// What `tailscale up` said, with a plain-language line in front when the cause is a
/// known one. The raw text stays after it: it is what the person reads out when asking for help.
fn explain_up_error(raw: &str) -> String {
    let low = raw.to_lowercase();
    let hint = if low.contains("failed to load preferences")
        || low.contains("failed to connect to local")
        || low.contains("doesn't appear to be running")
        || low.contains("is stopped")
        || low.contains("not running")
        || low.contains("tailscaled")
    {
        Some("Ứng dụng Tailscale trên máy chưa chạy hoặc chưa bật — mở Tailscale (biểu tượng trên thanh menu / khay hệ thống), bật nó lên rồi thử lại")
    } else if low.contains("requires mentioning all non-default flags") {
        Some("Tailscale đang giữ cấu hình cũ — mở Tailscale và Log out, rồi thử lại")
    } else if low.contains("timed out") || low.contains("timeout") || low.contains("context deadline") {
        Some("Tailscale không vào được mạng của team (hết thời gian chờ) — kiểm tra Internet, hoặc mã mời đã hết hạn thì xin mã mới")
    } else if low.contains("invalid key") || low.contains("key not valid") || low.contains("expired") {
        Some("Mã mời (auth key) không hợp lệ hoặc đã hết hạn — xin quản trị viên tạo mã mời mới")
    } else {
        None
    };
    match hint {
        Some(h) if !raw.is_empty() => format!("{h}. ({raw})"),
        Some(h) => h.to_string(),
        None => raw.to_string(),
    }
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

/// Tailscale's key `description` rejects punctuation like `:` and `/` (and
/// presumably anything outside ASCII) — hit in practice via a Vietnamese-dated
/// label ("27/9/2026") and a "Name: person" label. Kept conservative (letters,
/// digits, space, hyphen, underscore) rather than guessing the exact allowed
/// set, so a name with accents or any other odd character never breaks this again.
fn sanitize_description(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_was_space = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            out.push(c);
            last_was_space = false;
        } else if !last_was_space {
            out.push(' ');
            last_was_space = true;
        }
    }
    let trimmed = out.trim().to_string();
    if trimmed.is_empty() { "Hir-Login".to_string() } else { trimmed }
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
    let description = sanitize_description(description);

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

/// Whether this OAuth Client's tailnet is the one this machine is actually on
/// — `None` when this machine isn't connected to any tailnet right now, so
/// there's nothing to compare against. A client from a *different* tailnet
/// still mints real, working keys; they just join whoever uses them to that
/// other network, unreachable from this one — exactly the "mã mời không kết
/// nối được" confusion a mismatched OAuth Client causes, caught here at the
/// moment it's pasted in rather than days later when a new hire's join fails.
pub async fn oauth_matches_this_host(client_id: &str, client_secret: &str) -> anyhow::Result<Option<bool>> {
    let Some(my_ip) = tailscale_ip() else { return Ok(None) };
    let client = http_client()?;
    let access_token = oauth_token(&client, client_id, client_secret).await?;
    #[derive(serde::Deserialize)]
    struct Device {
        #[serde(default)]
        addresses: Vec<String>,
    }
    #[derive(serde::Deserialize)]
    struct DevicesResp {
        #[serde(default)]
        devices: Vec<Device>,
    }
    let resp = client
        .get("https://api.tailscale.com/api/v2/tailnet/-/devices")
        .bearer_auth(&access_token)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("không lấy được danh sách máy trong mạng: {e}"))?;
    let resp = ok_body(resp).await?;
    let resp: DevicesResp = resp.json().await.map_err(|e| anyhow::anyhow!("phản hồi danh sách máy không đọc được: {e}"))?;
    Ok(Some(resp.devices.iter().any(|d| d.addresses.iter().any(|a| a == &my_ip))))
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
mod bin_cache_tests {
    use super::*;

    /// The getter must return a cached path without re-probing — tested by
    /// seeding the cache directly rather than depending on this machine (or a
    /// CI runner, which never has it) actually having Tailscale installed.
    #[test]
    fn a_resolved_binary_path_is_cached_and_reused() {
        let cell = tailscale_bin_cache();
        let original = cell.lock().unwrap_or_else(|e| e.into_inner()).clone();
        *cell.lock().unwrap_or_else(|e| e.into_inner()) = Some("/fake/tailscale".into());

        assert_eq!(tailscale_bin().as_deref(), Some("/fake/tailscale"), "a cached hit must come back as-is, with no real probe");

        *cell.lock().unwrap_or_else(|e| e.into_inner()) = original;
    }

    /// `connected` and `ip` must agree, and a connected result must always
    /// imply `installed` — holds on any machine, Tailscale or not, since it's
    /// a property of how `status()` derives its three fields from one probe.
    #[test]
    fn status_fields_are_internally_consistent() {
        let (installed, connected, ip) = status();
        assert_eq!(connected, ip.is_some(), "connected must come straight from whether the probe found an IP");
        if connected {
            assert!(installed, "a live 100.x IP means Tailscale is obviously installed");
        }
    }
}

#[cfg(test)]
mod oauth_tests {
    use super::*;

    /// With no live Tailscale connection on this machine, there's nothing to
    /// compare the OAuth Client's tailnet against — answered `None` without
    /// even spending a network call on the (possibly bogus) credentials, not
    /// misread as "mismatch". True on any CI runner (never has Tailscale) and,
    /// right now, true on a dev machine mid-way through testing a join too.
    #[tokio::test]
    async fn no_local_tailscale_connection_is_unknown_not_a_mismatch() {
        if tailscale_ip().is_some() {
            eprintln!("skipped: this machine is currently connected to Tailscale");
            return;
        }
        let got = oauth_matches_this_host("whatever-id", "whatever-secret").await.unwrap();
        assert_eq!(got, None);
    }

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
    fn description_strips_characters_tailscale_rejects() {
        // Hit in practice: a Vietnamese-locale date ("27/9/2026") and a "Name: person" label.
        assert_eq!(sanitize_description("Hir-Login admin device 27/9/2026"), "Hir-Login admin device 27 9 2026");
        assert_eq!(sanitize_description("Hir-Login: Huyền"), "Hir-Login Huy n");
        assert_eq!(sanitize_description("plain-name_ok-123"), "plain-name_ok-123");
        assert_eq!(sanitize_description("   "), "Hir-Login");
        assert_eq!(sanitize_description(""), "Hir-Login");
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

    #[test]
    fn a_stopped_app_and_a_timeout_get_a_plain_line_in_front_of_the_raw_text() {
        let stopped = explain_up_error("Error: The Tailscale CLI failed to start: Failed to load preferences.");
        assert!(stopped.starts_with("Ứng dụng Tailscale trên máy chưa chạy"), "{stopped}");
        assert!(stopped.contains("Failed to load preferences"), "the raw text stays: {stopped}");
        assert!(explain_up_error("context deadline exceeded").contains("hết thời gian chờ"));
        assert_eq!(explain_up_error("something unheard of"), "something unheard of");
        assert_eq!(explain_up_error(""), "");
    }

    #[test]
    fn status_addresses_cover_this_machine_and_offline_peers_only() {
        let v: serde_json::Value = serde_json::from_str(r#"{
            "Self": {"TailscaleIPs": ["100.121.218.2", "fd7a:115c:a1e0::1"]},
            "Peer": {
                "nodekey:a": {"TailscaleIPs": ["100.83.136.52"], "Online": false},
                "nodekey:b": {"TailscaleIPs": ["100.126.170.5"], "Online": false}
            }}"#).unwrap();
        let all = addresses_in_status(&v);
        assert!(all.iter().any(|a| a == "100.121.218.2"), "this machine");
        assert!(all.iter().any(|a| a == "100.83.136.52"), "an offline peer is still known");
        assert!(!all.iter().any(|a| a == "100.93.82.19"), "an address of another tailnet is not");
        assert!(addresses_in_status(&serde_json::json!({})).is_empty(), "no Self, no Peer: nothing known");
    }
}
