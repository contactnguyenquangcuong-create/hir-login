// ProxyShard billing/user API client (https://user-api.proxyshard.com).
//
// Every /user/api/ path takes `Authorization: Bearer <API_KEY>`.  The key
// lives in its own `psapi.json` (see store::psapi_path) so the Settings page,
// which round-trips the whole Settings struct, can't accidentally wipe it.
//
// `call()` is a thin generic wrapper: it injects the bearer key + base URL,
// sends JSON, and unwraps the API's `{ success:false, message }` error shape
// into an anyhow error so the UI shows the server's own wording.

use crate::proxy::{self, ProxyEntry, ProxyKind};
use crate::store;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;

const BASE: &str = "https://user-api.proxyshard.com";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PsConfig {
    #[serde(default)]
    pub api_key: String,
}

pub fn load() -> Result<PsConfig> {
    let path = store::psapi_path()?;
    if !path.exists() {
        return Ok(PsConfig::default());
    }
    let body = fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&body).unwrap_or_default())
}

fn save(c: &PsConfig) -> Result<()> {
    fs::write(store::psapi_path()?, serde_json::to_string_pretty(c)?)?;
    Ok(())
}

pub fn get_key() -> Result<String> {
    Ok(load()?.api_key)
}

pub fn set_key(key: String) -> Result<()> {
    let mut c = load()?;
    c.api_key = key.trim().to_string();
    save(&c)
}

/// Authenticated JSON request against the billing API.
/// `method` is one of GET / POST / PATCH / DELETE.
pub async fn call(
    method: &str,
    path: &str,
    query: &[(String, String)],
    body: Option<Value>,
) -> Result<Value> {
    let key = get_key()?;
    if key.is_empty() {
        return Err(anyhow!("ProxyShard API key not set"));
    }
    let url = format!("{BASE}{path}");
    let cli = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    let mut req = match method {
        "GET" => cli.get(&url),
        "POST" => cli.post(&url),
        "PATCH" => cli.patch(&url),
        "DELETE" => cli.delete(&url),
        other => return Err(anyhow!("unsupported method {other}")),
    };
    req = req.bearer_auth(&key);
    if !query.is_empty() {
        req = req.query(query);
    }
    if let Some(b) = body {
        req = req.json(&b);
    } else if matches!(method, "POST" | "PATCH") {
        // Body-less POST/PATCH: send an empty JSON object so reqwest emits a
        // real Content-Length (+ Content-Type). A zero-length body can be sent
        // with no Content-Length at all, which the server (actix) rejects with
        // "no Content-Length specified … invalid Header provided".
        req = req.json(&serde_json::json!({}));
    }

    let resp = req.send().await.context("request to ProxyShard failed")?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);

    if !status.is_success() {
        if status.as_u16() == 401 {
            return Err(anyhow!("Unauthorized — check your API key"));
        }
        let msg = value
            .get("message")
            .and_then(|m| m.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| if text.is_empty() { status.as_str() } else { &text });
        return Err(anyhow!("{msg}"));
    }
    Ok(value)
}

/// Fetch the active proxies for a Datacenter/ISP order and persist them into
/// the local proxy list as `kind` ("socks5" | "http"). Returns the number of
/// new proxies actually added (existing host:port:user pairs are skipped).
pub async fn import_order_proxies(order_id: i64, kind: String) -> Result<usize> {
    let q = vec![("order_id".to_string(), order_id.to_string())];
    let resp = call("GET", "/user/api/proxies/active", &q, None).await?;

    let data = resp
        .get("data")
        .and_then(|d| d.as_array())
        .cloned()
        .unwrap_or_default();
    let tag = resp
        .get("order_tag")
        .and_then(|t| t.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let label = tag.unwrap_or_else(|| format!("order {order_id}"));

    let use_http = kind == "http";
    let s = |v: &Value, k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let port_of = |v: &Value, k: &str| v.get(k).and_then(|x| x.as_u64()).unwrap_or(0) as u16;

    let mut entries = Vec::new();
    for it in data {
        let ip = s(&it, "ip");
        if ip.is_empty() {
            continue;
        }
        let (proxy_kind, port) = if use_http {
            (ProxyKind::Http, port_of(&it, "http_port"))
        } else {
            (ProxyKind::Socks5, port_of(&it, "socks_port"))
        };
        if port == 0 {
            continue;
        }
        entries.push(ProxyEntry {
            id: String::new(),
            name: format!("{label} · {ip}"),
            kind: proxy_kind,
            host: ip,
            port,
            username: s(&it, "username"),
            password: s(&it, "password"),
            country: String::new(),
            notes: format!("ProxyShard order {order_id}"),
        });
    }

    proxy::bulk_save(entries)
}

/// One proxy line for a project's `proxy.generate` block. Deliberately not via
/// `import_order_proxies`: this must not land in the operator's proxy library.
#[cfg_attr(not(feature = "automation"), allow(dead_code))]
pub async fn pick_proxy(order: Option<&str>, country: Option<&str>) -> anyhow::Result<String> {
    let mut q: Vec<(String, String)> = Vec::new();
    if let Some(o) = order.filter(|o| !o.is_empty()) {
        q.push(("order_id".into(), o.to_string()));
    }
    let resp = call("GET", "/user/api/proxies/active", &q, None).await?;
    let data = resp
        .get("data")
        .and_then(|d| d.as_array())
        .cloned()
        .unwrap_or_default();

    let s = |v: &Value, k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let candidates: Vec<&Value> = data
        .iter()
        .filter(|it| !s(it, "ip").is_empty())
        .filter(|it| match country {
            Some(cc) if !cc.is_empty() => s(it, "country").eq_ignore_ascii_case(cc),
            _ => true,
        })
        .collect();
    if candidates.is_empty() {
        return Err(anyhow!(match country {
            Some(cc) if !cc.is_empty() => format!("no active proxy in {cc}"),
            _ => "no active proxies on that order".to_string(),
        }));
    }
    // Pick at random: always the first would put a whole fleet on one address.
    let idx = (uuid::Uuid::new_v4().as_bytes()[0] as usize) % candidates.len();
    let it = candidates[idx];
    let port = it
        .get("socks_port")
        .or_else(|| it.get("port"))
        .and_then(|x| x.as_u64())
        .unwrap_or(0);
    let user = s(it, "login");
    let pass = s(it, "password");
    let ip = s(it, "ip");
    Ok(if user.is_empty() {
        format!("{ip}:{port}")
    } else {
        format!("{ip}:{port}:{user}:{pass}")
    })
}

// ---- Residential sessions ----
// Built only by the automation runner, hence the feature gate on each item.

/// Tier to (`plan` for the session login, `proxy_type` for the account endpoints).
/// Crossing the pair silently hands out a session on the wrong plan.
#[cfg(feature = "automation")]
fn resi_tokens(tier: &str) -> Result<(&'static str, &'static str)> {
    match tier.trim().to_lowercase().as_str() {
        "standart" | "standard" => Ok(("limited", "standart")),
        "premium" => Ok(("premium", "premium")),
        "unmetered" | "unlimited" => Ok(("unlimited", "unlimited")),
        other => Err(anyhow!("unknown residential plan \"{other}\"")),
    }
}

/// Relay gateways and their ports, the same ones the generator card offers.
#[cfg(feature = "automation")]
const RESI_RELAYS: [&str; 3] = [
    "relay-eu.proxyshard.com",
    "relay-ru.proxyshard.net",
    "relay-ua.proxyshard.com",
];

/// What a project asked for.
#[cfg(feature = "automation")]
pub struct ResiRequest<'a> {
    pub tier: &'a str,
    pub country: Option<&'a str>,
    pub region: Option<&'a str>,
    pub city: Option<&'a str>,
    /// macos | windows | android | linux | ios. Premium only.
    pub os: Option<&'a str>,
    /// ISP code (carrier) to pin exits to. Premium only; the code comes from
    /// resi_isps() for the chosen country/region/city.
    pub isp: Option<&'a str>,
    /// Sticky pins one exit address for the session; dynamic takes a new one.
    pub sticky: bool,
    /// "static" holds the session until it is dropped; anything else, including
    /// nothing, turns over seconds after idle. Independent of sticky/dynamic.
    pub session_mode: Option<&'a str>,
    pub http: bool,
    /// Empty for the first relay.
    pub relay: Option<&'a str>,
}

#[cfg(feature = "automation")]
/// Lists the ISPs for a country/region/city as `{count, results:[{code,name,...}]}`.
/// Premium only: the provider keeps no ISP directory for the other tiers.
pub async fn resi_isps(tier: &str, country: &str, region: &str, city: &str) -> Result<Value> {
    let (_plan, proxy_type) = resi_tokens(tier)?;
    let q = vec![
        ("proxy_type".to_string(), proxy_type.to_string()),
        ("country_code".to_string(), country.trim().to_lowercase()),
        ("region_code".to_string(), region.trim().to_lowercase()),
        ("city_code".to_string(), city.trim().to_lowercase()),
    ];
    call("GET", "/user/api/proxies/isps", &q, None)
        .await
        .context("asking ProxyShard for the ISP list")
}

/// Builds one residential session and hands back a proxy ready to bind. Fails
/// fast when the plan has no password or no traffic left, not page by page.
#[cfg(feature = "automation")]
pub async fn residential_proxy(req: ResiRequest<'_>) -> Result<ProxyEntry> {
    let (plan, proxy_type) = resi_tokens(req.tier)?;

    // OS and ISP are premium-only: a cheaper plan would come back without them
    // and say nothing, so name the mistake here.
    let os = req.os.map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty() && s != "unset");
    let isp = req.isp.map(|s| s.trim().to_string()).filter(|s| !s.is_empty() && s != "unset");
    if isp.is_some() && plan != "premium" {
        return Err(anyhow!(
            "an ISP filter can only be pinned on the Premium residential plan"
        ));
    }
    if let Some(os) = os.as_deref() {
        if plan != "premium" {
            return Err(anyhow!(
                "the OS \"{os}\" can only be pinned on the Premium residential plan"
            ));
        }
        const KNOWN: [&str; 5] = ["macos", "windows", "android", "linux", "ios"];
        if !KNOWN.contains(&os) {
            return Err(anyhow!("unknown OS \"{os}\" — one of macos, windows, android, linux, ios"));
        }
    }

    let profile = call(
        "GET",
        "/user/api/proxies/profile",
        &[("proxy_type".into(), proxy_type.to_string())],
        None,
    )
    .await
    .context("asking ProxyShard for the residential profile")?;

    let password = profile
        .get("proxy_password")
        .or_else(|| profile.get("password"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if password.is_empty() {
        return Err(anyhow!(
            "no residential password on this account for the {proxy_type} plan"
        ));
    }

    // Unmetered is a flat plan and reports no meter, so there is nothing to
    // check; the other two stop here rather than burn a pass on dead sessions.
    if proxy_type != "unlimited" {
        let left = profile.get("data_remain").and_then(|v| v.as_f64()).unwrap_or(0.0);
        if left <= 0.0 {
            return Err(anyhow!(
                "no residential traffic left on the {proxy_type} plan — top it up and run again"
            ));
        }
    }

    let mut login = format!("plan-{plan}");
    if let Some(c) = req.country.map(str::trim).filter(|s| !s.is_empty()) {
        login.push_str(&format!("-country-{}", c.to_lowercase()));
    }
    if let Some(r) = req.region.map(str::trim).filter(|s| !s.is_empty()) {
        login.push_str(&format!("-region-{r}"));
    }
    if let Some(c) = req.city.map(str::trim).filter(|s| !s.is_empty()) {
        login.push_str(&format!("-city-{c}"));
    }
    if let Some(isp) = isp.as_deref() {
        login.push_str(&format!("-isp-{isp}"));
    }
    if req.sticky {
        // One address for the whole session. Random per call, so two profiles
        // in one run never share an exit.
        let sid = uuid::Uuid::new_v4().simple().to_string()[..8].to_string();
        login.push_str(&format!("-sid-{sid}"));
    }
    if let Some(os) = os.as_deref() {
        login.push_str(&format!("-os-{os}"));
    }
    // The token is the five-second mode; static is its absence. Same rule as the
    // generator card, so both paths build the same session.
    let static_mode = req
        .session_mode
        .map(|m| m.trim().eq_ignore_ascii_case("static"))
        .unwrap_or(false);
    if !static_mode {
        login.push_str("-session_mode-2");
    }

    let host = req
        .relay
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(RESI_RELAYS[0])
        .to_string();
    let (kind, port) = if req.http {
        (ProxyKind::Http, 8080u16)
    } else {
        (ProxyKind::Socks5, 1080u16)
    };

    Ok(ProxyEntry {
        id: String::new(),
        name: format!("resi {plan}"),
        kind,
        host,
        port,
        username: login,
        password,
        country: req
            .country
            .map(|c| c.trim().to_uppercase())
            .unwrap_or_default(),
        notes: format!("ProxyShard residential ({plan})"),
    })
}
