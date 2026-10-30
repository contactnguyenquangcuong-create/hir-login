//! Tells the company's central server (`web-quan-tri/`, Worker `hirlogin-web`) what this machine is
//! doing, so the admin sees every machine in one place whichever team server it uses. This is for
//! management only: no profile data travels, and this has nothing to do with team sync.
//!
//! Sent: this machine's name and version, how many profiles it has, which of them are open (name,
//! folder, for how long), which automation runs are going and their counts, the team server it is
//! on. Never: the list of its other profiles, pages visited, cookies, passwords, proxy details.
//!
//! Identified by the license key and machine id, which the server checks with the key server. A
//! failure here is only retried at the next beat: nothing in the app waits on it.

use crate::{cloud_sync, license, process, profile, settings};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

const REPORT_URL: &str = "https://hirlogin-web.hir-contact-vn.workers.dev/api/bao-cao";
/// How often to report when nothing changes; the server may ask for another pace in its answer.
const DEFAULT_BEAT_SECS: u64 = 180;
const WATCH_EVERY: Duration = Duration::from_secs(5);
/// A profile opened or closed is reported at once, but no more often than this.
const MIN_GAP: Duration = Duration::from_secs(10);
/// The key is re-checked when the server says it changed, at most this often.
const RECHECK_GAP: Duration = Duration::from_secs(1800);

#[derive(Debug, Clone, Serialize, Default)]
pub(crate) struct OpenProfile {
    id: String,
    ten: String,
    thu_muc: String,
    giay: u64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub(crate) struct Run {
    du_an: String,
    luong: u32,
    dong_ok: u32,
    dong_loi: u32,
    tu_luc: u64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub(crate) struct Report {
    key: String,
    ma_may: String,
    ten_nguoi: String,
    ten_may: String,
    phien_ban: String,
    he_dieu_hanh: String,
    may_chu: String,
    tong_profile: usize,
    cho_tai_len: usize,
    dang_mo: Vec<OpenProfile>,
    tu_dong: Vec<Run>,
}

#[derive(Debug, Deserialize, Default, PartialEq)]
pub(crate) struct Reply {
    #[serde(default)]
    sau_giay: Option<u64>,
    #[serde(default)]
    key_loi: Option<String>,
}

/// Host and port only: the address alone tells the admin which team server this is.
fn server_label(url: &str) -> String {
    url.trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/').chars().take(120).collect()
}

fn running_runs() -> Vec<Run> {
    #[cfg(feature = "automation")]
    return crate::runner::all()
        .into_iter()
        .filter(|r| r.running)
        .map(|r| Run { du_an: r.project_name, luong: r.workers.len() as u32, dong_ok: r.rows_ok, dong_loi: r.rows_failed, tu_luc: r.started_at })
        .collect();
    #[cfg(not(feature = "automation"))]
    Vec::new()
}

/// What changes the admin should see at once: the set of open profiles and of running projects.
fn activity_signature() -> Vec<String> {
    let mut s: Vec<String> = process::Tracker::shared().running().into_iter().map(|r| r.profile_id).collect();
    s.extend(running_runs().into_iter().map(|r| format!("run:{}", r.du_an)));
    s.sort();
    s
}

fn build_report(key: String, machine_id: String, person: String) -> Report {
    let metas = profile::list_all().unwrap_or_default();
    let dang_mo = process::Tracker::shared()
        .running()
        .into_iter()
        .map(|r| {
            let m = metas.iter().find(|m| m.id == r.profile_id);
            OpenProfile {
                ten: m.map(|m| m.name.clone()).unwrap_or_default(),
                thu_muc: m.map(|m| m.folder.clone()).unwrap_or_default(),
                id: r.profile_id,
                giay: r.uptime_ms / 1000,
            }
        })
        .collect();
    let sync = settings::load().map(|s| s.sync).unwrap_or_default();
    let may_chu = if sync.enabled { sync.server_url.as_deref().map(server_label).unwrap_or_default() } else { String::new() };
    Report {
        key,
        ma_may: machine_id,
        ten_nguoi: person,
        ten_may: cloud_sync::device_name(&sync),
        phien_ban: env!("CARGO_PKG_VERSION").to_string(),
        he_dieu_hanh: std::env::consts::OS.to_string(),
        may_chu,
        tong_profile: metas.len(),
        cho_tai_len: cloud_sync::pending_count(),
        dang_mo,
        tu_dong: running_runs(),
    }
}

fn http() -> reqwest::Client {
    reqwest::Client::builder().timeout(Duration::from_secs(25)).build().unwrap_or_default()
}

/// The server's answer, for a refusal as well: 403 carries what is wrong with the key.
pub(crate) async fn send(client: &reqwest::Client, url: &str, report: &Report) -> anyhow::Result<Reply> {
    let resp = client.post(url).json(report).send().await?;
    let refused = resp.status().as_u16() == 403;
    if !refused && !resp.status().is_success() {
        anyhow::bail!("server answered {}", resp.status());
    }
    let mut reply: Reply = resp.json().await.unwrap_or_default();
    if refused && reply.key_loi.is_none() {
        reply.key_loi = Some("KHONG_XAC_THUC".into());
    }
    Ok(reply)
}

fn report_url() -> String {
    #[cfg(debug_assertions)]
    if let Ok(u) = std::env::var("HIR_LOGIN_REPORT_URL") {
        return u;
    }
    REPORT_URL.to_string()
}

/// The server's pace, within limits a bad answer cannot push it out of.
fn beat(reply: &Reply) -> Duration {
    Duration::from_secs(reply.sau_giay.unwrap_or(DEFAULT_BEAT_SECS).clamp(60, 3600))
}

pub async fn run_forever() {
    tokio::time::sleep(Duration::from_secs(8)).await;
    let (client, url) = (http(), report_url());
    let mut pace = Duration::from_secs(DEFAULT_BEAT_SECS);
    let mut last_recheck: Option<Instant> = None;
    loop {
        let Some((key, machine_id, person)) = license::report_identity() else {
            tokio::time::sleep(Duration::from_secs(60)).await;
            continue;
        };
        let signature = activity_signature();
        let built = tokio::task::spawn_blocking(move || build_report(key, machine_id, person)).await;
        if let Ok(report) = built {
            match send(&client, &url, &report).await {
                Ok(reply) => {
                    pace = beat(&reply);
                    if reply.key_loi.is_some() && last_recheck.is_none_or(|t| t.elapsed() >= RECHECK_GAP) {
                        last_recheck = Some(Instant::now());
                        license::recheck_now().await;
                    }
                }
                Err(e) => eprintln!("[fleet-report] not sent: {e:#}"),
            }
        }
        let sent_at = Instant::now();
        loop {
            tokio::time::sleep(WATCH_EVERY).await;
            let elapsed = sent_at.elapsed();
            if elapsed >= pace || (elapsed >= MIN_GAP && activity_signature() != signature) {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{extract::State, http::StatusCode, routing::post as axum_post, Json, Router};
    use std::sync::{Arc, Mutex};

    type Seen = Arc<Mutex<Vec<serde_json::Value>>>;

    /// A server that answers with (status, body) and keeps what it was sent.
    async fn server(code: u16, reply: serde_json::Value) -> (String, Seen) {
        let seen: Seen = Arc::default();
        let app = Router::new().route(
            "/r",
            axum_post(|State((seen, code, reply)): State<(Seen, u16, serde_json::Value)>, Json(body): Json<serde_json::Value>| async move {
                seen.lock().unwrap().push(body);
                (StatusCode::from_u16(code).unwrap(), Json(reply))
            }),
        ).with_state((seen.clone(), code, reply));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, app).await.ok() });
        (format!("http://127.0.0.1:{port}/r"), seen)
    }

    #[tokio::test]
    async fn a_report_goes_up_once_and_the_server_sets_the_pace() {
        let (url, seen) = server(200, serde_json::json!({"ok": true, "sau_giay": 300})).await;
        let r = send(&http(), &url, &Report { ten_may: "PC".into(), tong_profile: 7, ..Default::default() }).await.unwrap();
        assert_eq!(r.sau_giay, Some(300));
        let sent = seen.lock().unwrap().clone();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0]["ten_may"], "PC");
        assert_eq!(sent[0]["tong_profile"], 7, "how many, not which");
    }

    #[tokio::test]
    async fn a_refused_machine_learns_about_its_key_and_a_broken_server_is_only_an_error() {
        let (url, _) = server(403, serde_json::json!({"ok": false, "key_loi": "CHUA_KICH_HOAT"})).await;
        assert_eq!(send(&http(), &url, &Report::default()).await.unwrap().key_loi.as_deref(), Some("CHUA_KICH_HOAT"));
        let (url, _) = server(403, serde_json::json!({"ok": false})).await;
        assert_eq!(send(&http(), &url, &Report::default()).await.unwrap().key_loi.as_deref(), Some("KHONG_XAC_THUC"));
        let (url, _) = server(503, serde_json::json!({"ok": false})).await;
        assert!(send(&http(), &url, &Report::default()).await.is_err());
        // Nobody listening at all: an error to log, nothing more.
        assert!(send(&http(), "http://127.0.0.1:9/r", &Report::default()).await.is_err());
    }

    /// Management only: the report holds no profile data and no list of profiles.
    #[test]
    fn the_report_carries_no_profile_data() {
        let r = Report { key: "HLG-A".into(), dang_mo: vec![OpenProfile { id: "p".into(), ten: "n".into(), thu_muc: "f".into(), giay: 1 }], ..Default::default() };
        let v = serde_json::to_value(&r).unwrap();
        let fields: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        assert!(!fields.contains(&"profiles"), "{fields:?}");
        let text = v.to_string().to_lowercase();
        for word in ["proxy", "cookie", "password", "url", "token", "bundle"] {
            assert!(!text.contains(word), "{word} in {text}");
        }
    }

    #[test]
    fn the_pace_stays_within_limits_and_the_server_address_is_short() {
        assert_eq!(beat(&Reply::default()), Duration::from_secs(180));
        assert_eq!(beat(&Reply { sau_giay: Some(1), ..Default::default() }), Duration::from_secs(60));
        assert_eq!(beat(&Reply { sau_giay: Some(999_999), ..Default::default() }), Duration::from_secs(3600));
        assert_eq!(server_label("http://100.1.2.3:8787/"), "100.1.2.3:8787");
    }
}
