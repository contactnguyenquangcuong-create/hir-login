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
//! Only the debugging port is used, only on this machine's loopback, and only for this.

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
    let saved = crate::cookies::export(profile_id).context("read the saved cookies")?;
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
