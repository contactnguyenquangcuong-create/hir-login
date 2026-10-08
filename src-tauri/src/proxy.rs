use crate::{settings, store};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyKind {
    Socks5,
    Http,
    Https,
}

impl ProxyKind {
    /// Reads "http"/"https"/"socks5" case-insensitively (also "sock5", "socks"),
    /// with "" or anything unrecognised falling back to Socks5 — the shape most
    /// exported proxy lists already come in.
    pub fn parse(s: &str) -> ProxyKind {
        match s.trim().to_lowercase().as_str() {
            "http" => ProxyKind::Http,
            "https" => ProxyKind::Https,
            _ => ProxyKind::Socks5,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ProxyKind::Socks5 => "socks5",
            ProxyKind::Http => "http",
            ProxyKind::Https => "https",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyEntry {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub kind: ProxyKind,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    /// "PL", "US", …
    #[serde(default)]
    pub country: String,
    /// Free-form note.
    #[serde(default)]
    pub notes: String,
}

impl ProxyEntry {
    /// `--proxy-server` value for the browser engine: `<scheme>://[user:pass@]host:port`
    /// with the login written exactly as it is. The engine does not decode percent
    /// escapes, so the URL form below sent a password like `ab=` as `ab%3D` — which
    /// the proxy then rejected. `+ @ : / # ? %` and spaces pass through as written;
    /// `=`, `;` and `,` do not (for any proxy kind), and such logins are handled by
    /// `proxy_relay` instead — see there.
    pub fn to_engine_arg(&self) -> String {
        let scheme = self.kind.as_str();
        let host_port = format!("{}:{}", self.host, self.port);
        if self.username.is_empty() && self.password.is_empty() {
            format!("{scheme}://{host_port}")
        } else {
            format!("{scheme}://{}:{}@{host_port}", self.username, self.password)
        }
    }

    /// The same as a proper URL (percent-encoded) for HTTP clients such as `reqwest`.
    pub fn to_proxy_server_arg(&self) -> String {
        let scheme = self.kind.as_str();
        let host_port = format!("{}:{}", self.host, self.port);
        if self.username.is_empty() && self.password.is_empty() {
            format!("{scheme}://{host_port}")
        } else {
            let user = url::form_urlencoded::byte_serialize(self.username.as_bytes())
                .collect::<String>();
            let pass = url::form_urlencoded::byte_serialize(self.password.as_bytes())
                .collect::<String>();
            format!("{scheme}://{user}:{pass}@{host_port}")
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProxyStore {
    #[serde(default)]
    pub proxies: Vec<ProxyEntry>,
}

/// Serialises every read-modify-write of the proxy list. Without it two
/// writers (an edit and the background country test that follows it) could each
/// load the old list and the later save would drop the other's change.
fn store_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Temp file + rename, so a reader never sees a half-written (or empty) file.
fn write_atomic(path: &std::path::Path, body: &[u8]) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, body)?;
    crate::winfs::rename_replace(&tmp, path)?;
    Ok(())
}

pub fn load() -> Result<ProxyStore> {
    let path = store::proxies_path()?;
    if !path.exists() {
        return Ok(ProxyStore::default());
    }
    let body = fs::read_to_string(&path)?;
    match serde_json::from_str(&body) {
        Ok(s) => Ok(s),
        Err(e) => {
            // Never treat an unreadable list as an empty one: the next save would
            // overwrite every proxy. Keep a copy and report the error instead.
            let backup = path.with_extension("json.corrupt");
            let _ = fs::copy(&path, &backup);
            anyhow::bail!("proxies.json could not be read ({e}); a copy was kept at {}", backup.display())
        }
    }
}

fn save(s: &ProxyStore) -> Result<()> {
    let body = serde_json::to_string_pretty(s)?;
    write_atomic(&store::proxies_path()?, body.as_bytes())
}

pub fn list() -> Result<Vec<ProxyEntry>> {
    Ok(load()?.proxies)
}

pub fn upsert(mut entry: ProxyEntry) -> Result<ProxyEntry> {
    let _g = store_guard();
    if entry.id.is_empty() {
        entry.id = uuid::Uuid::new_v4().to_string();
    }
    let mut s = load()?;
    if let Some(slot) = s.proxies.iter_mut().find(|p| p.id == entry.id) {
        *slot = entry.clone();
    } else {
        s.proxies.push(entry.clone());
    }
    save(&s)?;
    Ok(entry)
}

/// Upsert that reuses an entry with the same kind/host/port/username.
pub fn upsert_dedup(mut entry: ProxyEntry) -> Result<ProxyEntry> {
    let _g = store_guard();
    let mut s = load()?;
    if let Some(existing) = s.proxies.iter().find(|p| {
        p.kind == entry.kind
            && p.host == entry.host
            && p.port == entry.port
            && p.username == entry.username
    }) {
        return Ok(existing.clone());
    }
    if entry.id.is_empty() {
        entry.id = uuid::Uuid::new_v4().to_string();
    }
    s.proxies.push(entry.clone());
    save(&s)?;
    Ok(entry)
}

pub fn delete(id: &str) -> Result<()> {
    let _g = store_guard();
    let mut s = load()?;
    s.proxies.retain(|p| p.id != id);
    save(&s)?;
    // Also wipe persisted test history.
    let mut hs = load_history()?;
    if hs.by_proxy.remove(id).is_some() {
        save_history(&hs)?;
    }
    forget_verified(id);
    Ok(())
}

pub fn get(id: &str) -> Result<Option<ProxyEntry>> {
    Ok(load()?.proxies.into_iter().find(|p| p.id == id))
}

/// SOCKS5/HTTP CONNECT probe; returns RTT in ms on success.
pub async fn probe(entry: &ProxyEntry) -> Result<u128> {
    // Only the connect had a timeout; every read after it could wait forever on
    // a proxy that accepts the TCP connection but never answers the handshake
    // (an HTTPS proxy declared as SOCKS5, for one) and hang the launch with it.
    tokio::time::timeout(std::time::Duration::from_secs(12), probe_inner(entry))
        .await
        .context("proxy did not answer — check the proxy type (SOCKS5/HTTP/HTTPS)")?
}

async fn probe_inner(entry: &ProxyEntry) -> Result<u128> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio::time::{timeout, Duration, Instant};

    let started = Instant::now();
    let addr = format!("{}:{}", entry.host, entry.port);
    let mut stream = timeout(Duration::from_secs(8), TcpStream::connect(&addr))
        .await
        .context("connect timeout")??;

    match entry.kind {
        ProxyKind::Socks5 => {
            // RFC 1928 §3 greeting
            let auth_method: u8 = if entry.username.is_empty() { 0x00 } else { 0x02 };
            stream.write_all(&[0x05, 0x01, auth_method]).await?;
            let mut resp = [0u8; 2];
            stream.read_exact(&mut resp).await?;
            if resp[0] != 0x05 {
                anyhow::bail!("not SOCKS5");
            }
            if resp[1] == 0xFF {
                anyhow::bail!("no acceptable auth method");
            }
            if auth_method == 0x02 {
                // RFC 1929 user/pass sub-negotiation
                let mut buf = vec![0x01u8];
                buf.push(entry.username.len() as u8);
                buf.extend_from_slice(entry.username.as_bytes());
                buf.push(entry.password.len() as u8);
                buf.extend_from_slice(entry.password.as_bytes());
                stream.write_all(&buf).await?;
                let mut auth_resp = [0u8; 2];
                stream.read_exact(&mut auth_resp).await?;
                if auth_resp[1] != 0x00 {
                    anyhow::bail!("auth failed");
                }
            }
        }
        ProxyKind::Http | ProxyKind::Https => {
            // CONNECT with Basic auth; read until CRLFCRLF to avoid clipping headers.
            use base64::{engine::general_purpose::STANDARD, Engine as _};
            let mut req = String::from(
                "CONNECT example.com:443 HTTP/1.1\r\n\
                 Host: example.com:443\r\n",
            );
            if !entry.username.is_empty() || !entry.password.is_empty() {
                let creds = format!("{}:{}", entry.username, entry.password);
                let encoded = STANDARD.encode(creds.as_bytes());
                req.push_str(&format!("Proxy-Authorization: Basic {encoded}\r\n"));
            }
            req.push_str("Proxy-Connection: keep-alive\r\n\r\n");
            stream.write_all(req.as_bytes()).await?;

            // Read until CRLFCRLF or 4 KB cap.
            let mut buf = Vec::with_capacity(512);
            let mut tmp = [0u8; 256];
            let head: String = loop {
                let n = timeout(Duration::from_secs(8), stream.read(&mut tmp))
                    .await
                    .context("read timeout")??;
                if n == 0 { break String::from_utf8_lossy(&buf).to_string(); }
                buf.extend_from_slice(&tmp[..n]);
                if buf.windows(4).any(|w| w == b"\r\n\r\n") || buf.len() > 4096 {
                    break String::from_utf8_lossy(&buf).to_string();
                }
            };
            let first_line = head.lines().next().unwrap_or("");
            if !first_line.starts_with("HTTP/1.1 200") && !first_line.starts_with("HTTP/1.0 200") {
                anyhow::bail!("CONNECT failed: {first_line}");
            }
        }
    }
    Ok(started.elapsed().as_millis())
}

// ---- Bulk import ----
//
// Accepted: socks5://user:pass@host:port, user:pass@host:port, host:port:user:pass,
//           host:port@user:pass, host:port. A trailing `#` is the proxy's name
//           (`#facebook`); `country=X` and `note=Y` there are still read, for
//           lines exported by older builds, but the country a proxy reports is
//           filled in by its test. Whole-line `#` comments are skipped.
//           SOCKS5 when no scheme given.

/// Parse a single proxy line for inline (unsaved) use by the API.
pub fn parse_single(line: &str) -> Option<ProxyEntry> {
    parse_one(line.trim(), &ProxyKind::Socks5)
}

/// Same as `parse_single`, but a line without a scheme (`http://`/`socks5://`/…)
/// prefix is assumed to be `default_kind` instead of always Socks5 — for callers
/// that let the operator say what kind their pasted/imported list is.
pub fn parse_single_with_kind(line: &str, default_kind: ProxyKind) -> Option<ProxyEntry> {
    parse_one(line.trim(), &default_kind)
}

pub fn parse_bulk(text: &str, default_kind: ProxyKind) -> Vec<ProxyEntry> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(p) = parse_one(line, &default_kind) {
            out.push(p);
        }
    }
    out
}

fn parse_one(line: &str, default_kind: &ProxyKind) -> Option<ProxyEntry> {
    // Optional trailing `# country=US note=foo`.
    let (main, comment) = match line.find('#') {
        Some(i) => (line[..i].trim(), Some(line[i + 1..].trim())),
        None => (line, None),
    };
    let (kind, rest) = if let Some(r) = main.strip_prefix("socks5://") {
        (ProxyKind::Socks5, r)
    } else if let Some(r) = main.strip_prefix("https://") {
        (ProxyKind::Https, r)
    } else if let Some(r) = main.strip_prefix("http://") {
        (ProxyKind::Http, r)
    } else {
        (default_kind.clone(), main)
    };

    let (host_part, user, pass) = if let Some((u, hp)) = rest.split_once('@') {
        let (un, pw) = u.split_once(':').unwrap_or((u, ""));
        (hp.to_string(), un.to_string(), pw.to_string())
    } else {
        // host:port or host:port:user:pass
        let parts: Vec<&str> = rest.split(':').collect();
        match parts.len() {
            2 => (rest.to_string(), String::new(), String::new()),
            4 => (
                format!("{}:{}", parts[0], parts[1]),
                parts[2].to_string(),
                parts[3].to_string(),
            ),
            _ => return None,
        }
    };

    let (host, port_s) = host_part.rsplit_once(':')?;
    let port: u16 = port_s.parse().ok()?;
    let mut country = String::new();
    let mut notes = String::new();
    // The comment is the name; `key=value` is only for lines older builds wrote.
    let mut name_parts: Vec<&str> = Vec::new();
    if let Some(c) = comment {
        for kv in c.split_whitespace() {
            if let Some(v) = kv.strip_prefix("country=") {
                country = v.to_string();
            } else if let Some(v) = kv.strip_prefix("note=") {
                notes = v.to_string();
            } else {
                name_parts.push(kv.trim_start_matches('#'));
            }
        }
    }
    let name = name_parts.join(" ");
    Some(ProxyEntry {
        // ID assigned now so pre-save test snapshots key under the kept uuid.
        id: uuid::Uuid::new_v4().to_string(),
        name: if name.is_empty() { format!("{host}:{port}") } else { name },
        kind,
        host: host.to_string(),
        port,
        username: user,
        password: pass,
        country,
        notes,
    })
}

/// Save many entries; returns count actually persisted (deduped on host:port:user).
pub fn bulk_save(entries: Vec<ProxyEntry>) -> Result<usize> {
    let _g = store_guard();
    let mut store_data = load()?;
    let mut added = 0usize;
    for mut e in entries {
        let dup = store_data
            .proxies
            .iter()
            .any(|x| x.host == e.host && x.port == e.port && x.username == e.username);
        if dup {
            continue;
        }
        if e.id.is_empty() {
            e.id = uuid::Uuid::new_v4().to_string();
        }
        store_data.proxies.push(e);
        added += 1;
    }
    save(&store_data)?;
    Ok(added)
}

// ---- UDP probe (SOCKS5 UDP_ASSOCIATE; RFC 1928 §7) ----

/// Resolve a public STUN server to IPv4 (probe target for the UDP relay).
async fn resolve_stun_ipv4() -> Result<(std::net::Ipv4Addr, u16)> {
    const HOSTS: &[&str] = &[
        "stun.l.google.com:19302",
        "stun1.l.google.com:19302",
        "stun.cloudflare.com:3478",
    ];
    for h in HOSTS {
        if let Ok(addrs) = tokio::net::lookup_host(*h).await {
            for a in addrs {
                if let std::net::IpAddr::V4(v4) = a.ip() {
                    return Ok((v4, a.port()));
                }
            }
        }
    }
    anyhow::bail!("no STUN server resolved to IPv4")
}

/// Can THIS process send UDP at all? Answers the question the relay probe
/// cannot: a VPN or firewall that passes TCP and drops UDP — and some do it
/// per application, so the browser may be allowed where the launcher is not —
/// looks exactly like a proxy without a relay. One STUN request straight out.
pub async fn can_send_udp_directly() -> bool {
    // A property of this machine, not of any proxy: asked once per run, not on every open.
    static ANSWER: std::sync::OnceLock<tokio::sync::OnceCell<bool>> = std::sync::OnceLock::new();
    *ANSWER.get_or_init(tokio::sync::OnceCell::new).get_or_init(can_send_udp_directly_now).await
}

async fn can_send_udp_directly_now() -> bool {
    use tokio::net::UdpSocket;
    use tokio::time::{timeout, Duration};
    let Ok((ip, port)) = resolve_stun_ipv4().await else {
        return false;
    };
    let Ok(sock) = UdpSocket::bind("0.0.0.0:0").await else {
        return false;
    };
    let mut req = vec![0x00u8, 0x01, 0x00, 0x00, 0x21, 0x12, 0xA4, 0x42];
    req.extend_from_slice(&uuid::Uuid::new_v4().as_bytes()[..12]);
    if sock.send_to(&req, (ip, port)).await.is_err() {
        return false;
    }
    let mut buf = [0u8; 512];
    matches!(
        timeout(Duration::from_secs(4), sock.recv_from(&mut buf)).await,
        Ok(Ok((n, _))) if n >= 20
    )
}

pub async fn probe_udp(entry: &ProxyEntry) -> Result<u128> {
    tokio::time::timeout(std::time::Duration::from_secs(12), probe_udp_inner(entry))
        .await
        .context("proxy did not answer the UDP handshake")?
}

async fn probe_udp_inner(entry: &ProxyEntry) -> Result<u128> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpStream, UdpSocket};
    use tokio::time::{timeout, Duration, Instant};

    if !matches!(entry.kind, ProxyKind::Socks5) {
        anyhow::bail!("UDP probe only supported for SOCKS5");
    }
    let started = Instant::now();
    let mut tcp = timeout(
        Duration::from_secs(8),
        TcpStream::connect(format!("{}:{}", entry.host, entry.port)),
    )
    .await
    .context("connect timeout")??;

    let auth_method: u8 = if entry.username.is_empty() { 0x00 } else { 0x02 };
    tcp.write_all(&[0x05, 0x01, auth_method]).await?;
    let mut greet = [0u8; 2];
    tcp.read_exact(&mut greet).await?;
    if greet[1] == 0xFF {
        anyhow::bail!("no acceptable auth method");
    }
    if auth_method == 0x02 {
        let mut buf = vec![0x01u8];
        buf.push(entry.username.len() as u8);
        buf.extend_from_slice(entry.username.as_bytes());
        buf.push(entry.password.len() as u8);
        buf.extend_from_slice(entry.password.as_bytes());
        tcp.write_all(&buf).await?;
        let mut ar = [0u8; 2];
        tcp.read_exact(&mut ar).await?;
        if ar[1] != 0x00 {
            anyhow::bail!("auth failed");
        }
    }
    // UDP_ASSOCIATE: cmd=0x03, ATYP=IPv4, addr=0.0.0.0, port=0
    tcp.write_all(&[0x05, 0x03, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
        .await?;
    let mut hdr = [0u8; 4];
    tcp.read_exact(&mut hdr).await?;
    if hdr[1] != 0x00 {
        anyhow::bail!("UDP_ASSOCIATE refused (rep={:#x})", hdr[1]);
    }
    let bind_addr: SocketAddr = match hdr[3] {
        0x01 => {
            // IPv4
            let mut ip = [0u8; 4];
            tcp.read_exact(&mut ip).await?;
            let mut p = [0u8; 2];
            tcp.read_exact(&mut p).await?;
            let port = u16::from_be_bytes(p);
            let v4 = std::net::Ipv4Addr::from(ip);
            // 0.0.0.0 → fall back to TCP peer (where the relay lives).
            if v4.is_unspecified() {
                let peer = tcp.peer_addr()?;
                SocketAddr::new(peer.ip(), port)
            } else {
                SocketAddr::new(std::net::IpAddr::V4(v4), port)
            }
        }
        0x04 => {
            let mut ip = [0u8; 16];
            tcp.read_exact(&mut ip).await?;
            let mut p = [0u8; 2];
            tcp.read_exact(&mut p).await?;
            SocketAddr::new(std::net::IpAddr::V6(std::net::Ipv6Addr::from(ip)), u16::from_be_bytes(p))
        }
        _ => anyhow::bail!("unsupported ATYP in UDP reply"),
    };

    // Probe with STUN binding request (DNS-port-53 often blocked, STUN passes).
    let (stun_ip, stun_port) = resolve_stun_ipv4()
        .await
        .context("could not resolve a STUN server to probe UDP with")?;

    // Not connect(): a connected UDP socket accepts datagrams only from the
    // exact address it was pointed at, and a relay is free to answer from
    // another one — RFC 1928 names BND.ADDR as where to SEND, not as where
    // replies come from. Behind a VPN that rewrites source addresses this is
    // the difference between a working relay and a silent timeout.
    let udp = UdpSocket::bind("0.0.0.0:0").await?;
    let mut pkt: Vec<u8> = Vec::with_capacity(32);
    // SOCKS5 UDP header: RSV(2)=0, FRAG=0, ATYP=IPv4, DST=<stun>, PORT.
    pkt.extend_from_slice(&[0, 0, 0, 0x01]);
    pkt.extend_from_slice(&stun_ip.octets());
    pkt.extend_from_slice(&stun_port.to_be_bytes());
    // STUN Binding Request (RFC 5389): type=0x0001, magic 0x2112A442, 12B txid.
    let mut stun = vec![0x00u8, 0x01, 0x00, 0x00, 0x21, 0x12, 0xA4, 0x42];
    stun.extend_from_slice(&uuid::Uuid::new_v4().as_bytes()[..12]);
    pkt.extend_from_slice(&stun);
    udp.send_to(&pkt, bind_addr)
        .await
        .with_context(|| format!("could not send UDP to the relay at {bind_addr}"))?;

    // A relay bound to this socket's source port answers only this socket, so
    // whatever arrives here is ours; the source is logged rather than trusted.
    let mut buf = vec![0u8; 1500];
    let (n, from) = timeout(Duration::from_secs(6), udp.recv_from(&mut buf))
        .await
        .context(
            "no UDP came back within 6s. Either the proxy does not relay UDP, \
             or this machine cannot send UDP out — a VPN or a firewall that \
             passes TCP and drops UDP looks exactly like a proxy without it",
        )??;
    if from != bind_addr {
        eprintln!("[launcher] UDP relay answered from {from}, associated at {bind_addr}");
    }
    if n < 20 {
        anyhow::bail!("UDP reply too short ({n} bytes)");
    }
    // RFC 1928: dropping TCP control tears down the relay; keep it alive.
    drop(tcp);
    Ok(started.elapsed().as_millis())
}

// ---- Geo lookup ----

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoInfo {
    pub ip: String,
    pub country: String,
    /// ISO 3166-1 alpha-2.
    pub country_code: String,
    pub region: String,
    pub city: String,
    pub isp: String,
    pub timezone: String,
    pub latitude: f64,
    pub longitude: f64,
    pub provider: String,
}

/// Probe IP/country the world sees when traffic exits the proxy.
pub async fn geo_check(entry: &ProxyEntry, provider_override: Option<String>) -> Result<GeoInfo> {
    geo_check_via(Some(entry), provider_override).await
}

/// The proxy as it should really be used — its real protocol, whatever the list called it.
///
/// Lists mislabel proxies in both directions. A pasted `host:port:user:pass` line is read as
/// SOCKS5, so an HTTP-only provider's proxies arrive as SOCKS5 and never answer the SOCKS
/// handshake; a provider's "HTTPS" is often a plain HTTP proxy, which used as TLS never answers
/// either; and an HTTP label can sit on a SOCKS5 port. In each case the proxy is live, the page
/// just times out and the location probe leaves the profile on UTC.
///
/// The declared protocol is tried first and kept if it answers. Only when it does not, the
/// other one is tried, and a proxy that answers that is used as such. One that answers neither
/// (a genuine TLS proxy, an unreachable one) stays as labelled — nothing is concluded from
/// silence. What was found is remembered per `host:port`, and written back to the saved proxy
/// so the list shows the real type.
///
/// This is the version for launching a profile: a proxy that was checked as it is configured now
/// is simply used as the kind it was checked as, and nothing is sent to it (see `geo_check_cached`).
/// `effective_live` below always looks.
pub async fn effective(entry: &ProxyEntry) -> ProxyEntry {
    if is_verified(entry) {
        return entry.clone();
    }
    effective_live(entry).await
}

/// `effective`, always asking the proxy. For the Test button, the first check, and the
/// background refresh.
pub async fn effective_live(entry: &ProxyEntry) -> ProxyEntry {
    static KNOWN: std::sync::OnceLock<std::sync::Mutex<HashMap<String, ProxyKind>>> = std::sync::OnceLock::new();
    let known = KNOWN.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    let key = format!("{}:{}", entry.host, entry.port);
    let cached = known.lock().ok().and_then(|m| m.get(&key).cloned());
    let found = match cached {
        Some(k) => Some(k),
        None => {
            let d = detect_kind(entry).await;
            if let (Some(k), Ok(mut m)) = (&d, known.lock()) {
                m.insert(key, k.clone());
            }
            d
        }
    };
    match found {
        Some(kind) if kind != entry.kind => {
            eprintln!(
                "[proxy] {}:{} is labelled {} but answers as {} — using it as {}",
                entry.host, entry.port, entry.kind.as_str(), kind.as_str(), kind.as_str()
            );
            if !entry.id.is_empty() {
                correct_stored_kind(entry, &kind);
            }
            let mut e = entry.clone();
            e.kind = kind;
            e
        }
        _ => entry.clone(),
    }
}

/// Writes the real protocol into the saved proxy, so the list stops showing the label the
/// provider's list happened to carry. Only an entry that still has the same address is touched.
fn correct_stored_kind(entry: &ProxyEntry, kind: &ProxyKind) {
    let _g = store_guard();
    let Ok(mut s) = load() else { return };
    let mut changed = false;
    for p in s.proxies.iter_mut() {
        if p.id == entry.id && p.host == entry.host && p.port == entry.port && p.kind != *kind {
            p.kind = kind.clone();
            changed = true;
        }
    }
    if changed {
        let _ = save(&s);
    }
}

/// What one probe of a port came back with.
enum Sniff {
    /// The connection could not be made: nothing else on this port is worth trying.
    Unreachable,
    /// Connected, and the first bytes the other side sent.
    Reply(Vec<u8>),
    /// Connected, and nothing came back (or it hung up).
    Silent,
}

/// Opens a connection, sends `payload` and returns what comes back first.
async fn sniff(entry: &ProxyEntry, payload: &[u8]) -> Sniff {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{timeout, Duration};
    let Ok(Ok(mut s)) = timeout(Duration::from_secs(5), tokio::net::TcpStream::connect((entry.host.as_str(), entry.port))).await else {
        return Sniff::Unreachable;
    };
    if s.write_all(payload).await.is_err() {
        return Sniff::Silent;
    }
    let mut buf = [0u8; 16];
    // A SOCKS5 server answers its greeting at once, and so does an HTTP proxy a CONNECT. An HTTP
    // proxy that is handed a SOCKS greeting says nothing, so this wait is the whole cost of the
    // first look at a proxy whose list labelled it wrongly — kept short for that reason.
    match timeout(Duration::from_millis(2500), s.read(&mut buf)).await {
        Ok(Ok(n)) if n > 0 => Sniff::Reply(buf[..n].to_vec()),
        _ => Sniff::Silent,
    }
}

/// The protocol `entry` really speaks: the declared one if it answers, else the other of
/// SOCKS5 / plain HTTP if that answers, else `None`.
async fn detect_kind(entry: &ProxyEntry) -> Option<ProxyKind> {
    // Version 5, two methods offered: none (00) and username/password (02).
    const SOCKS_GREETING: &[u8] = &[0x05, 0x02, 0x00, 0x02];
    const HTTP_CONNECT: &[u8] = b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n";
    let order = if matches!(entry.kind, ProxyKind::Socks5) {
        [ProxyKind::Socks5, ProxyKind::Http]
    } else {
        [ProxyKind::Http, ProxyKind::Socks5]
    };
    for kind in order {
        let (payload, is_it): (&[u8], fn(&[u8]) -> bool) = match kind {
            ProxyKind::Socks5 => (SOCKS_GREETING, |b| b.first() == Some(&0x05)),
            _ => (HTTP_CONNECT, |b| b.len() >= 5 && b[..5].eq_ignore_ascii_case(b"HTTP/")),
        };
        match sniff(entry, payload).await {
            Sniff::Unreachable => return None,
            Sniff::Reply(b) if is_it(&b) => return Some(kind),
            _ => {}
        }
    }
    None
}

// ---- what launching a profile asks of a proxy, asked once ----
//
// Opening a profile asked its proxy the same things one after another: where it exits (to set
// the time zone and language), where it exits again (for the WebRTC address), and whether it
// relays UDP (up to 6 s, and 4 more when it does not). Each of those goes through the proxy, so
// each costs a full round trip or a timeout. They are asked once now, at the same time, and
// remembered for a short while — callers that arrive while the first is still waiting share its
// answer instead of sending their own.

/// Remembers one answer per key for a while, and lets simultaneous askers share one request.
struct TtlCache<V: Clone + Send + Sync + 'static> {
    map: std::sync::Mutex<HashMap<String, (std::time::Instant, std::sync::Arc<tokio::sync::OnceCell<V>>)>>,
}

impl<V: Clone + Send + Sync + 'static> TtlCache<V> {
    fn new() -> Self {
        Self { map: std::sync::Mutex::new(HashMap::new()) }
    }

    /// The answer for `key`: remembered if it is younger than `ttl(&answer)`, otherwise
    /// computed by `make` (once, however many ask meanwhile). A `ttl` of zero forgets it at once.
    async fn get_or<F, Fut>(&self, key: &str, ttl: impl Fn(&V) -> std::time::Duration, make: F) -> V
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = V>,
    {
        let cell = {
            let mut m = self.map.lock().unwrap_or_else(|e| e.into_inner());
            let fresh = m.get(key).filter(|(at, cell)| match cell.get() {
                Some(v) => at.elapsed() < ttl(v),
                None => true, // still being asked: join it
            });
            match fresh {
                Some((_, cell)) => cell.clone(),
                None => {
                    let cell = std::sync::Arc::new(tokio::sync::OnceCell::new());
                    m.insert(key.to_string(), (std::time::Instant::now(), cell.clone()));
                    cell
                }
            }
        };
        let answer = cell.get_or_init(make).await.clone();
        // The age counts from when the answer arrived, not from when the question was asked.
        if let Ok(mut m) = self.map.lock() {
            if let Some((at, c)) = m.get_mut(key) {
                if std::sync::Arc::ptr_eq(c, &cell) {
                    *at = std::time::Instant::now();
                }
            }
        }
        answer
    }
}

/// Identifies a proxy for the caches below: the address and who it is used as, not the password.
fn proxy_key(entry: &ProxyEntry) -> String {
    format!("{}://{}@{}:{}", entry.kind.as_str(), entry.username, entry.host, entry.port)
}

// ---- a proxy is checked once, and trusted until it changes ----
//
// What opening a profile needs to know about its proxy — where it exits (time zone, language,
// WebRTC address) and whether it relays UDP — does not change from one open to the next. So it is
// found out the first time, written to `proxies-verified.json`, and used from then on without
// sending the proxy anything. What was learned belongs to the proxy *as configured*: change its
// type, host, port or login and the record no longer matches, and it is checked again. A record
// older than 12 hours is still used (the profile opens at once) and refreshed in the background;
// one older than a week is not trusted and the proxy is asked afresh.

const REFRESH_AFTER_SECS: u64 = 12 * 3600;
const EXPIRES_AFTER_SECS: u64 = 7 * 24 * 3600;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UdpFact {
    ok: bool,
    ms: u128,
    err: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Verified {
    /// `proxy_sig` of the proxy this was learned about.
    sig: String,
    #[serde(default)]
    geo: Option<GeoInfo>,
    #[serde(default)]
    geo_at: u64,
    #[serde(default)]
    udp: Option<UdpFact>,
    #[serde(default)]
    udp_at: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct VerifiedStore {
    #[serde(default)]
    by_proxy: HashMap<String, Verified>,
}

fn verified_path() -> Result<PathBuf> {
    Ok(store::user_files_root()?.join("proxies-verified.json"))
}

fn verified_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(())).lock().unwrap_or_else(|e| e.into_inner())
}

fn load_verified() -> VerifiedStore {
    verified_path()
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|b| serde_json::from_str(&b).ok())
        .unwrap_or_default()
}

fn save_verified(st: &VerifiedStore) {
    if let (Ok(body), Ok(path)) = (serde_json::to_string_pretty(st), verified_path()) {
        let _ = write_atomic(&path, body.as_bytes());
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// What the connection to a proxy consists of. A record about the proxy holds only while this
/// is unchanged; the name, country tag and notes are not part of it.
pub fn proxy_sig(e: &ProxyEntry) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    for part in [e.kind.as_str(), &e.host, &e.port.to_string(), &e.username, &e.password] {
        h.update(part.as_bytes());
        h.update([0u8]);
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn verified_for(entry: &ProxyEntry) -> Option<Verified> {
    if entry.id.is_empty() {
        return None; // not a saved proxy (a quick profile's own): nothing to remember it by
    }
    let st = load_verified();
    let v = st.by_proxy.get(&entry.id)?;
    (v.sig == proxy_sig(entry)).then(|| v.clone())
}

fn update_verified(entry: &ProxyEntry, f: impl FnOnce(&mut Verified)) {
    if entry.id.is_empty() {
        return;
    }
    let _g = verified_guard();
    let mut st = load_verified();
    let sig = proxy_sig(entry);
    let v = st.by_proxy.entry(entry.id.clone()).or_default();
    if v.sig != sig {
        *v = Verified { sig, ..Default::default() };
    }
    f(v);
    save_verified(&st);
}

fn forget_verified(id: &str) {
    let _g = verified_guard();
    let mut st = load_verified();
    if st.by_proxy.remove(id).is_some() {
        save_verified(&st);
    }
}

/// Where this proxy exits, from the record, with the record's age in seconds — unless there is
/// none, it is about a different configuration, or it is too old to trust.
fn geo_record(entry: &ProxyEntry) -> Option<(GeoInfo, u64)> {
    let v = verified_for(entry)?;
    let age = now_secs().saturating_sub(v.geo_at);
    if age >= EXPIRES_AFTER_SECS {
        return None;
    }
    v.geo.map(|g| (g, age))
}

fn udp_record(entry: &ProxyEntry) -> Option<(UdpFact, u64)> {
    let v = verified_for(entry)?;
    let age = now_secs().saturating_sub(v.udp_at);
    if age >= EXPIRES_AFTER_SECS {
        return None;
    }
    v.udp.map(|u| (u, age))
}

fn remember_geo(entry: &ProxyEntry, g: &GeoInfo) {
    update_verified(entry, |v| {
        v.geo = Some(g.clone());
        v.geo_at = now_secs();
    });
}

fn remember_udp(entry: &ProxyEntry, r: &Result<u128, String>) {
    update_verified(entry, |v| {
        v.udp = Some(match r {
            Ok(ms) => UdpFact { ok: true, ms: *ms, err: String::new() },
            Err(e) => UdpFact { ok: false, ms: 0, err: e.clone() },
        });
        v.udp_at = now_secs();
    });
}

/// True when the proxy has been checked as it is configured now and the check is still trusted:
/// nothing needs to be sent to it to open a profile.
fn is_verified(entry: &ProxyEntry) -> bool {
    geo_record(entry).is_some()
}

/// Checks the proxy again without making anyone wait: a profile opened on a record older than
/// 12 hours starts at once on it, and this runs beside. If the proxy no longer answers (a plan
/// that ran out, say) the record is dropped, so the next open asks for real and says what is
/// wrong, and a warning tells the person now.
fn refresh_in_background(entry: ProxyEntry) {
    static BUSY: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> = std::sync::OnceLock::new();
    let busy = BUSY.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()));
    if !busy.lock().map(|mut b| b.insert(entry.id.clone())).unwrap_or(false) {
        return;
    }
    tokio::spawn(async move {
        let used = effective_live(&entry).await;
        let want_udp = matches!(used.kind, ProxyKind::Socks5);
        let (geo, udp) = tokio::join!(geo_check_via(Some(&used), None), async {
            if want_udp {
                Some(probe_udp(&used).await.map_err(|e| e.to_string()))
            } else {
                None
            }
        });
        match geo {
            Ok(g) => {
                remember_geo(&used, &g);
                if let Some(u) = &udp {
                    remember_udp(&used, u);
                }
                eprintln!("[proxy] {}:{} re-checked in the background: exits from {} ({})", used.host, used.port, g.ip, g.country_code);
            }
            Err(e) => {
                forget_verified(&used.id);
                eprintln!("[proxy] {}:{} did not answer the background re-check: {e}", used.host, used.port);
                crate::notify_warning(format!(
                    "Proxy {}:{} không còn trả lời (có thể đã hết hạn hoặc bị chặn). Lần mở profile tới sẽ kiểm tra lại.",
                    used.host, used.port
                ));
            }
        }
        if let Ok(mut b) = busy.lock() {
            b.remove(&entry.id);
        }
    });
}

/// Where the proxy exits, for launching a profile: from the record when the proxy has been
/// checked and has not changed (nothing is sent to it), otherwise asked for now — once, however
/// many callers want it at the same time — and written down for next time. A failure is
/// remembered for half a minute only, so the second asker does not repeat a lookup that just failed.
pub async fn geo_check_cached(entry: &ProxyEntry) -> Result<GeoInfo, String> {
    if let Some((g, age)) = geo_record(entry) {
        if age > REFRESH_AFTER_SECS {
            refresh_in_background(entry.clone());
        }
        return Ok(g);
    }
    static CACHE: std::sync::OnceLock<TtlCache<Result<GeoInfo, String>>> = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(TtlCache::new);
    cache
        .get_or(
            &proxy_key(entry),
            |r| std::time::Duration::from_secs(if r.is_ok() { 120 } else { 30 }),
            || async {
                let r = geo_check_via(Some(entry), None).await.map_err(|e| e.to_string());
                if let Ok(g) = &r {
                    remember_geo(entry, g);
                }
                r
            },
        )
        .await
}

/// Whether the proxy relays UDP, for launching a profile: the same record-first rule as the
/// location. A failure is kept for three minutes in memory and is also written down — a proxy
/// that cannot relay UDP will not start to.
pub async fn probe_udp_cached(entry: &ProxyEntry) -> Result<u128, String> {
    if let Some((u, _)) = udp_record(entry) {
        return if u.ok { Ok(u.ms) } else { Err(u.err) };
    }
    static CACHE: std::sync::OnceLock<TtlCache<Result<u128, String>>> = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(TtlCache::new);
    cache
        .get_or(
            &proxy_key(entry),
            |r| std::time::Duration::from_secs(if r.is_ok() { 600 } else { 180 }),
            || async {
                let r = probe_udp(entry).await.map_err(|e| e.to_string());
                remember_udp(entry, &r);
                r
            },
        )
        .await
}

/// Where this machine's own connection exits (a profile with no proxy), kept for ten minutes.
pub async fn geo_check_direct_cached() -> Result<GeoInfo, String> {
    static CACHE: std::sync::OnceLock<TtlCache<Result<GeoInfo, String>>> = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(TtlCache::new);
    cache
        .get_or(
            "direct",
            |r| std::time::Duration::from_secs(if r.is_ok() { 600 } else { 30 }),
            || async { geo_check_via(None, None).await.map_err(|e| e.to_string()) },
        )
        .await
}

/// Every provider we know, chosen provider first. ip-api is plain HTTP on the
/// free tier, and a proxy that refuses port 80 or rewrites HTTP fails on it
/// alone — the other two are HTTPS, so the chain gets an answer anyway.
fn provider_chain(chosen: &str) -> Vec<String> {
    let all = ["ip-api.com", "ipapi.co", "ipwho.is"];
    let mut out = vec![chosen.to_string()];
    out.extend(all.iter().filter(|p| **p != chosen).map(|p| p.to_string()));
    out
}

/// Probe geo through `entry` if Some, else direct. Tries the chosen provider,
/// then the others; the error carries what each one said.
pub async fn geo_check_via(entry: Option<&ProxyEntry>, provider_override: Option<String>) -> Result<GeoInfo> {
    let effective_entry = match entry {
        Some(e) => Some(effective(e).await),
        None => None,
    };
    let entry = effective_entry.as_ref();
    let chosen = provider_override
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| settings::load().ok().and_then(|s| s.geo_checker).unwrap_or_else(|| "ip-api.com".into()));

    let mut failures = Vec::new();
    for provider in provider_chain(&chosen) {
        match geo_check_one(entry, &provider).await {
            Ok(info) => {
                if !failures.is_empty() {
                    eprintln!(
                        "[launcher] geo: {provider} answered after {} failed: {}",
                        failures.len(),
                        failures.join("; ")
                    );
                }
                return Ok(info);
            }
            Err(e) => failures.push(format!("{provider}: {e}")),
        }
    }
    anyhow::bail!("every geo source failed — {}", failures.join("; "))
}

async fn geo_check_one(entry: Option<&ProxyEntry>, provider: &str) -> Result<GeoInfo> {
    let url = match provider {
        "ip-api.com" => "http://ip-api.com/json/?fields=status,message,query,country,countryCode,regionName,city,isp,timezone,lat,lon",
        "ipapi.co" => "https://ipapi.co/json/",
        "ipwho.is" => "https://ipwho.is/",
        _ => "http://ip-api.com/json/?fields=status,message,query,country,countryCode,regionName,city,isp,timezone,lat,lon",
    };

    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8));
    if let Some(entry) = entry {
        let scheme = match entry.kind {
            ProxyKind::Socks5 => "socks5h", // DNS via proxy
            ProxyKind::Http => "http",
            ProxyKind::Https => "https",
        };
        let proxy_url = if entry.username.is_empty() && entry.password.is_empty() {
            format!("{scheme}://{}:{}", entry.host, entry.port)
        } else {
            let user = url::form_urlencoded::byte_serialize(entry.username.as_bytes()).collect::<String>();
            let pass = url::form_urlencoded::byte_serialize(entry.password.as_bytes()).collect::<String>();
            format!("{scheme}://{user}:{pass}@{}:{}", entry.host, entry.port)
        };
        let proxy = reqwest::Proxy::all(&proxy_url).context("bad proxy URL")?;
        builder = builder.proxy(proxy);
    } else {
        // Direct check: bypass any system proxy.
        builder = builder.no_proxy();
    }
    let client = builder.build()?;

    let body: serde_json::Value = client.get(url).send().await?.json().await?;

    let s = |v: &serde_json::Value, k: &str| {
        v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
    };
    let f = |v: &serde_json::Value, k: &str| {
        v.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0)
    };
    // A provider over its quota answers 200 with an error object. Parsed as a
    // success that gave no fields, it fell through as "no location" and the
    // chain stopped trying — the profile then launched on the host's clock.
    if body.get("error").and_then(|v| v.as_bool()) == Some(true)
        || body.get("success").and_then(|v| v.as_bool()) == Some(false)
    {
        let why = body
            .get("reason")
            .or_else(|| body.get("message"))
            .and_then(|v| v.as_str())
            .unwrap_or("refused");
        anyhow::bail!("{provider}: {why}");
    }

    let info = match provider {
        "ip-api.com" => {
            if s(&body, "status") == "fail" {
                anyhow::bail!("ip-api.com: {}", s(&body, "message"));
            }
            GeoInfo {
                ip: s(&body, "query"),
                country: s(&body, "country"),
                country_code: s(&body, "countryCode"),
                region: s(&body, "regionName"),
                city: s(&body, "city"),
                isp: s(&body, "isp"),
                timezone: s(&body, "timezone"),
                latitude: f(&body, "lat"),
                longitude: f(&body, "lon"),
                provider: provider.to_string(),
            }
        }
        "ipapi.co" => GeoInfo {
            ip: s(&body, "ip"),
            country: s(&body, "country_name"),
            country_code: s(&body, "country_code"),
            region: s(&body, "region"),
            city: s(&body, "city"),
            isp: s(&body, "org"),
            timezone: s(&body, "timezone"),
            latitude: f(&body, "latitude"),
            longitude: f(&body, "longitude"),
            provider: provider.to_string(),
        },
        "ipwho.is" => GeoInfo {
            ip: s(&body, "ip"),
            country: s(&body, "country"),
            country_code: s(&body, "country_code"),
            region: s(&body, "region"),
            city: s(&body, "city"),
            isp: body.get("connection").and_then(|c| c.get("isp")).and_then(|x| x.as_str()).unwrap_or("").to_string(),
            timezone: body.get("timezone").and_then(|t| t.get("id")).and_then(|x| x.as_str()).unwrap_or("").to_string(),
            latitude: f(&body, "latitude"),
            longitude: f(&body, "longitude"),
            provider: provider.to_string(),
        },
        _ => GeoInfo {
            ip: s(&body, "query"),
            country: s(&body, "country"),
            country_code: s(&body, "countryCode"),
            region: String::new(),
            city: String::new(),
            isp: String::new(),
            timezone: String::new(),
            latitude: 0.0,
            longitude: 0.0,
            provider: provider.to_string(),
        },
    };
    // Every provider names the address it saw. Without one there is nothing to
    // trust in the rest of the object, so the next provider gets a turn.
    if info.ip.trim().is_empty() {
        anyhow::bail!("{provider}: answered without an IP");
    }
    Ok(info)
}

/// Map ISO-3166 alpha-2 to BCP-47 locale (coarse).
pub fn country_to_locale(cc: &str) -> &'static str {
    match cc.to_ascii_uppercase().as_str() {
        "US" => "en-US",
        "GB" | "UK" => "en-GB",
        "CA" => "en-CA",
        "AU" => "en-AU",
        "NZ" => "en-NZ",
        "IE" => "en-IE",
        "ZA" => "en-ZA",
        "IN" => "en-IN",
        "DE" => "de-DE",
        "AT" => "de-AT",
        "CH" => "de-CH",
        "FR" => "fr-FR",
        "BE" => "fr-BE",
        "ES" => "es-ES",
        "MX" => "es-MX",
        "AR" => "es-AR",
        "CO" => "es-CO",
        "CL" => "es-CL",
        "IT" => "it-IT",
        "NL" => "nl-NL",
        "PL" => "pl-PL",
        "BR" => "pt-BR",
        "PT" => "pt-PT",
        "RO" => "ro-RO",
        "RU" => "ru-RU",
        "BY" => "be-BY",
        "UA" => "uk-UA",
        "TR" => "tr-TR",
        "GR" => "el-GR",
        "CZ" => "cs-CZ",
        "SK" => "sk-SK",
        "HU" => "hu-HU",
        "SE" => "sv-SE",
        "FI" => "fi-FI",
        "NO" => "nb-NO",
        "DK" => "da-DK",
        "BG" => "bg-BG",
        "HR" => "hr-HR",
        "SI" => "sl-SI",
        "RS" => "sr-RS",
        "IL" => "he-IL",
        "SA" | "AE" | "EG" => "ar-SA",
        "ID" => "id-ID",
        "MY" => "ms-MY",
        "PH" => "fil-PH",
        "VN" => "vi-VN",
        "TH" => "th-TH",
        "CN" => "zh-CN",
        "HK" => "zh-HK",
        "TW" => "zh-TW",
        "JP" => "ja-JP",
        "KR" => "ko-KR",
        _ => "en-US",
    }
}

// ---- Test history ----

/// One observation of a proxy's exit state; same-IP consecutive entries collapse.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSnapshot {
    pub first_seen: String,
    pub last_seen: String,
    pub ip: String,
    pub country_code: String,
    pub country: String,
    pub region: String,
    pub city: String,
    pub isp: String,
    pub timezone: String,
    pub latitude: f64,
    pub longitude: f64,
    pub tcp_ms: Option<u128>,
    pub udp_ms: Option<u128>,
    pub udp_error: Option<String>,
    /// Why there is no geo on this snapshot. Without it a failed probe reads
    /// as "no country", and the profile quietly launches on the host's clock.
    #[serde(default)]
    pub geo_error: Option<String>,
    pub provider: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct HistoryStore {
    #[serde(default)]
    by_proxy: HashMap<String, Vec<TestSnapshot>>,
}

fn history_path() -> Result<PathBuf> {
    Ok(store::user_files_root()?.join("proxies-history.json"))
}

fn load_history() -> Result<HistoryStore> {
    let path = history_path()?;
    if !path.exists() {
        return Ok(HistoryStore::default());
    }
    let body = fs::read_to_string(&path)?;
    // History is a log, not user data: an unreadable file just starts over.
    Ok(serde_json::from_str(&body).unwrap_or_default())
}

fn save_history(s: &HistoryStore) -> Result<()> {
    let body = serde_json::to_string_pretty(s)?;
    write_atomic(&history_path()?, body.as_bytes())
}

/// Persist a test result; same-IP entries collapse, capped at 50 per proxy.
fn record_test(proxy_id: &str, mut snap: TestSnapshot) -> Result<TestSnapshot> {
    if proxy_id.is_empty() {
        if snap.first_seen.is_empty() {
            snap.first_seen = snap.last_seen.clone();
        }
        return Ok(snap);
    }
    let _g = store_guard();
    let mut hs = load_history()?;
    let entries = hs.by_proxy.entry(proxy_id.into()).or_default();
    if let Some(last) = entries.last_mut() {
        if !snap.ip.is_empty() && last.ip == snap.ip {
            last.last_seen = snap.last_seen.clone();
            last.tcp_ms = snap.tcp_ms;
            last.udp_ms = snap.udp_ms;
            last.udp_error = snap.udp_error.clone();
            let out = last.clone();
            save_history(&hs)?;
            return Ok(out);
        }
    }
    if snap.first_seen.is_empty() {
        snap.first_seen = snap.last_seen.clone();
    }
    entries.push(snap.clone());
    if entries.len() > 50 {
        let drop = entries.len() - 50;
        entries.drain(..drop);
    }
    save_history(&hs)?;
    Ok(snap)
}

pub fn history(proxy_id: &str) -> Result<Vec<TestSnapshot>> {
    let hs = load_history()?;
    Ok(hs.by_proxy.get(proxy_id).cloned().unwrap_or_default())
}

pub fn latest_test(proxy_id: &str) -> Option<TestSnapshot> {
    load_history()
        .ok()
        .and_then(|hs| hs.by_proxy.get(proxy_id).and_then(|v| v.last().cloned()))
}

fn unix_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let s = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("@{s}")
}

/// Run TCP + UDP + geo, persist into history, auto-fill country tag.
pub async fn full_test(entry: &ProxyEntry) -> Result<TestSnapshot> {
    let now = unix_now();
    let entry = &effective_live(entry).await;

    // The three checks do not depend on each other: one wait, not their sum.
    let want_udp = matches!(entry.kind, ProxyKind::Socks5);
    let (tcp_res, udp_res, geo_res) = tokio::join!(
        probe(entry),
        async { if want_udp { Some(probe_udp(entry).await) } else { None } },
        geo_check(entry, None)
    );
    // What was just learned is what launching a profile trusts from now on; a proxy that
    // failed is forgotten, so the next open looks again instead of relying on an old answer.
    match (&tcp_res, &geo_res) {
        (Ok(_), Ok(g)) => {
            remember_geo(entry, g);
            if let Some(u) = &udp_res {
                remember_udp(entry, &u.as_ref().map(|ms| *ms).map_err(|e| e.to_string()));
            }
        }
        _ => forget_verified(&entry.id),
    }

    // TCP failure → zero geo so snapshot reads "Failed, no IP".
    let tcp_failed = tcp_res.is_err();
    let geo_error = match (&geo_res, tcp_failed) {
        (_, true) => tcp_res.as_ref().err().map(|e| format!("proxy unreachable: {e}")),
        (Err(e), false) => Some(e.to_string()),
        (Ok(_), false) => None,
    };
    let (ip, country_code, country, region, city, isp, tz, lat, lng, provider) =
        match (&geo_res, tcp_failed) {
            (Ok(g), false) => (
                g.ip.clone(), g.country_code.clone(), g.country.clone(),
                g.region.clone(), g.city.clone(), g.isp.clone(),
                g.timezone.clone(), g.latitude, g.longitude, g.provider.clone(),
            ),
            _ => (String::new(), String::new(), String::new(),
                  String::new(), String::new(), String::new(),
                  String::new(), 0.0, 0.0, String::new()),
        };

    let snap = TestSnapshot {
        first_seen: String::new(),
        last_seen: now,
        ip,
        country_code,
        country,
        region,
        city,
        isp,
        timezone: tz,
        latitude: lat,
        longitude: lng,
        tcp_ms: tcp_res.ok(),
        udp_ms: udp_res
            .as_ref()
            .and_then(|r| r.as_ref().ok().copied()),
        udp_error: udp_res
            .as_ref()
            .and_then(|r| r.as_ref().err().map(|e| e.to_string())),
        geo_error,
        provider,
    };

    let recorded = record_test(&entry.id, snap)?;

    // Keep the stored country tag in step with the latest successful test. It
    // used to fill only an empty tag, so whatever the first test said stuck
    // forever — a proxy first seen as VN kept the VN flag after later tests
    // correctly reported PH.
    if !recorded.country_code.is_empty() {
        let _g = store_guard();
        let mut store_data = load()?;
        if let Some(p) = store_data.proxies.iter_mut().find(|p| p.id == entry.id) {
            if p.country != recorded.country_code {
                p.country = recorded.country_code.clone();
                save(&store_data)?;
            }
        }
    }

    Ok(recorded)
}

/// Best-effort update of a stored proxy's country tag (no-op if unchanged or
/// unknown); used when a launch geolocates the proxy live.
pub fn set_country_tag(id: &str, country_code: &str) {
    if country_code.is_empty() {
        return;
    }
    let _g = store_guard();
    let Ok(mut store_data) = load() else { return };
    if let Some(p) = store_data.proxies.iter_mut().find(|p| p.id == id) {
        if p.country != country_code {
            p.country = country_code.to_string();
            let _ = save(&store_data);
        }
    }
}

/// Fallback country → IANA timezone for providers that omit timezone.
pub fn country_to_timezone(cc: &str) -> &'static str {
    match cc.to_ascii_uppercase().as_str() {
        "US" => "America/New_York",
        "CA" => "America/Toronto",
        "GB" | "UK" => "Europe/London",
        "DE" => "Europe/Berlin",
        "FR" => "Europe/Paris",
        "ES" => "Europe/Madrid",
        "IT" => "Europe/Rome",
        "NL" => "Europe/Amsterdam",
        "PL" => "Europe/Warsaw",
        "PT" => "Europe/Lisbon",
        "RO" => "Europe/Bucharest",
        "RU" => "Europe/Moscow",
        "UA" => "Europe/Kyiv",
        "TR" => "Europe/Istanbul",
        "GR" => "Europe/Athens",
        "CZ" => "Europe/Prague",
        "HU" => "Europe/Budapest",
        "SE" => "Europe/Stockholm",
        "FI" => "Europe/Helsinki",
        "NO" => "Europe/Oslo",
        "DK" => "Europe/Copenhagen",
        "CH" => "Europe/Zurich",
        "AT" => "Europe/Vienna",
        "BR" => "America/Sao_Paulo",
        "AR" => "America/Argentina/Buenos_Aires",
        "MX" => "America/Mexico_City",
        "AU" => "Australia/Sydney",
        "NZ" => "Pacific/Auckland",
        "IN" => "Asia/Kolkata",
        "ID" => "Asia/Jakarta",
        "MY" => "Asia/Kuala_Lumpur",
        "SG" => "Asia/Singapore",
        "TH" => "Asia/Bangkok",
        "VN" => "Asia/Ho_Chi_Minh",
        "CN" => "Asia/Shanghai",
        "HK" => "Asia/Hong_Kong",
        "TW" => "Asia/Taipei",
        "JP" => "Asia/Tokyo",
        "KR" => "Asia/Seoul",
        "IL" => "Asia/Jerusalem",
        "SA" => "Asia/Riyadh",
        "AE" => "Asia/Dubai",
        _ => "UTC",
    }
}

#[cfg(test)]
mod engine_arg_tests {
    use super::*;

    fn p(kind: ProxyKind, user: &str, pass: &str) -> ProxyEntry {
        ProxyEntry { id: String::new(), name: String::new(), kind, host: "h.example".into(), port: 55159, username: user.into(), password: pass.into(), country: String::new(), notes: String::new() }
    }

    /// The engine reads the login out of the argument as written and does not
    /// decode percent escapes; encoding it made a password like `x==` arrive as
    /// `x%3D%3D` and be refused (SOCKS5 no network, HTTP a sign-in prompt).
    #[test]
    fn the_engine_gets_the_login_as_written_and_http_clients_get_a_proper_url() {
        let e = p(ProxyKind::Socks5, "user01", "c2VjcmV0cGFzcw==");
        assert_eq!(e.to_engine_arg(), "socks5://user01:c2VjcmV0cGFzcw==@h.example:55159");
        assert_eq!(e.to_proxy_server_arg(), "socks5://user01:c2VjcmV0cGFzcw%3D%3D@h.example:55159");
        let odd = p(ProxyKind::Http, "u", "p+ss @:/#?%");
        assert_eq!(odd.to_engine_arg(), "http://u:p+ss @:/#?%@h.example:55159");
        assert_eq!(p(ProxyKind::Http, "", "").to_engine_arg(), "http://h.example:55159");
    }
}

#[cfg(test)]
mod effective_kind_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn entry(kind: ProxyKind, port: u16) -> ProxyEntry {
        ProxyEntry { id: String::new(), name: String::new(), kind, host: "127.0.0.1".into(), port, username: "u".into(), password: "p".into(), country: String::new(), notes: String::new() }
    }

    /// A listener that answers like `what`: a plain HTTP proxy (407 to an unauthenticated
    /// CONNECT), a TLS-only one (an alert record, then closes), or one that just hangs up.
    async fn fake(what: &'static str) -> u16 {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let Ok((mut c, _)) = l.accept().await else { break };
                tokio::spawn(async move {
                    let mut b = [0u8; 256];
                    let _ = c.read(&mut b).await;
                    match what {
                        "http" => { let _ = c.write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\nContent-Length: 0\r\n\r\n").await; }
                        "tls" => { let _ = c.write_all(&[0x15, 0x03, 0x03, 0x00, 0x02, 0x02, 0x0a]).await; }
                        // A SOCKS5 server picking "username/password" for the greeting.
                        "socks" => { let _ = c.write_all(&[0x05, 0x02]).await; }
                        _ => {}
                    }
                });
            }
        });
        port
    }

    /// A provider's "HTTPS proxy" that is really a plain HTTP one is used as HTTP;
    /// a genuine TLS proxy, a dead one, and every other kind are left alone.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_https_label_on_a_plain_http_proxy_is_used_as_http() {
        let plain = fake("http").await;
        assert_eq!(effective(&entry(ProxyKind::Https, plain)).await.kind, ProxyKind::Http);
        assert_eq!(effective(&entry(ProxyKind::Https, fake("tls").await)).await.kind, ProxyKind::Https, "a real TLS proxy stays HTTPS");
        assert_eq!(effective(&entry(ProxyKind::Https, fake("silent").await)).await.kind, ProxyKind::Https);
        let dead = { let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap(); l.local_addr().unwrap().port() };
        assert_eq!(effective(&entry(ProxyKind::Https, dead)).await.kind, ProxyKind::Https, "unreachable: nothing to conclude");
        assert_eq!(effective(&entry(ProxyKind::Http, plain)).await.kind, ProxyKind::Http);
    }

    /// A pasted `host:port:user:pass` list is read as SOCKS5, so an HTTP-only provider's
    /// proxies arrive labelled SOCKS5 and never answer the SOCKS handshake. They are used
    /// as HTTP; a real SOCKS5 server, a silent one and a dead one are left as labelled.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_socks5_label_on_a_plain_http_proxy_is_used_as_http() {
        // Answers the greeting with an HTTP error line.
        assert_eq!(effective(&entry(ProxyKind::Socks5, fake("http").await)).await.kind, ProxyKind::Http);
        // Says nothing to the greeting, answers a CONNECT: the tunproxy shape.
        assert_eq!(effective(&entry(ProxyKind::Socks5, fake_waiting().await)).await.kind, ProxyKind::Http);
        // A real SOCKS5 server stays SOCKS5, and so do a silent and an unreachable port.
        assert_eq!(effective(&entry(ProxyKind::Socks5, fake("socks").await)).await.kind, ProxyKind::Socks5);
        assert_eq!(effective(&entry(ProxyKind::Socks5, fake("silent").await)).await.kind, ProxyKind::Socks5);
        let dead = { let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap(); l.local_addr().unwrap().port() };
        assert_eq!(effective(&entry(ProxyKind::Socks5, dead)).await.kind, ProxyKind::Socks5);
    }

    /// The other direction too: an HTTP label on a SOCKS5 port is used as SOCKS5, a correctly
    /// labelled proxy is left alone, and what is found is written back to the saved proxy so the
    /// list shows the real type.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_http_label_on_a_socks5_port_is_used_as_socks5_and_the_saved_kind_is_corrected() {
        let _root = crate::cloud_sync::TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(effective(&entry(ProxyKind::Http, fake("socks").await)).await.kind, ProxyKind::Socks5);
        assert_eq!(effective(&entry(ProxyKind::Https, fake("socks").await)).await.kind, ProxyKind::Socks5);
        assert_eq!(effective(&entry(ProxyKind::Http, fake("http").await)).await.kind, ProxyKind::Http, "a right label stays");
        assert_eq!(effective(&entry(ProxyKind::Http, fake("silent").await)).await.kind, ProxyKind::Http, "silence concludes nothing");

        let mut mislabelled = entry(ProxyKind::Socks5, fake("http").await);
        mislabelled.name = "tunproxy-test".into();
        let saved = upsert(mislabelled).unwrap();
        assert_eq!(effective(&saved).await.kind, ProxyKind::Http);
        assert_eq!(get(&saved.id).unwrap().unwrap().kind, ProxyKind::Http, "the saved proxy now carries its real type");
        delete(&saved.id).unwrap();
    }

    /// Not run by default: `HIR_PROXY_LIST` holds `host:port:user:pass` lines read as SOCKS5, as a
    /// pasted list is, and each one must pass the Test button's probe once `effective` has had its say.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore]
    async fn live_list_from_the_environment_passes_the_probe() {
        let list = std::env::var("HIR_PROXY_LIST").expect("set HIR_PROXY_LIST");
        let mut bad = Vec::new();
        for e in parse_bulk(&list, ProxyKind::Socks5) {
            let used = effective(&e).await;
            match probe(&used).await {
                Ok(ms) => eprintln!("{}:{} labelled {} used as {} -> OK {ms} ms", e.host, e.port, e.kind.as_str(), used.kind.as_str()),
                Err(err) => { eprintln!("{}:{} -> FAIL {err}", e.host, e.port); bad.push(e.host.clone()); }
            }
        }
        assert!(bad.is_empty(), "failed: {bad:?}");
    }

    /// Reads the SOCKS greeting without a word, then answers an HTTP CONNECT on the next
    /// connection — what an HTTP proxy does with bytes it cannot parse.
    async fn fake_waiting() -> u16 {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let Ok((mut c, _)) = l.accept().await else { break };
                tokio::spawn(async move {
                    let mut b = [0u8; 256];
                    let n = c.read(&mut b).await.unwrap_or(0);
                    if b[..n].starts_with(b"CONNECT") {
                        let _ = c.write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\nContent-Length: 0\r\n\r\n").await;
                    } else {
                        // Hold the connection open, like a proxy waiting for a request line.
                        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    }
                });
            }
        });
        port
    }
}


#[cfg(test)]
mod ttl_cache_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    /// Everyone who asks while the first answer is still on its way gets that answer: three
    /// callers, one request. This is what lets a prefetch and the two later lookups in a launch
    /// share a single trip through the proxy.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn askers_that_arrive_together_share_one_request() {
        let cache: Arc<TtlCache<u32>> = Arc::new(TtlCache::new());
        let calls = Arc::new(AtomicUsize::new(0));
        let ask = |cache: Arc<TtlCache<u32>>, calls: Arc<AtomicUsize>| async move {
            cache
                .get_or("k", |_| Duration::from_secs(60), || async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    7
                })
                .await
        };
        let (a, b, c) = tokio::join!(
            ask(cache.clone(), calls.clone()),
            ask(cache.clone(), calls.clone()),
            ask(cache.clone(), calls.clone())
        );
        assert_eq!((a, b, c), (7, 7, 7));
        assert_eq!(calls.load(Ordering::SeqCst), 1, "one request, not three");
    }

    /// An answer is kept for as long as its own rule says, and asked for again after: a success
    /// for a while, a failure for a shorter while (zero here: forgotten at once).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_answer_is_remembered_for_its_time_and_a_failure_can_be_forgotten_at_once() {
        let cache: TtlCache<Result<u32, String>> = TtlCache::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let ttl = |r: &Result<u32, String>| Duration::from_secs(if r.is_ok() { 60 } else { 0 });
        let ask = |ok: bool| {
            let calls = calls.clone();
            cache.get_or("k", ttl, move || async move {
                calls.fetch_add(1, Ordering::SeqCst);
                if ok { Ok(1) } else { Err("down".into()) }
            })
        };
        assert_eq!(ask(true).await, Ok(1));
        assert_eq!(ask(true).await, Ok(1));
        assert_eq!(calls.load(Ordering::SeqCst), 1, "a success is remembered");

        let other: TtlCache<Result<u32, String>> = TtlCache::new();
        let ask_bad = || {
            let calls = calls.clone();
            other.get_or("k", ttl, move || async move {
                calls.fetch_add(1, Ordering::SeqCst);
                Err::<u32, String>("down".into())
            })
        };
        assert!(ask_bad().await.is_err());
        assert!(ask_bad().await.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 3, "a failure with no time to live is asked again");

        // Different proxies never share an answer.
        let a = proxy_key(&ProxyEntry { id: String::new(), name: String::new(), kind: ProxyKind::Http, host: "h1".into(), port: 1, username: "u".into(), password: "p".into(), country: String::new(), notes: String::new() });
        let b = proxy_key(&ProxyEntry { id: String::new(), name: String::new(), kind: ProxyKind::Http, host: "h2".into(), port: 1, username: "u".into(), password: "p".into(), country: String::new(), notes: String::new() });
        assert_ne!(a, b);
        assert_eq!(a, "http://u@h1:1", "who and where, never the password");
    }
}

#[cfg(test)]
mod verified_tests {
    use super::*;

    /// A proxy on `port` of this machine, with its own id so tests never share a record.
    fn entry(kind: ProxyKind, port: u16) -> ProxyEntry {
        ProxyEntry {
            id: uuid::Uuid::new_v4().to_string(),
            name: "t".into(),
            kind,
            host: "127.0.0.1".into(),
            port,
            username: "u".into(),
            password: "p".into(),
            country: String::new(),
            notes: String::new(),
        }
    }

    fn geo(ip: &str) -> GeoInfo {
        GeoInfo {
            ip: ip.into(),
            country: "Viet Nam".into(),
            country_code: "VN".into(),
            region: String::new(),
            city: "Hanoi".into(),
            isp: String::new(),
            timezone: "Asia/Ho_Chi_Minh".into(),
            latitude: 21.0,
            longitude: 105.8,
            provider: "test".into(),
        }
    }

    /// A port nothing listens on: any attempt to reach it fails at once.
    fn dead_port() -> u16 {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    }

    /// The point of it: a proxy that has been checked is not asked again. Its address here is a
    /// dead port, so a lookup through it would fail — the answer can only have come from the record.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_proxy_checked_once_is_answered_from_its_record_and_never_contacted() {
        let _g = crate::cloud_sync::TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let e = entry(ProxyKind::Socks5, dead_port());
        remember_geo(&e, &geo("203.0.113.7"));
        remember_udp(&e, &Ok(42));
        let started = std::time::Instant::now();
        let g = geo_check_cached(&e).await.expect("answered from the record");
        let udp = probe_udp_cached(&e).await;
        let kept = effective(&e).await;
        assert_eq!((g.ip.as_str(), g.timezone.as_str()), ("203.0.113.7", "Asia/Ho_Chi_Minh"));
        assert_eq!(udp, Ok(42));
        assert_eq!(kept.kind, ProxyKind::Socks5, "the kind it was checked as");
        assert!(started.elapsed() < std::time::Duration::from_millis(500), "nothing was sent anywhere: {:?}", started.elapsed());

        // A relay that was found not to work stays "not working" without trying again.
        remember_udp(&e, &Err("no relay".into()));
        assert_eq!(probe_udp_cached(&e).await, Err("no relay".to_string()));
        forget_verified(&e.id);
    }

    /// What was learned holds for the proxy as configured. Editing the connection (type, host,
    /// port, login) makes the record not apply — it is checked again; editing its name, country
    /// tag or notes does not.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn changing_the_proxy_makes_it_be_checked_again_but_renaming_it_does_not() {
        let _g = crate::cloud_sync::TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // A "proxy" that hangs up on everyone at once. (A closed port is slow to refuse on Windows,
        // and a lookup tries three providers.)
        let hangup = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = hangup.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                if let Ok((sock, _)) = hangup.accept().await {
                    drop(sock);
                }
            }
        });
        let e = entry(ProxyKind::Http, port);
        remember_geo(&e, &geo("203.0.113.8"));
        assert!(geo_record(&e).is_some());

        let mut renamed = e.clone();
        renamed.name = "another name".into();
        renamed.country = "US".into();
        renamed.notes = "a note".into();
        assert!(geo_record(&renamed).is_some(), "name, country and notes are not the connection");

        let mut host = e.clone(); host.host = "localhost".into();
        let mut port = e.clone(); port.port = e.port.wrapping_add(1).max(1025);
        let mut user = e.clone(); user.username = "other".into();
        let mut pass = e.clone(); pass.password = "other".into();
        let mut kind = e.clone(); kind.kind = ProxyKind::Socks5;
        for (what, changed) in [("host", host), ("port", port), ("user", user), ("password", pass), ("type", kind)] {
            assert!(geo_record(&changed).is_none(), "a changed {what} must not reuse the old answer");
            assert_ne!(proxy_sig(&changed), proxy_sig(&e), "{what}");
        }

        // So the first open after an edit really asks — here the (dead) proxy, which fails fast.
        let mut edited = e.clone();
        edited.password = "changed".into();
        let t = std::time::Instant::now();
        assert!(geo_check_cached(&edited).await.is_err());
        assert!(t.elapsed() < std::time::Duration::from_secs(20), "{:?}", t.elapsed());
        forget_verified(&e.id);
    }

    /// An answer is trusted for a week. Past 12 hours it is still used (the profile opens at once)
    /// and re-checked in the background; past a week it is not used at all.
    #[test]
    fn a_record_is_used_while_it_is_young_and_dropped_when_it_is_a_week_old() {
        let _g = crate::cloud_sync::TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let e = entry(ProxyKind::Http, 1080);
        update_verified(&e, |v| { v.geo = Some(geo("1.1.1.1")); v.geo_at = now_secs() - 3 * 3600; });
        let (_, age) = geo_record(&e).expect("three hours old: used");
        assert!(age < REFRESH_AFTER_SECS, "{age}");

        update_verified(&e, |v| v.geo_at = now_secs() - 20 * 3600);
        let (_, age) = geo_record(&e).expect("twenty hours old: still used");
        assert!(age > REFRESH_AFTER_SECS && age < EXPIRES_AFTER_SECS, "{age}: due for a background refresh");

        update_verified(&e, |v| v.geo_at = now_secs() - 8 * 24 * 3600);
        assert!(geo_record(&e).is_none(), "a week old: ask again");
        forget_verified(&e.id);
    }

    /// Deleting a proxy deletes what was known about it.
    #[test]
    fn deleting_a_proxy_forgets_what_was_verified_about_it() {
        let _g = crate::cloud_sync::TEST_ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let saved = upsert(entry(ProxyKind::Http, 3128)).unwrap();
        remember_geo(&saved, &geo("198.51.100.1"));
        assert!(geo_record(&saved).is_some());
        delete(&saved.id).unwrap();
        assert!(geo_record(&saved).is_none());
    }
}
