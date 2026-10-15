//! Puts back, through the browser itself, the saved logins the browser did not take from its cookie file.
//!
//! On a Mac the browser can refuse cookies this launcher wrote into its file — it opens them with a
//! key of its own, and a value sealed with another one is skipped without a word — and the profile
//! then opens logged out although every cookie is in the file (seen: Google's login cookies
//! missing from a Mac browser that had them on Windows). The browser always accepts a cookie it is
//! handed over its debugging port: it seals it with its own key. So, a moment after the browser
//! starts, what the file holds is compared with what the browser has, and what it lacks is
//! handed over, after which the open pages are reloaded to see the login.
//!
//! Only the debugging port is used, only on this machine's loopback, and only for this. It runs for
//! every interactive profile on every system, so the logins reach the browser the same way
//! wherever the profile was last open.

use crate::cookies::Cookie;
use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::HashSet;
use tokio_tungstenite::tungstenite::Message;

/// A cookie as the browser's `Storage.setCookies` wants it.
fn to_param(c: &Cookie) -> Value {
    let mut p = json!({
        "name": c.name,
        "value": c.value,
        "domain": c.domain,
        "path": if c.path.is_empty() { "/" } else { c.path.as_str() },
        "secure": c.secure,
        "httpOnly": c.http_only,
    });
    if let Some(e) = c.expires.filter(|e| *e > 0.0 && !c.session) {
        p["expires"] = json!(e);
    }
    match c.same_site.as_deref().map(|s| s.to_ascii_lowercase()).as_deref() {
        Some("none") | Some("no_restriction") => p["sameSite"] = json!("None"),
        Some("lax") => p["sameSite"] = json!("Lax"),
        Some("strict") => p["sameSite"] = json!("Strict"),
        _ => {}
    }
    p
}

/// `domain|name|path` — what makes two cookies the same cookie. A leading dot on the domain is
/// not part of the identity (the browser reports `.google.com`, a file may hold `google.com`).
fn key(domain: &str, name: &str, path: &str) -> String {
    format!("{}|{}|{}", domain.trim_start_matches('.').to_ascii_lowercase(), name, if path.is_empty() { "/" } else { path })
}

/// The saved cookies the browser does not have. A cookie already expired, or with no value (this
/// machine could not open it either), is not worth handing over.
pub(crate) fn missing(saved: &[Cookie], browser: &[(String, String, String)], now: f64) -> Vec<Cookie> {
    let have: HashSet<String> = browser.iter().map(|(d, n, p)| key(d, n, p)).collect();
    saved
        .iter()
        .filter(|c| !c.value.is_empty())
        .filter(|c| c.session || c.expires.map_or(true, |e| e > now))
        .filter(|c| !have.contains(&key(&c.domain, &c.name, &c.path)))
        .cloned()
        .collect()
}

struct Conn {
    sink: futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
        Message,
    >,
    source: futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    >,
    next: u64,
}

impl Conn {
    async fn call(&mut self, method: &str, params: Value, session: Option<&str>) -> Result<Value> {
        self.next += 1;
        let id = self.next;
        let mut msg = json!({ "id": id, "method": method, "params": params });
        if let Some(s) = session {
            msg["sessionId"] = json!(s);
        }
        self.sink.send(Message::Text(msg.to_string())).await.context("send")?;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            let next = tokio::time::timeout_at(deadline, self.source.next())
                .await
                .map_err(|_| anyhow::anyhow!("{method}: no answer"))?
                .ok_or_else(|| anyhow::anyhow!("{method}: connection closed"))?
                .context("receive")?;
            let Message::Text(text) = next else { continue };
            let Ok(v) = serde_json::from_str::<Value>(&text) else { continue };
            if v.get("id").and_then(|x| x.as_u64()) != Some(id) {
                continue; // an event, or the answer to something else
            }
            if let Some(e) = v.get("error") {
                anyhow::bail!("{method}: {e}");
            }
            return Ok(v.get("result").cloned().unwrap_or(Value::Null));
        }
    }
}

/// Compares the cookies saved for the profile with the ones its running browser has, hands over
/// what is missing, and reloads the open pages if it handed anything over. `ws_url` is the
/// browser's own debugging address. Returns a line for the log.
pub async fn heal(profile_id: &str, ws_url: &str) -> Result<String> {
    // The browser holds its cookie file while it starts (on Windows, so that it cannot be opened
    // for a moment): ask again a few times before giving up.
    let mut saved = None;
    let mut last_err = None;
    for attempt in 0..6 {
        let id = profile_id.to_string();
        match tokio::task::spawn_blocking(move || crate::cookies::export(&id)).await {
            Ok(Ok(list)) => {
                saved = Some(list);
                break;
            }
            Ok(Err(e)) => last_err = Some(e),
            Err(e) => last_err = Some(anyhow::anyhow!("{e}")),
        }
        if attempt < 5 {
            tokio::time::sleep(std::time::Duration::from_millis(700)).await;
        }
    }
    let saved = match saved {
        Some(s) => s,
        None => return Err(last_err.unwrap_or_else(|| anyhow::anyhow!("unknown")).context("read the saved cookies")),
    };
    if saved.is_empty() {
        return Ok("no saved cookies to compare".into());
    }
    let (stream, _) = tokio_tungstenite::connect_async(ws_url).await.context("connect")?;
    let (sink, source) = stream.split();
    let mut c = Conn { sink, source, next: 0 };

    let have = c.call("Storage.getCookies", json!({}), None).await?;
    let browser: Vec<(String, String, String)> = have
        .get("cookies")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .map(|x| {
                    let s = |k: &str| x.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
                    (s("domain"), s("name"), s("path"))
                })
                .collect()
        })
        .unwrap_or_default();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    let lacking = missing(&saved, &browser, now);
    if lacking.is_empty() {
        return Ok(format!("the browser has all of the {} saved cookies", saved.len()));
    }
    let params: Vec<Value> = lacking.iter().map(to_param).collect();
    c.call("Storage.setCookies", json!({ "cookies": params }), None).await?;

    // The pages that opened before the cookies arrived show the logged-out site: reload them.
    let mut reloaded = 0;
    if let Ok(t) = c.call("Target.getTargets", json!({}), None).await {
        let pages: Vec<String> = t
            .get("targetInfos")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter(|x| x.get("type").and_then(|v| v.as_str()) == Some("page"))
                    .filter_map(|x| x.get("targetId").and_then(|v| v.as_str()).map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        for id in pages {
            let Ok(a) = c.call("Target.attachToTarget", json!({ "targetId": id, "flatten": true }), None).await else { continue };
            let Some(session) = a.get("sessionId").and_then(|v| v.as_str()).map(String::from) else { continue };
            if c.call("Page.reload", json!({}), Some(&session)).await.is_ok() {
                reloaded += 1;
            }
            let _ = c.call("Target.detachFromTarget", json!({ "sessionId": session }), None).await;
        }
    }
    let names: Vec<String> = lacking.iter().take(6).map(|c| format!("{}:{}", c.domain, c.name)).collect();
    Ok(format!(
        "the browser had {} of {} saved cookies — handed it the {} it lacked (e.g. {}), reloaded {} page(s)",
        saved.len() - lacking.len().min(saved.len()),
        saved.len(),
        lacking.len(),
        names.join(", "),
        reloaded,
    ))
}

/// A tab with nothing of the person's in it: a blank page, a new-tab page, or the engine's own
/// start page (`chrome://shardx/`). They piled up — one more each time the profile opened.
pub(crate) fn is_blank_tab(url: &str) -> bool {
    let u = url.trim().to_ascii_lowercase();
    u.is_empty()
        || u == "about:blank"
        || u.starts_with("chrome://shardx")
        || u.starts_with("chrome://newtab")
        || u.starts_with("chrome://new-tab-page")
        || u.starts_with("chrome-search://local-ntp")
}

/// Which tabs go, from (target id, url) of every open page: with any page of the person's open,
/// every blank one goes; with none, exactly one stays (`Some(id)` when it is not `about:blank`
/// yet and should be turned into one). Returns (to close, to blank).
pub(crate) fn plan_tab_cleanup(tabs: &[(String, String)]) -> (Vec<String>, Option<String>) {
    let blanks: Vec<&(String, String)> = tabs.iter().filter(|(_, u)| is_blank_tab(u)).collect();
    if blanks.len() < tabs.len() {
        return (blanks.iter().map(|(id, _)| id.clone()).collect(), None);
    }
    // Only blank ones: keep one, an about:blank if there is one.
    let keep = blanks.iter().find(|(_, u)| u.trim().eq_ignore_ascii_case("about:blank")).or(blanks.first());
    let Some((keep_id, keep_url)) = keep else { return (Vec::new(), None) };
    let close = blanks.iter().filter(|(id, _)| id != keep_id).map(|(id, _)| id.clone()).collect();
    let navigate = (!keep_url.trim().eq_ignore_ascii_case("about:blank")).then(|| keep_id.clone());
    (close, navigate)
}

/// Once the browser has brought back the last session: the tabs the person had open stay, and
/// nothing else — no blank tab, no new-tab page, no engine start page on top of them; a profile
/// with nothing to bring back opens on a single blank tab. Returns a line for the log.
pub async fn tidy_tabs(ws_url: &str) -> Result<String> {
    let (stream, _) = tokio_tungstenite::connect_async(ws_url).await.context("connect")?;
    let (sink, source) = stream.split();
    let mut c = Conn { sink, source, next: 0 };
    let pages = |v: &Value| -> Vec<(String, String)> {
        v.get("targetInfos").and_then(|x| x.as_array()).map(|a| {
            a.iter()
                .filter(|t| t.get("type").and_then(|x| x.as_str()) == Some("page"))
                .map(|t| {
                    let s = |k: &str| t.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
                    (s("targetId"), s("url"))
                })
                .collect()
        }).unwrap_or_default()
    };
    // The session comes back over a moment: wait until the list of tabs stops changing.
    let mut last: Vec<(String, String)> = Vec::new();
    for _ in 0..12 {
        let now = pages(&c.call("Target.getTargets", json!({}), None).await?);
        if !now.is_empty() && now == last {
            break;
        }
        last = now;
        tokio::time::sleep(std::time::Duration::from_millis(700)).await;
    }
    let (close, navigate) = plan_tab_cleanup(&last);
    if let Some(id) = &navigate {
        if let Ok(a) = c.call("Target.attachToTarget", json!({ "targetId": id, "flatten": true }), None).await {
            if let Some(session) = a.get("sessionId").and_then(|v| v.as_str()).map(String::from) {
                let _ = c.call("Page.navigate", json!({ "url": "about:blank" }), Some(&session)).await;
                let _ = c.call("Target.detachFromTarget", json!({ "sessionId": session }), None).await;
            }
        }
    }
    for id in &close {
        let _ = c.call("Target.closeTarget", json!({ "targetId": id }), None).await;
    }
    Ok(format!("{} tab(s) open, {} blank one(s) closed", last.len(), close.len()))
}

/// How long a cookie the site meant to last "until the browser closes" is kept instead: long
/// enough to cover weeks away from a profile, under the 400 days the browser itself allows.
const SESSION_KEPT_SECS: f64 = 180.0 * 86_400.0;

/// The session cookies in the browser's own list (`Storage.getCookies`), as `Storage.setCookies`
/// parameters with an expiry set. A site that logs a person in with a cookie that carries no expiry
/// loses that login the moment the browser closes or restarts and asks for it again — a profile
/// that is opened every few days is "a new device" every time. Another way of keeping such a login
/// is to give the cookie a date, which is what this does. Partitioned (third-party embed) cookies
/// are left alone.
pub(crate) fn persist_params(browser_cookies: &[Value], now: f64) -> Vec<Value> {
    browser_cookies
        .iter()
        .filter(|c| c.get("session").and_then(|v| v.as_bool()).unwrap_or(false))
        .filter(|c| c.get("partitionKey").map_or(true, |p| p.is_null()))
        .filter(|c| c.get("value").and_then(|v| v.as_str()).is_some_and(|v| !v.is_empty()))
        .filter_map(|c| {
            let s = |k: &str| c.get(k).and_then(|v| v.as_str()).map(String::from);
            let mut p = json!({
                "name": s("name")?,
                "value": s("value")?,
                "domain": s("domain")?,
                "path": s("path").filter(|p| !p.is_empty()).unwrap_or_else(|| "/".into()),
                "secure": c.get("secure").and_then(|v| v.as_bool()).unwrap_or(false),
                "httpOnly": c.get("httpOnly").and_then(|v| v.as_bool()).unwrap_or(false),
                "expires": (now + SESSION_KEPT_SECS).floor(),
            });
            for k in ["sameSite", "priority", "sourceScheme"] {
                if let Some(v) = c.get(k).filter(|v| v.is_string()) {
                    p[k] = v.clone();
                }
            }
            if let Some(port) = c.get("sourcePort").filter(|v| v.is_number()) {
                p["sourcePort"] = port.clone();
            }
            Some(p)
        })
        .collect()
}

/// One pass over a running browser: its session cookies get an expiry. Returns how many.
async fn persist_sessions_once(ws_url: &str) -> Result<usize> {
    let (stream, _) = tokio_tungstenite::connect_async(ws_url).await.context("connect")?;
    let (sink, source) = stream.split();
    let mut c = Conn { sink, source, next: 0 };
    let have = c.call("Storage.getCookies", json!({}), None).await?;
    let list = have.get("cookies").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    let params = persist_params(&list, now);
    if params.is_empty() {
        return Ok(0);
    }
    let n = params.len();
    c.call("Storage.setCookies", json!({ "cookies": params }), None).await?;
    Ok(n)
}

/// Keeps the logins that would end with the browser: for as long as the profile runs, once a
/// minute, every session cookie is given an expiry (see `persist_params`). A cookie the site sets
/// again as a session cookie is caught by the next pass, so at most a minute of a new login is
/// unprotected; the first pass waits a little for the browser to settle.
pub async fn keep_sessions(profile_id: String, ws_url: String) {
    tokio::time::sleep(std::time::Duration::from_secs(25)).await;
    let mut first = true;
    while crate::process::Tracker::shared().is_running(&profile_id) {
        match persist_sessions_once(&ws_url).await {
            Ok(n) if n > 0 && first => crate::cloud_sync::log_line(&format!("sessions {profile_id}: {n} session cookie(s) given an expiry")),
            Ok(_) => {}
            Err(e) => eprintln!("[heal] keep_sessions({profile_id}): {e:#}"),
        }
        first = false;
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cookie(domain: &str, name: &str, value: &str, expires: Option<f64>) -> Cookie {
        Cookie {
            domain: domain.into(),
            name: name.into(),
            value: value.into(),
            path: "/".into(),
            expires,
            session: expires.is_none(),
            secure: true,
            http_only: true,
            same_site: Some("None".into()),
        }
    }

    #[test]
    fn only_what_the_browser_lacks_is_handed_over() {
        let saved = vec![
            cookie(".google.com", "SID", "s", Some(2e9)),
            cookie("accounts.google.com", "LSID", "l", Some(2e9)),
            cookie("accounts.google.com", "__Host-1PLSID", "h", Some(2e9)),
            cookie(".google.com", "OLD", "o", Some(1.0)),
            cookie(".google.com", "EMPTY", "", Some(2e9)),
            cookie(".google.com", "SESSION", "x", None),
        ];
        // The browser has SID (reported with its leading dot) and nothing of accounts.google.com.
        let browser = vec![(".google.com".to_string(), "SID".to_string(), "/".to_string())];
        let lacking = missing(&saved, &browser, 1.7e9);
        let names: Vec<&str> = lacking.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["LSID", "__Host-1PLSID", "SESSION"], "expired and empty ones are left out");
        // A file that spells the domain without the dot is still the same cookie.
        let browser = vec![("google.com".to_string(), "SID".to_string(), "/".to_string())];
        assert!(!missing(&saved, &browser, 1.7e9).iter().any(|c| c.name == "SID"));
    }

    #[test]
    fn blank_and_engine_tabs_go_and_the_persons_pages_stay() {
        let t = |id: &str, url: &str| (id.to_string(), url.to_string());
        // The person's pages, with blank ones and the engine's start page among them.
        let (close, nav) = plan_tab_cleanup(&[
            t("1", "https://shopee.sg/"), t("2", "about:blank"), t("3", "chrome://shardx/"),
            t("4", "https://www.facebook.com/"), t("5", "chrome://new-tab-page/"), t("6", "chrome://newtab/"),
        ]);
        assert_eq!(close, vec!["2", "3", "5", "6"]);
        assert_eq!(nav, None, "no blank tab is kept when there is a real page");
        // Nothing of the person's: exactly one about:blank stays.
        let (close, nav) = plan_tab_cleanup(&[t("1", "chrome://shardx/"), t("2", "about:blank"), t("3", "about:blank")]);
        assert_eq!(close, vec!["1", "3"]);
        assert_eq!(nav, None, "the kept one is already about:blank");
        // Only the engine's page: it stays, turned into about:blank.
        let (close, nav) = plan_tab_cleanup(&[t("1", "chrome://shardx/")]);
        assert!(close.is_empty());
        assert_eq!(nav.as_deref(), Some("1"));
        // One real page alone: nothing to do.
        let (close, nav) = plan_tab_cleanup(&[t("1", "https://mail.google.com/")]);
        assert!(close.is_empty() && nav.is_none());
    }

    #[test]
    fn only_session_cookies_get_a_date_and_nothing_else_changes() {
        let list = vec![
            json!({"name":"sess","value":"v","domain":".site.com","path":"/","session":true,"secure":true,"httpOnly":true,"sameSite":"Lax","priority":"Medium","sourceScheme":"Secure","sourcePort":443}),
            json!({"name":"long","value":"v","domain":".site.com","path":"/","session":false,"expires":2e9}),
            json!({"name":"empty","value":"","domain":".site.com","path":"/","session":true}),
            json!({"name":"part","value":"v","domain":".x.com","path":"/","session":true,"partitionKey":{"topLevelSite":"https://a.com"}}),
        ];
        let out = persist_params(&list, 1.7e9);
        assert_eq!(out.len(), 1, "only the plain session cookie with a value");
        assert_eq!(out[0]["name"], "sess");
        assert_eq!(out[0]["expires"], (1.7e9 + SESSION_KEPT_SECS).floor());
        assert_eq!(out[0]["secure"], true);
        assert_eq!(out[0]["sameSite"], "Lax");
        assert_eq!(out[0]["domain"], ".site.com");
    }

    #[test]
    fn a_cookie_is_described_the_way_the_browser_expects() {
        let p = to_param(&cookie("accounts.google.com", "__Host-GAPS", "v", Some(2e9)));
        assert_eq!(p["domain"], "accounts.google.com", "a host-only cookie keeps its domain without a dot");
        assert_eq!(p["secure"], true);
        assert_eq!(p["sameSite"], "None");
        assert_eq!(p["expires"], 2e9);
        let s = to_param(&cookie(".google.com", "S", "v", None));
        assert!(s.get("expires").is_none(), "a session cookie has no expiry");
    }
}
