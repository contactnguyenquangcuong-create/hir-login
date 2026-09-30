//! A tiny local forwarder that supplies a proxy's login for the browser.
//!
//! The engine takes a proxy's credentials from `--proxy-server=scheme://user:pass@host:port`
//! and does not decode percent escapes, so the login has to go through as written.
//! Three characters cannot: measured against the real engine (its own net-log says
//! which proxy it resolved), a `=` anywhere in the login makes it drop the whole
//! setting and connect DIRECTLY — the real IP, silently — for HTTP and SOCKS5
//! alike; `;` loses the setting too, and `,` turns a SOCKS5 proxy into an HTTP one.
//! Proxy providers that hand out base64-looking passwords ending in `==` hit this.
//! Every other character tried (`+ @ : / # ? %` and spaces) goes through as written.
//!
//! For those logins the browser is pointed at `http://127.0.0.1:<port>` with no login
//! in it, and this forwarder — bound to loopback only, one per profile — presents the
//! login to the real proxy (Basic auth for an HTTP proxy, RFC 1929 for SOCKS5) and
//! passes the traffic on.

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

/// Login characters the engine's `--proxy-server` parsing cannot carry.
fn breaks_engine_arg(s: &str) -> bool {
    s.chars().any(|c| matches!(c, '=' | ';' | ','))
}

fn login_breaks_engine_arg(p: &ProxyEntry) -> bool {
    breaks_engine_arg(&p.username) || breaks_engine_arg(&p.password)
}

/// True when this proxy has to go through the local forwarder (HTTP or SOCKS5).
pub fn needs_relay(p: &ProxyEntry) -> bool {
    matches!(p.kind, ProxyKind::Http | ProxyKind::Socks5) && login_breaks_engine_arg(p)
}

/// Whether UDP (QUIC, WebRTC) can really travel through this proxy. Through the local
/// forwarder it cannot: the browser only sees an HTTP proxy on loopback. Claiming
/// it could made the launcher enable QUIC and let WebRTC "relay through the proxy",
/// and measured with the real engine the page then gathered a WebRTC candidate with
/// this machine's real public IP (and real IPv6).
pub fn udp_path_exists(p: &ProxyEntry, udp_relay_probe_ok: bool) -> bool {
    udp_relay_probe_ok && !needs_relay(p)
}

/// An `https://` proxy (TLS to the proxy itself) with such a login has no safe
/// way through, and handing it over as-is would mean no proxy at all.
pub fn cannot_carry(p: &ProxyEntry) -> bool {
    matches!(p.kind, ProxyKind::Https) && login_breaks_engine_arg(p)
}

/// What the forwarder talks to on the far side.
#[derive(Clone)]
enum Upstream {
    Http { addr: String, auth: String },
    Socks5 { addr: String, user: String, pass: String },
}

/// Starts the forwarder for `profile_id` (replacing any earlier one) and returns
/// the loopback port the browser should use as its HTTP proxy.
pub async fn start(profile_id: &str, p: &ProxyEntry) -> Result<u16> {
    let addr = format!("{}:{}", p.host, p.port);
    let upstream = match p.kind {
        ProxyKind::Socks5 => Upstream::Socks5 { addr, user: p.username.clone(), pass: p.password.clone() },
        _ => Upstream::Http {
            addr,
            auth: base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", p.username, p.password)),
        },
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.context("bind local proxy relay")?;
    let port = listener.local_addr()?.port();
    let task = tokio::spawn(async move {
        loop {
            let Ok((conn, _)) = listener.accept().await else { break };
            let upstream = upstream.clone();
            tokio::spawn(async move {
                let _ = handle(conn, &upstream).await;
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

/// SOCKS5 (RFC 1928) CONNECT to `host:port` with username/password (RFC 1929).
/// The target goes over as a name, so DNS is done by the proxy, as a browser expects.
async fn socks5_connect(s: &mut TcpStream, user: &str, pass: &str, host: &str, port: u16) -> std::io::Result<()> {
    let bad = |m: &str| std::io::Error::other(m.to_string());
    s.write_all(&[5, 2, 0, 2]).await?;
    let mut sel = [0u8; 2];
    s.read_exact(&mut sel).await?;
    if sel[0] != 5 {
        return Err(bad("not a SOCKS5 proxy"));
    }
    match sel[1] {
        0 => {}
        2 => {
            if user.len() > 255 || pass.len() > 255 {
                return Err(bad("login too long for SOCKS5"));
            }
            let mut m = vec![1, user.len() as u8];
            m.extend_from_slice(user.as_bytes());
            m.push(pass.len() as u8);
            m.extend_from_slice(pass.as_bytes());
            s.write_all(&m).await?;
            let mut r = [0u8; 2];
            s.read_exact(&mut r).await?;
            if r[1] != 0 {
                return Err(bad("SOCKS5 login refused"));
            }
        }
        _ => return Err(bad("SOCKS5 proxy accepts none of our login methods")),
    }
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let mut req = vec![5, 1, 0];
    if let Ok(ip) = host.parse::<std::net::Ipv4Addr>() {
        req.push(1);
        req.extend_from_slice(&ip.octets());
    } else if let Ok(ip) = host.parse::<std::net::Ipv6Addr>() {
        req.push(4);
        req.extend_from_slice(&ip.octets());
    } else {
        if host.len() > 255 {
            return Err(bad("host name too long"));
        }
        req.push(3);
        req.push(host.len() as u8);
        req.extend_from_slice(host.as_bytes());
    }
    req.extend_from_slice(&port.to_be_bytes());
    s.write_all(&req).await?;
    let mut h = [0u8; 4];
    s.read_exact(&mut h).await?;
    if h[1] != 0 {
        return Err(bad("SOCKS5 connect refused"));
    }
    let skip = match h[3] {
        1 => 4 + 2,
        4 => 16 + 2,
        3 => {
            let mut l = [0u8; 1];
            s.read_exact(&mut l).await?;
            l[0] as usize + 2
        }
        _ => return Err(bad("bad SOCKS5 reply")),
    };
    let mut junk = vec![0u8; skip];
    s.read_exact(&mut junk).await?;
    Ok(())
}

/// `host:port` of a CONNECT target, or of an `http://host[:port]/path` request target.
fn split_host_port(hostport: &str, default_port: u16) -> (String, u16) {
    if let Some(rest) = hostport.strip_prefix('[') {
        if let Some((h, tail)) = rest.split_once(']') {
            return (h.to_string(), tail.strip_prefix(':').and_then(|p| p.parse().ok()).unwrap_or(default_port));
        }
    }
    match hostport.rsplit_once(':') {
        Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty() => (h.to_string(), p.parse().unwrap_or(default_port)),
        _ => (hostport.to_string(), default_port),
    }
}

async fn handle(mut client: TcpStream, upstream: &Upstream) -> std::io::Result<()> {
    let (head, rest) = read_head(&mut client).await?;
    let text = String::from_utf8_lossy(&head).to_string();
    let mut lines = text.split("\r\n");
    let request_line = lines.next().unwrap_or("").to_string();
    let mut parts = request_line.split_whitespace();
    let (method, target, version) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""), parts.next().unwrap_or("HTTP/1.1"));
    let is_connect = method.eq_ignore_ascii_case("CONNECT");
    let addr = match upstream {
        Upstream::Http { addr, .. } | Upstream::Socks5 { addr, .. } => addr,
    };

    let mut up = match dial(addr).await {
        Ok(s) => s,
        Err(_) => {
            let _ = client.write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n").await;
            return Ok(());
        }
    };

    match upstream {
        Upstream::Http { auth, .. } => {
            if is_connect {
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
        }
        Upstream::Socks5 { user, pass, .. } => {
            // A SOCKS proxy does not speak HTTP: open the tunnel to the real target
            // first, then either hand the browser its 200 (CONNECT) or send the
            // request itself in origin form.
            let (host, port, path) = if is_connect {
                let (h, p) = split_host_port(target, 443);
                (h, p, String::new())
            } else {
                let rest_of = target.strip_prefix("http://").unwrap_or(target);
                let (hostport, path) = match rest_of.find('/') {
                    Some(i) => (&rest_of[..i], rest_of[i..].to_string()),
                    None => (rest_of, "/".to_string()),
                };
                let (h, p) = split_host_port(hostport, 80);
                (h, p, path)
            };
            if socks5_connect(&mut up, user, pass, &host, port).await.is_err() {
                let _ = client.write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n").await;
                return Ok(());
            }
            if is_connect {
                client.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await?;
                up.write_all(&rest).await?;
            } else {
                let mut out = format!("{method} {path} {version}\r\n");
                let mut has_host = false;
                for l in lines.filter(|l| !l.is_empty()) {
                    let name = l.split(':').next().unwrap_or("").trim().to_ascii_lowercase();
                    if matches!(name.as_str(), "proxy-authorization" | "proxy-connection" | "connection") {
                        continue;
                    }
                    has_host |= name == "host";
                    out.push_str(l);
                    out.push_str("\r\n");
                }
                if !has_host {
                    out.push_str(&format!("Host: {host}\r\n"));
                }
                out.push_str("Connection: close\r\n\r\n");
                up.write_all(out.as_bytes()).await?;
                up.write_all(&rest).await?;
            }
        }
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
    fn only_a_proxy_whose_login_the_engine_cannot_carry_needs_the_relay() {
        assert!(needs_relay(&entry(ProxyKind::Http, 1, "u", "c2VjcmV0cGFzcw==")));
        assert!(needs_relay(&entry(ProxyKind::Http, 1, "a;b", "x")));
        assert!(needs_relay(&entry(ProxyKind::Http, 1, "u", "a,b")));
        assert!(!needs_relay(&entry(ProxyKind::Http, 1, "u", "plain+pass@x:y/#?% ")));
        // SOCKS5 has the same three (the engine drops the proxy for `=`, mangles `,`/`;`).
        assert!(needs_relay(&entry(ProxyKind::Socks5, 1, "user01", "c2VjcmV0cGFzcw==")));
        assert!(needs_relay(&entry(ProxyKind::Socks5, 1, "u", "a;b")));
        assert!(needs_relay(&entry(ProxyKind::Socks5, 1, "u", "a,b")));
        assert!(!needs_relay(&entry(ProxyKind::Socks5, 1, "u", "plain+pass@x:y/#?% ")));
        assert!(cannot_carry(&entry(ProxyKind::Https, 1, "u", "p==")));
        // UDP: a probe that succeeds still does not count once the forwarder is in the path.
        assert!(udp_path_exists(&entry(ProxyKind::Socks5, 1, "u", "plain"), true));
        assert!(!udp_path_exists(&entry(ProxyKind::Socks5, 1, "u", "p=="), true), "forwarded: no UDP, or WebRTC leaks the real IP");
        assert!(!udp_path_exists(&entry(ProxyKind::Socks5, 1, "u", "plain"), false));
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
        let pass = "c2VjcmV0cGFzcw==";
        let (up_port, seen) = mock_upstream("user01", pass).await;
        let echo = echo_server().await;
        let relay = start("relay-test", &entry(ProxyKind::Http, up_port, "user01", pass)).await.unwrap();

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
        assert!(log.iter().all(|l| l.ends_with(&format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(format!("user01:{pass}"))))), "{log:?}");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_wrong_login_is_refused_by_the_proxy_and_passed_back_not_hidden() {
        let (up_port, _) = mock_upstream("user01", "right==").await;
        let relay = start("relay-test-2", &entry(ProxyKind::Http, up_port, "user01", "wrong==")).await.unwrap();
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

    /// A SOCKS5 server that insists on one login, records the login and the target
    /// it was asked for, then tunnels to that target (local echo / tiny HTTP server).
    async fn mock_socks(user: &str, pass: &str) -> (u16, std::sync::Arc<Mutex<Vec<String>>>) {
        let (user, pass) = (user.to_string(), pass.to_string());
        let seen = std::sync::Arc::new(Mutex::new(Vec::<String>::new()));
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        let seen2 = seen.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut c, _)) = l.accept().await else { break };
                let (user, pass, seen) = (user.clone(), pass.clone(), seen2.clone());
                tokio::spawn(async move {
                    let mut b = [0u8; 2];
                    if c.read_exact(&mut b).await.is_err() { return; }
                    let mut methods = vec![0u8; b[1] as usize];
                    if c.read_exact(&mut methods).await.is_err() { return; }
                    if !methods.contains(&2) { let _ = c.write_all(&[5, 0xff]).await; return; }
                    let _ = c.write_all(&[5, 2]).await;
                    let mut v = [0u8; 2];
                    if c.read_exact(&mut v).await.is_err() { return; }
                    let mut u = vec![0u8; v[1] as usize];
                    let _ = c.read_exact(&mut u).await;
                    let mut pl = [0u8; 1];
                    let _ = c.read_exact(&mut pl).await;
                    let mut p = vec![0u8; pl[0] as usize];
                    let _ = c.read_exact(&mut p).await;
                    let got = format!("{}:{}", String::from_utf8_lossy(&u), String::from_utf8_lossy(&p));
                    if got != format!("{user}:{pass}") {
                        seen.lock().unwrap().push(format!("refused {got}"));
                        let _ = c.write_all(&[1, 1]).await;
                        return;
                    }
                    let _ = c.write_all(&[1, 0]).await;
                    let mut h = [0u8; 4];
                    if c.read_exact(&mut h).await.is_err() { return; }
                    let host = match h[3] {
                        3 => { let mut n = [0u8; 1]; let _ = c.read_exact(&mut n).await; let mut x = vec![0u8; n[0] as usize]; let _ = c.read_exact(&mut x).await; String::from_utf8_lossy(&x).to_string() }
                        1 => { let mut x = [0u8; 4]; let _ = c.read_exact(&mut x).await; std::net::Ipv4Addr::from(x).to_string() }
                        _ => return,
                    };
                    let mut pt = [0u8; 2];
                    let _ = c.read_exact(&mut pt).await;
                    let port = u16::from_be_bytes(pt);
                    seen.lock().unwrap().push(format!("ok {got} -> {host}:{port} (atyp {})", h[3]));
                    // `example.test` is pretend-DNS; serve it from a built-in page, anything else dial for real.
                    if host == "example.test" {
                        let mut buf = vec![0u8; 4096];
                        let _ = c.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).await;
                        let n = c.read(&mut buf).await.unwrap_or(0);
                        let head = String::from_utf8_lossy(&buf[..n]).to_string();
                        seen.lock().unwrap().push(format!("request: {}", head.lines().next().unwrap_or("")));
                        let _ = c.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\nSOCKS-OK").await;
                        return;
                    }
                    let Ok(mut t) = TcpStream::connect((host.as_str(), port)).await else { let _ = c.write_all(&[5, 5, 0, 1, 0, 0, 0, 0, 0, 0]).await; return; };
                    let _ = c.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).await;
                    let _ = tokio::io::copy_bidirectional(&mut c, &mut t).await;
                });
            }
        });
        (port, seen)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_socks5_login_with_equals_signs_is_presented_intact_for_tunnels_and_plain_requests() {
        let pass = "c2VjcmV0cGFzcw==";
        let (up_port, seen) = mock_socks("user01", pass).await;
        let echo = echo_server().await;
        let relay = start("relay-socks", &entry(ProxyKind::Socks5, up_port, "user01", pass)).await.unwrap();

        // https pages: CONNECT to a name, then bytes both ways.
        let mut c = TcpStream::connect(("127.0.0.1", relay)).await.unwrap();
        c.write_all(format!("CONNECT localhost:{echo} HTTP/1.1\r\nHost: localhost:{echo}\r\n\r\n").as_bytes()).await.unwrap();
        let (head, _) = read_head(&mut c).await.unwrap();
        assert!(String::from_utf8_lossy(&head).starts_with("HTTP/1.1 200"), "{}", String::from_utf8_lossy(&head));
        c.write_all(b"hello over socks").await.unwrap();
        let mut back = vec![0u8; 16];
        c.read_exact(&mut back).await.unwrap();
        assert_eq!(back, b"hello over socks");

        // a plain http page: the request reaches the target in origin form.
        let mut c = TcpStream::connect(("127.0.0.1", relay)).await.unwrap();
        c.write_all(b"GET http://example.test:8080/a/b?x=1 HTTP/1.1\r\nHost: example.test:8080\r\nProxy-Connection: keep-alive\r\n\r\n").await.unwrap();
        let mut all = Vec::new();
        c.read_to_end(&mut all).await.unwrap();
        assert!(String::from_utf8_lossy(&all).contains("SOCKS-OK"), "{}", String::from_utf8_lossy(&all));

        let log = seen.lock().unwrap().clone();
        stop("relay-socks");
        assert!(log.iter().any(|l| l == &format!("ok user01:{pass} -> localhost:{echo} (atyp 3)")), "{log:?}");
        assert!(log.iter().any(|l| l.contains("-> example.test:8080 (atyp 3)")), "{log:?}");
        assert!(log.iter().any(|l| l == "request: GET /a/b?x=1 HTTP/1.1"), "{log:?}");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_refused_socks5_login_becomes_a_502_not_a_direct_connection() {
        let (up_port, seen) = mock_socks("user01", "right==").await;
        let relay = start("relay-socks-2", &entry(ProxyKind::Socks5, up_port, "user01", "wrong==")).await.unwrap();
        let mut c = TcpStream::connect(("127.0.0.1", relay)).await.unwrap();
        c.write_all(b"CONNECT example.test:443 HTTP/1.1\r\nHost: example.test:443\r\n\r\n").await.unwrap();
        let mut all = Vec::new();
        c.read_to_end(&mut all).await.unwrap();
        let log = seen.lock().unwrap().clone();
        stop("relay-socks-2");
        assert!(String::from_utf8_lossy(&all).starts_with("HTTP/1.1 502"), "{}", String::from_utf8_lossy(&all));
        assert!(log.iter().any(|l| l.starts_with("refused ")), "{log:?}");
    }
}
