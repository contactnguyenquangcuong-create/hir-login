//! A tiny local forwarder that adds an HTTP proxy's login for the browser.
//!
//! The engine takes a proxy's credentials from `--proxy-server=scheme://user:pass@host:port`
//! and does not decode percent escapes. For a SOCKS5 proxy that is enough — the
//! text goes through as written, `=` included. For an HTTP proxy, a `=` (or `;` /
//! `,`) in the login is read as Chromium's per-scheme rule syntax
//! (`http=proxy;https=proxy`), the whole setting is dropped, and the browser
//! connects directly: no proxy at all, silently. Proxy providers that hand out
//! base64-looking passwords ending in `==` hit exactly this.
//!
//! So for that one case the browser is pointed at `http://127.0.0.1:<port>` with
//! no login in it, and this forwarder — bound to loopback only, one per profile —
//! adds `Proxy-Authorization` and passes the traffic on to the real proxy.

use crate::proxy::{ProxyEntry, ProxyKind};
use anyhow::{Context, Result};
use base64::Engine as _;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

fn running() -> &'static Mutex<HashMap<String, JoinHandle<()>>> {
    static M: OnceLock<Mutex<HashMap<String, JoinHandle<()>>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Login characters the engine's `--proxy-server` parsing cannot carry for an HTTP proxy.
fn breaks_engine_arg(s: &str) -> bool {
    s.chars().any(|c| matches!(c, '=' | ';' | ','))
}

/// True when this proxy has to go through the local forwarder.
pub fn needs_relay(p: &ProxyEntry) -> bool {
    matches!(p.kind, ProxyKind::Http) && (breaks_engine_arg(&p.username) || breaks_engine_arg(&p.password))
}

/// An `https://` proxy (TLS to the proxy itself) with such a login has no safe
/// way through, and handing it over as-is would mean no proxy at all.
pub fn cannot_carry(p: &ProxyEntry) -> bool {
    matches!(p.kind, ProxyKind::Https) && (breaks_engine_arg(&p.username) || breaks_engine_arg(&p.password))
}

/// Starts the forwarder for `profile_id` (replacing any earlier one) and returns
/// the loopback port the browser should use as its HTTP proxy.
pub async fn start(profile_id: &str, p: &ProxyEntry) -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").await.context("bind local proxy relay")?;
    let port = listener.local_addr()?.port();
    let upstream = format!("{}:{}", p.host, p.port);
    let auth = base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", p.username, p.password));
    let task = tokio::spawn(async move {
        loop {
            let Ok((conn, _)) = listener.accept().await else { break };
            let (upstream, auth) = (upstream.clone(), auth.clone());
            tokio::spawn(async move {
                let _ = handle(conn, &upstream, &auth).await;
            });
        }
    });
    if let Ok(mut m) = running().lock() {
        if let Some(old) = m.insert(profile_id.to_string(), task) {
            old.abort();
        }
    }
    Ok(port)
}

/// Stops the forwarder when its profile's browser has exited.
pub fn stop(profile_id: &str) {
    if let Ok(mut m) = running().lock() {
        if let Some(t) = m.remove(profile_id) {
            t.abort();
        }
    }
}

const MAX_HEAD: usize = 64 * 1024;

/// Reads up to and including the blank line that ends an HTTP head; returns the
/// head and whatever bytes followed it in the same read.
async fn read_head(s: &mut TcpStream) -> std::io::Result<(Vec<u8>, Vec<u8>)> {
    let mut buf = Vec::with_capacity(2048);
    let mut chunk = [0u8; 4096];
    loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let rest = buf.split_off(i + 4);
            return Ok((buf, rest));
        }
        if buf.len() > MAX_HEAD {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "request head too large"));
        }
        let n = s.read(&mut chunk).await?;
        if n == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}

async fn dial(upstream: &str) -> std::io::Result<TcpStream> {
    tokio::time::timeout(Duration::from_secs(20), TcpStream::connect(upstream))
        .await
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "upstream proxy did not answer"))?
}

async fn handle(mut client: TcpStream, upstream: &str, auth: &str) -> std::io::Result<()> {
    let (head, rest) = read_head(&mut client).await?;
    let text = String::from_utf8_lossy(&head).to_string();
    let mut lines = text.split("\r\n");
    let request_line = lines.next().unwrap_or("").to_string();
    let mut parts = request_line.split_whitespace();
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));

    let mut up = match dial(upstream).await {
        Ok(s) => s,
        Err(_) => {
            let _ = client.write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n").await;
            return Ok(());
        }
    };

    if method.eq_ignore_ascii_case("CONNECT") {
        let req = format!(
            "CONNECT {target} HTTP/1.1\r\nHost: {target}\r\nProxy-Authorization: Basic {auth}\r\nProxy-Connection: keep-alive\r\n\r\n"
        );
        up.write_all(req.as_bytes()).await?;
        let (resp_head, resp_rest) = read_head(&mut up).await?;
        client.write_all(&resp_head).await?;
        let ok = String::from_utf8_lossy(&resp_head).split_whitespace().nth(1).map(|c| c.starts_with('2')).unwrap_or(false);
        if !ok {
            return Ok(());
        }
        client.write_all(&resp_rest).await?;
        up.write_all(&rest).await?;
    } else {
        // A plain request through the proxy: same head with our login, one request
        // per upstream connection (the browser simply reconnects for the next one).
        let mut out = format!("{request_line}\r\n");
        for l in lines.filter(|l| !l.is_empty()) {
            let name = l.split(':').next().unwrap_or("").trim().to_ascii_lowercase();
            if matches!(name.as_str(), "proxy-authorization" | "proxy-connection" | "connection") {
                continue;
            }
            out.push_str(l);
            out.push_str("\r\n");
        }
        out.push_str(&format!("Proxy-Authorization: Basic {auth}\r\nConnection: close\r\n\r\n"));
        up.write_all(out.as_bytes()).await?;
        up.write_all(&rest).await?;
    }
    let _ = tokio::io::copy_bidirectional(&mut client, &mut up).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: ProxyKind, port: u16, user: &str, pass: &str) -> ProxyEntry {
        ProxyEntry {
            id: String::new(), name: String::new(), kind, host: "127.0.0.1".into(), port,
            username: user.into(), password: pass.into(), country: String::new(), notes: String::new(),
        }
    }

    #[test]
    fn only_an_http_proxy_with_an_engine_hostile_login_needs_the_relay() {
        assert!(needs_relay(&entry(ProxyKind::Http, 1, "u", "mzuwntg1odg0mw==")));
        assert!(needs_relay(&entry(ProxyKind::Http, 1, "a;b", "x")));
        assert!(needs_relay(&entry(ProxyKind::Http, 1, "u", "a,b")));
        assert!(!needs_relay(&entry(ProxyKind::Http, 1, "u", "plain+pass@x:y/#?")));
        // SOCKS5 carries `=` as written; no relay.
        assert!(!needs_relay(&entry(ProxyKind::Socks5, 1, "u", "p==")));
        assert!(cannot_carry(&entry(ProxyKind::Https, 1, "u", "p==")));
        assert!(!cannot_carry(&entry(ProxyKind::Https, 1, "u", "plain")));
    }

    /// An upstream HTTP proxy that insists on one login, and what it was shown.
    async fn mock_upstream(user: &str, pass: &str) -> (u16, std::sync::Arc<Mutex<Vec<String>>>) {
        let want = format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}")));
        let seen = std::sync::Arc::new(Mutex::new(Vec::<String>::new()));
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        let seen2 = seen.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut c, _)) = l.accept().await else { break };
                let (want, seen) = (want.clone(), seen2.clone());
                tokio::spawn(async move {
                    let Ok((head, _)) = read_head(&mut c).await else { return };
                    let text = String::from_utf8_lossy(&head).to_string();
                    let first = text.lines().next().unwrap_or("").to_string();
                    let got = text.lines().find_map(|l| l.strip_prefix("Proxy-Authorization: ")).unwrap_or("").to_string();
                    seen.lock().unwrap().push(format!("{first} | {got}"));
                    if got != want {
                        let _ = c.write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\nContent-Length: 0\r\n\r\n").await;
                        return;
                    }
                    if first.starts_with("CONNECT ") {
                        let target = first.split_whitespace().nth(1).unwrap_or("").to_string();
                        let Ok(mut t) = TcpStream::connect(&target).await else {
                            let _ = c.write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n").await;
                            return;
                        };
                        let _ = c.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await;
                        let _ = tokio::io::copy_bidirectional(&mut c, &mut t).await;
                    } else {
                        let _ = c.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\nPROXY-OK").await;
                    }
                });
            }
        });
        (port, seen)
    }

    async fn echo_server() -> u16 {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let Ok((mut c, _)) = l.accept().await else { break };
                tokio::spawn(async move {
                    let mut b = [0u8; 256];
                    while let Ok(n) = c.read(&mut b).await {
                        if n == 0 || c.write_all(&b[..n]).await.is_err() { break; }
                    }
                });
            }
        });
        port
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_login_with_equals_signs_reaches_the_proxy_intact_for_tunnels_and_plain_requests() {
        let pass = "mzuwntg1odg0mw==";
        let (up_port, seen) = mock_upstream("liam", pass).await;
        let echo = echo_server().await;
        let relay = start("relay-test", &entry(ProxyKind::Http, up_port, "liam", pass)).await.unwrap();

        // A tunnel (what every https page uses): CONNECT, then bytes both ways.
        let mut c = TcpStream::connect(("127.0.0.1", relay)).await.unwrap();
        c.write_all(format!("CONNECT 127.0.0.1:{echo} HTTP/1.1\r\nHost: 127.0.0.1:{echo}\r\n\r\n").as_bytes()).await.unwrap();
        let (head, _) = read_head(&mut c).await.unwrap();
        assert!(String::from_utf8_lossy(&head).starts_with("HTTP/1.1 200"), "{}", String::from_utf8_lossy(&head));
        c.write_all(b"hello through the tunnel").await.unwrap();
        let mut back = vec![0u8; 24];
        c.read_exact(&mut back).await.unwrap();
        assert_eq!(back, b"hello through the tunnel");

        // A plain http request: the browser's own Proxy-* headers are replaced by ours.
        let mut c = TcpStream::connect(("127.0.0.1", relay)).await.unwrap();
        c.write_all(b"GET http://example.test/ HTTP/1.1\r\nHost: example.test\r\nProxy-Connection: keep-alive\r\nProxy-Authorization: Basic wrong\r\n\r\n").await.unwrap();
        let mut all = Vec::new();
        c.read_to_end(&mut all).await.unwrap();
        assert!(String::from_utf8_lossy(&all).contains("PROXY-OK"), "{}", String::from_utf8_lossy(&all));

        let log = seen.lock().unwrap().clone();
        stop("relay-test");
        assert_eq!(log.len(), 2, "{log:?}");
        assert!(log.iter().all(|l| l.ends_with(&format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(format!("liam:{pass}"))))), "{log:?}");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_wrong_login_is_refused_by_the_proxy_and_passed_back_not_hidden() {
        let (up_port, _) = mock_upstream("liam", "right==").await;
        let relay = start("relay-test-2", &entry(ProxyKind::Http, up_port, "liam", "wrong==")).await.unwrap();
        let mut c = TcpStream::connect(("127.0.0.1", relay)).await.unwrap();
        c.write_all(b"CONNECT 127.0.0.1:9 HTTP/1.1\r\nHost: 127.0.0.1:9\r\n\r\n").await.unwrap();
        let (head, _) = read_head(&mut c).await.unwrap();
        stop("relay-test-2");
        assert!(String::from_utf8_lossy(&head).starts_with("HTTP/1.1 407"), "{}", String::from_utf8_lossy(&head));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_dead_upstream_gets_a_502_instead_of_a_hang() {
        // A port nothing listens on.
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let dead = l.local_addr().unwrap().port();
        drop(l);
        let relay = start("relay-test-3", &entry(ProxyKind::Http, dead, "u", "p==")).await.unwrap();
        let mut c = TcpStream::connect(("127.0.0.1", relay)).await.unwrap();
        c.write_all(b"CONNECT example.test:443 HTTP/1.1\r\nHost: example.test:443\r\n\r\n").await.unwrap();
        let mut all = Vec::new();
        c.read_to_end(&mut all).await.unwrap();
        stop("relay-test-3");
        assert!(String::from_utf8_lossy(&all).starts_with("HTTP/1.1 502"));
    }
}
