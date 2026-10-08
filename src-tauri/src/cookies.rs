// Cookie import/export for ShardX profiles (Chromium Cookies sqlite v24).
// v10 blob = "v10" + cipher; plaintext = SHA256(host) + value.
//   macOS: AES-128-CBC,  key = PBKDF2(mock_password, saltysalt, 1003)
//   Linux: AES-128-CBC,  key = PBKDF2(peanuts,       saltysalt, 1)
//   Win:   AES-256-GCM,  key = DPAPI-unwrapped Local State os_crypt.encrypted_key

use crate::profile;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

// ---- AES-128-CBC cipher (macOS + Linux) ----
#[cfg(not(target_os = "windows"))]
use aes::Aes128;
#[cfg(not(target_os = "windows"))]
use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
#[cfg(not(target_os = "windows"))]
type Aes128CbcDec = cbc::Decryptor<Aes128>;
#[cfg(not(target_os = "windows"))]
type Aes128CbcEnc = cbc::Encryptor<Aes128>;
#[cfg(not(target_os = "windows"))]
const IV: [u8; 16] = [0x20; 16];

/// Tool-friendly cookie shape (httpOnly / sameSite camelCase aliases accepted).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cookie {
    pub domain: String,
    pub name: String,
    pub value: String,
    #[serde(default = "default_path")]
    pub path: String,
    /// Unix seconds; None = session cookie. Browser extensions (Cookie-Editor,
    /// EditThisCookie…) export this as `expirationDate` or `expiry`; Playwright uses
    /// -1 for a session cookie.
    #[serde(default, alias = "expirationDate", alias = "expiry")]
    pub expires: Option<f64>,
    /// Extension exports say `"session": true` instead of leaving the expiry out.
    #[serde(default, skip_serializing)]
    pub session: bool,
    #[serde(default)]
    pub secure: bool,
    #[serde(default, alias = "httpOnly")]
    pub http_only: bool,
    /// "Strict" | "Lax" | "None" | "unspecified" (case-insensitive).
    #[serde(default, alias = "sameSite")]
    pub same_site: Option<String>,
}

fn default_path() -> String {
    "/".to_string()
}

// Chromium time = µs since 1601-01-01.
const WIN_EPOCH_DELTA_SECS: i64 = 11_644_473_600;

fn chromium_to_unix_secs(micros: i64) -> f64 {
    micros as f64 / 1_000_000.0 - WIN_EPOCH_DELTA_SECS as f64
}
fn unix_to_chromium(secs: f64) -> i64 {
    ((secs + WIN_EPOCH_DELTA_SECS as f64) * 1_000_000.0) as i64
}
fn now_chromium() -> i64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    unix_to_chromium(now)
}

// ---- OSCrypt key + cipher (per OS) ----

/// Resolved OSCrypt key; cipher chosen per-OS at compile time.
struct Crypt {
    key: Vec<u8>,
}

impl Crypt {
    /// POSIX: fixed derivation; Windows: read/mint os_crypt.encrypted_key.
    fn open(udd: &Path) -> Result<Self> {
        Ok(Self {
            key: os_crypt_key(udd)?,
        })
    }

    fn decrypt(&self, encrypted: &[u8], plain: &str) -> String {
        // Legacy rows: value in `value` column, no v10 blob.
        if encrypted.len() < 3 || &encrypted[..3] != b"v10" {
            return plain.to_string();
        }
        match cipher_decrypt(&self.key, &encrypted[3..]) {
            Some(pt) => String::from_utf8_lossy(&strip_host_prefix(pt)).into_owned(),
            None => String::new(),
        }
    }

    /// The plaintext inside a "v10" blob, whatever sealed it. `strip_host`: cookies carry a
    /// 32-byte SHA256(host) prefix, saved passwords do not.
    fn decrypt_blob(&self, encrypted: &[u8], strip_host: bool) -> Option<Vec<u8>> {
        if encrypted.len() < 3 || &encrypted[..3] != b"v10" {
            return None;
        }
        let pt = cipher_decrypt(&self.key, &encrypted[3..])?;
        Some(if strip_host { strip_host_prefix(pt) } else { pt })
    }

    /// A password sealed the way Chromium seals it: "v10" + cipher(plaintext), no host prefix.
    fn encrypt_plain(&self, plaintext: &[u8]) -> Vec<u8> {
        let body = cipher_encrypt(&self.key, plaintext);
        let mut out = Vec::with_capacity(3 + body.len());
        out.extend_from_slice(b"v10");
        out.extend_from_slice(&body);
        out
    }

    fn encrypt(&self, host: &str, value: &str) -> Vec<u8> {
        // 32-byte SHA256(host) prefix per Chromium ≥130.
        let mut plaintext = Sha256::digest(host.as_bytes()).to_vec();
        plaintext.extend_from_slice(value.as_bytes());
        let body = cipher_encrypt(&self.key, &plaintext);
        let mut out = Vec::with_capacity(3 + body.len());
        out.extend_from_slice(b"v10");
        out.extend_from_slice(&body);
        out
    }
}

/// Strip the 32-byte SHA256(host) domain prefix.
fn strip_host_prefix(mut pt: Vec<u8>) -> Vec<u8> {
    if pt.len() >= 32 {
        pt.drain(0..32);
    }
    pt
}

// ---- macOS: mock_password ----
#[cfg(target_os = "macos")]
fn os_crypt_key(_udd: &Path) -> Result<Vec<u8>> {
    let mut key = [0u8; 16];
    pbkdf2::pbkdf2_hmac::<sha1::Sha1>(b"mock_password", b"saltysalt", 1003, &mut key);
    Ok(key.to_vec())
}

// ---- Linux: peanuts ----
#[cfg(target_os = "linux")]
fn os_crypt_key(_udd: &Path) -> Result<Vec<u8>> {
    let mut key = [0u8; 16];
    pbkdf2::pbkdf2_hmac::<sha1::Sha1>(b"peanuts", b"saltysalt", 1, &mut key);
    Ok(key.to_vec())
}

// ---- POSIX CBC ----
#[cfg(not(target_os = "windows"))]
fn cipher_decrypt(key: &[u8], body: &[u8]) -> Option<Vec<u8>> {
    let dec = Aes128CbcDec::new_from_slices(key, &IV).ok()?;
    dec.decrypt_padded_vec_mut::<Pkcs7>(body).ok()
}
#[cfg(not(target_os = "windows"))]
fn cipher_encrypt(key: &[u8], plaintext: &[u8]) -> Vec<u8> {
    let enc = Aes128CbcEnc::new_from_slices(key, &IV).expect("16-byte key/iv");
    enc.encrypt_padded_vec_mut::<Pkcs7>(plaintext)
}

// ---- Windows: DPAPI key + AES-256-GCM ----
#[cfg(target_os = "windows")]
fn os_crypt_key(udd: &Path) -> Result<Vec<u8>> {
    win::os_crypt_key(udd)
}
#[cfg(target_os = "windows")]
fn cipher_decrypt(key: &[u8], body: &[u8]) -> Option<Vec<u8>> {
    win::gcm_decrypt(key, body)
}
#[cfg(target_os = "windows")]
fn cipher_encrypt(key: &[u8], plaintext: &[u8]) -> Vec<u8> {
    win::gcm_encrypt(key, plaintext)
}

#[cfg(target_os = "windows")]
mod win {
    use aes_gcm::{
        aead::{Aead, KeyInit},
        Aes256Gcm, Key, Nonce,
    };
    use anyhow::{anyhow, Context, Result};
    use base64::{engine::general_purpose::STANDARD, Engine};
    use std::path::Path;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB,
    };
    use windows_sys::Win32::Foundation::LocalFree;

    const DPAPI_TAG: &[u8] = b"DPAPI";

    fn rand_bytes<const N: usize>() -> [u8; N] {
        let mut b = [0u8; N];
        getrandom::getrandom(&mut b).expect("getrandom");
        b
    }

    // v10 body = nonce(12) || ciphertext || tag(16)
    pub fn gcm_decrypt(key: &[u8], body: &[u8]) -> Option<Vec<u8>> {
        if key.len() != 32 || body.len() < 12 + 16 {
            return None;
        }
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
        let (nonce, ct) = body.split_at(12);
        cipher.decrypt(Nonce::from_slice(nonce), ct).ok()
    }

    pub fn gcm_encrypt(key: &[u8], plaintext: &[u8]) -> Vec<u8> {
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
        let nonce = rand_bytes::<12>();
        let ct = cipher
            .encrypt(Nonce::from_slice(&nonce), plaintext)
            .expect("gcm encrypt");
        let mut out = Vec::with_capacity(12 + ct.len());
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ct);
        out
    }

    // ---- DPAPI wrap/unwrap ----
    unsafe fn dpapi(input: &[u8], protect: bool) -> Result<Vec<u8>> {
        let in_blob = CRYPT_INTEGER_BLOB {
            cbData: input.len() as u32,
            pbData: input.as_ptr() as *mut u8,
        };
        let mut out_blob = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        let ok = if protect {
            CryptProtectData(
                &in_blob,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                &mut out_blob,
            )
        } else {
            CryptUnprotectData(
                &in_blob,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                &mut out_blob,
            )
        };
        if ok == 0 {
            return Err(anyhow!(
                "DPAPI {} failed",
                if protect { "protect" } else { "unprotect" }
            ));
        }
        let out =
            std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize).to_vec();
        LocalFree(out_blob.pbData as _);
        Ok(out)
    }

    /// Read os_crypt key, or mint+persist one for never-launched profiles.
    pub fn os_crypt_key(udd: &Path) -> Result<Vec<u8>> {
        let ls_path = udd.join("Local State");
        if let Some(key) = read_key(&ls_path)? {
            return Ok(key);
        }
        let key = rand_bytes::<32>().to_vec();
        write_key(&ls_path, &key)?;
        Ok(key)
    }

    fn read_key(ls_path: &Path) -> Result<Option<Vec<u8>>> {
        if !ls_path.exists() {
            return Ok(None);
        }
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(ls_path).context("read Local State")?)
                .context("parse Local State")?;
        let enc = match json
            .get("os_crypt")
            .and_then(|o| o.get("encrypted_key"))
            .and_then(|k| k.as_str())
        {
            Some(s) => s,
            None => return Ok(None),
        };
        let blob = STANDARD.decode(enc).context("base64 encrypted_key")?;
        if blob.len() <= DPAPI_TAG.len() || &blob[..DPAPI_TAG.len()] != DPAPI_TAG {
            return Err(anyhow!("encrypted_key missing DPAPI tag"));
        }
        Ok(Some(unsafe { dpapi(&blob[DPAPI_TAG.len()..], false)? }))
    }

    fn write_key(ls_path: &Path, key: &[u8]) -> Result<()> {
        let wrapped = unsafe { dpapi(key, true)? };
        let mut tagged = DPAPI_TAG.to_vec();
        tagged.extend_from_slice(&wrapped);
        let b64 = STANDARD.encode(&tagged);

        // Merge into existing Local State or create minimal one.
        let mut json: serde_json::Value = if ls_path.exists() {
            serde_json::from_str(&std::fs::read_to_string(ls_path)?)
                .unwrap_or_else(|_| serde_json::json!({}))
        } else {
            if let Some(p) = ls_path.parent() {
                std::fs::create_dir_all(p).ok();
            }
            serde_json::json!({})
        };
        if !json.is_object() {
            json = serde_json::json!({});
        }
        json["os_crypt"]["encrypted_key"] = serde_json::Value::String(b64);
        std::fs::write(ls_path, serde_json::to_string(&json)?)?;
        Ok(())
    }
}

fn samesite_to_str(v: i64) -> &'static str {
    match v {
        0 => "None",
        1 => "Lax",
        2 => "Strict",
        _ => "unspecified",
    }
}
fn samesite_from_str(s: Option<&str>) -> i64 {
    match s.map(|x| x.to_ascii_lowercase()).as_deref() {
        // "no_restriction" is what EditThisCookie-style exports call SameSite=None.
        Some("none") | Some("no_restriction") => 0,
        Some("lax") => 1,
        Some("strict") => 2,
        _ => -1,
    }
}

/// Path to the profile's Cookies SQLite DB; Default/ or Default/Network/.
/// The cookie database this profile's browser actually reads and writes. Current engines keep it
/// at `Default/Network/Cookies`; `Default/Cookies` is the old place, and a profile can have both —
/// a stale leftover next to the live one (seen on a Mac: 5 rows in the old file, 76 in the live
/// one). The old file used to win here, so a login restored on that machine went into a file the
/// browser never opens and the profile came up logged out. The live place wins; the old one is
/// only used when it is all there is.
fn cookies_db_path(udd: &Path) -> PathBuf {
    let live = udd.join("Default").join("Network").join("Cookies");
    if live.exists() {
        return live;
    }
    let legacy = udd.join("Default").join("Cookies");
    if legacy.exists() {
        return legacy;
    }
    live
}

/// Export decrypted cookies.
pub fn export(profile_id: &str) -> Result<Vec<Cookie>> {
    crate::cloud_sync::ensure_access(profile_id)?;
    let udd = profile::user_data_dir(profile_id)?;
    let path = cookies_db_path(&udd);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let crypt = Crypt::open(&udd)?;
    // Read-only to avoid WAL write-lock fights with a running browser.
    let conn = rusqlite::Connection::open_with_flags(
        &path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .with_context(|| format!("open {}", path.display()))?;

    let mut stmt = conn.prepare(
        "SELECT host_key, name, value, encrypted_value, path, expires_utc, \
         is_secure, is_httponly, has_expires, samesite FROM cookies",
    )?;
    let rows = stmt.query_map([], |r| {
        let host: String = r.get(0)?;
        let name: String = r.get(1)?;
        let plain: String = r.get(2)?;
        let enc: Vec<u8> = r.get(3)?;
        let path: String = r.get(4)?;
        let expires_utc: i64 = r.get(5)?;
        let is_secure: i64 = r.get(6)?;
        let is_httponly: i64 = r.get(7)?;
        let has_expires: i64 = r.get(8)?;
        let samesite: i64 = r.get(9)?;
        Ok(Cookie {
            value: crypt.decrypt(&enc, &plain),
            domain: host,
            name,
            path,
            expires: if has_expires != 0 {
                Some(chromium_to_unix_secs(expires_utc))
            } else {
                None
            },
            session: has_expires == 0,
            secure: is_secure != 0,
            http_only: is_httponly != 0,
            same_site: Some(samesite_to_str(samesite).to_string()),
        })
    })?;
    let mut out = Vec::new();
    for c in rows {
        out.push(c?);
    }
    Ok(out)
}

/// The expiry to store, or None for a session cookie: not when flagged `session`,
/// not for the -1 / 0 some tools use for "none", and millisecond timestamps are
/// brought back to seconds.
fn persistent_expiry(c: &Cookie) -> Option<f64> {
    if c.session {
        return None;
    }
    c.expires.filter(|e| *e > 0.0).map(|e| if e > 1e11 { e / 1000.0 } else { e })
}

/// Reads a cookie file in any of the shapes people actually have: a JSON array or
/// `{"cookies": [...]}` (Cookie-Editor, EditThisCookie, Playwright…), the Netscape
/// `cookies.txt` format, or a bare `name=value; name=value` string when it is
/// recognisably Facebook's (`c_user`, `xs`…), which carries no domain of its own.
pub fn parse_any(text: &str) -> Result<Vec<Cookie>> {
    let t = text.trim().trim_start_matches('\u{feff}');
    if t.is_empty() {
        anyhow::bail!("file is empty");
    }
    if t.starts_with('[') || t.starts_with('{') {
        let v: serde_json::Value = serde_json::from_str(t).context("not valid JSON")?;
        let arr = match v {
            serde_json::Value::Array(a) => a,
            serde_json::Value::Object(mut o) => match o.remove("cookies") {
                Some(serde_json::Value::Array(a)) => a,
                _ => anyhow::bail!("JSON object without a \"cookies\" array"),
            },
            _ => anyhow::bail!("unsupported JSON"),
        };
        let mut out = Vec::new();
        for (i, item) in arr.into_iter().enumerate() {
            out.push(serde_json::from_value::<Cookie>(item).with_context(|| format!("cookie #{}: needs at least domain, name and value", i + 1))?);
        }
        return Ok(out);
    }
    if t.lines().any(|l| l.split('\t').count() >= 7) {
        return Ok(parse_netscape(t));
    }
    parse_cookie_string(t)
}

fn parse_netscape(text: &str) -> Vec<Cookie> {
    let mut out = Vec::new();
    for line in text.lines() {
        let l = line.trim_end_matches(['\r', '\n']);
        let (l, http_only) = match l.strip_prefix("#HttpOnly_") {
            Some(r) => (r, true),
            None => (l, false),
        };
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        let f: Vec<&str> = l.split('\t').collect();
        if f.len() < 7 {
            continue;
        }
        let include_sub = f[1].eq_ignore_ascii_case("TRUE");
        let domain = if include_sub && !f[0].starts_with('.') { format!(".{}", f[0]) } else { f[0].to_string() };
        let expiry = f[4].trim().parse::<f64>().ok().filter(|e| *e > 0.0);
        out.push(Cookie {
            domain,
            name: f[5].to_string(),
            value: f[6..].join("\t"),
            path: f[2].to_string(),
            expires: expiry,
            session: expiry.is_none(),
            secure: f[3].eq_ignore_ascii_case("TRUE"),
            http_only,
            same_site: None,
        });
    }
    out
}

fn parse_cookie_string(t: &str) -> Result<Vec<Cookie>> {
    let t = t.strip_prefix("Cookie:").unwrap_or(t).trim();
    let pairs: Vec<(String, String)> = t
        .split(';')
        .filter_map(|p| p.trim().split_once('=').map(|(k, v)| (k.trim().to_string(), v.trim().to_string())))
        .filter(|(k, _)| !k.is_empty())
        .collect();
    let looks_facebook = pairs.iter().any(|(k, _)| matches!(k.as_str(), "c_user" | "xs" | "datr" | "fr" | "sb"));
    if pairs.is_empty() || !looks_facebook {
        anyhow::bail!("unrecognised cookie file: expected JSON, Netscape cookies.txt, or a Facebook cookie string (c_user=…; xs=…)");
    }
    let expires = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0) + 365.0 * 86400.0;
    Ok(pairs
        .into_iter()
        .map(|(name, value)| Cookie {
            http_only: matches!(name.as_str(), "xs" | "datr" | "fr" | "sb"),
            domain: ".facebook.com".into(),
            name,
            value,
            path: "/".into(),
            expires: Some(expires),
            session: false,
            secure: true,
            same_site: Some("None".into()),
        })
        .collect())
}

/// Import cookies (v10-encrypted). Caller MUST stop the profile first.
pub fn import(profile_id: &str, cookies: &[Cookie]) -> Result<usize> {
    let udd = profile::user_data_dir(profile_id)?;
    let path = cookies_db_path(&udd);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let crypt = Crypt::open(&udd)?;
    let conn = rusqlite::Connection::open(&path)
        .with_context(|| format!("open {}", path.display()))?;
    ensure_schema(&conn)?;

    let now = now_chromium();
    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT OR REPLACE INTO cookies (\
             creation_utc, host_key, top_frame_site_key, name, value, encrypted_value, \
             path, expires_utc, is_secure, is_httponly, last_access_utc, has_expires, \
             is_persistent, priority, samesite, source_scheme, source_port, \
             last_update_utc, source_type, has_cross_site_ancestor) \
             VALUES (?1, ?2, '', ?3, '', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 1, ?12, ?13, ?14, ?15, 0, 1)",
        )?;
        for c in cookies {
            let enc = crypt.encrypt(&c.domain, &c.value);
            let expiry = persistent_expiry(c);
            let has_expires = expiry.is_some();
            let expires_utc = expiry.map(unix_to_chromium).unwrap_or(0);
            let source_scheme = if c.secure { 2 } else { 1 };
            let source_port = if c.secure { 443 } else { 80 };
            stmt.execute(rusqlite::params![
                now,
                c.domain,
                c.name,
                enc,
                c.path,
                expires_utc,
                c.secure as i64,
                c.http_only as i64,
                now,
                has_expires as i64,
                has_expires as i64,
                samesite_from_str(c.same_site.as_deref()),
                source_scheme,
                source_port,
                now,
            ])?;
        }
    }
    tx.commit()?;
    Ok(cookies.len())
}

/// Create cookies table + meta to match Chromium v24 schema for a fresh DB.
fn ensure_schema(conn: &rusqlite::Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS meta (key LONGVARCHAR NOT NULL UNIQUE PRIMARY KEY, value LONGVARCHAR);\
         INSERT OR IGNORE INTO meta (key, value) VALUES ('version', '24');\
         INSERT OR IGNORE INTO meta (key, value) VALUES ('last_compatible_version', '24');\
         CREATE TABLE IF NOT EXISTS cookies (\
            creation_utc INTEGER NOT NULL, host_key TEXT NOT NULL, \
            top_frame_site_key TEXT NOT NULL, name TEXT NOT NULL, value TEXT NOT NULL, \
            encrypted_value BLOB NOT NULL, path TEXT NOT NULL, expires_utc INTEGER NOT NULL, \
            is_secure INTEGER NOT NULL, is_httponly INTEGER NOT NULL, last_access_utc INTEGER NOT NULL, \
            has_expires INTEGER NOT NULL, is_persistent INTEGER NOT NULL, priority INTEGER NOT NULL, \
            samesite INTEGER NOT NULL, source_scheme INTEGER NOT NULL, source_port INTEGER NOT NULL, \
            last_update_utc INTEGER NOT NULL, source_type INTEGER NOT NULL, \
            has_cross_site_ancestor INTEGER NOT NULL);\
         CREATE UNIQUE INDEX IF NOT EXISTS cookies_unique_index ON cookies (\
            host_key, top_frame_site_key, has_cross_site_ancestor, name, path, source_scheme, source_port);",
    )?;
    Ok(())
}

#[cfg(test)]
mod import_format_tests {
    use super::*;

    #[test]
    fn extension_exports_keep_their_expiry_and_samesite() {
        let json = r#"[{"domain":".facebook.com","name":"xs","value":"a%3Ab","path":"/","expirationDate":1893456000.5,"secure":true,"httpOnly":true,"sameSite":"no_restriction","hostOnly":false,"session":false,"storeId":"0"},
                       {"domain":".facebook.com","name":"tmp","value":"1","session":true,"expirationDate":1893456000,"sameSite":"lax"}]"#;
        let c = parse_any(json).unwrap();
        assert_eq!(persistent_expiry(&c[0]), Some(1893456000.5));
        assert_eq!((c[0].http_only, c[0].secure), (true, true));
        assert_eq!(samesite_from_str(c[0].same_site.as_deref()), 0, "no_restriction is SameSite=None");
        assert_eq!(persistent_expiry(&c[1]), None, "an explicit session cookie stays a session cookie");
        assert_eq!(samesite_from_str(c[1].same_site.as_deref()), 1);
    }

    #[test]
    fn playwright_and_millisecond_and_wrapped_shapes() {
        let c = parse_any(r#"{"cookies":[{"name":"a","value":"1","domain":"x.com","path":"/","expires":-1},{"name":"b","value":"2","domain":"x.com","expires":1893456000000}],"origins":[]}"#).unwrap();
        assert_eq!(persistent_expiry(&c[0]), None, "-1 means session");
        assert_eq!(persistent_expiry(&c[1]), Some(1893456000.0), "milliseconds are brought back to seconds");
    }

    #[test]
    fn netscape_files_including_httponly_lines() {
        let txt = "# Netscape HTTP Cookie File\n.facebook.com\tTRUE\t/\tTRUE\t1893456000\tc_user\t100000000000001\n#HttpOnly_.facebook.com\tTRUE\t/\tTRUE\t1893456000\txs\t43%3Aabc\nexample.com\tFALSE\t/\tFALSE\t0\tsess\tv\n";
        let c = parse_any(txt).unwrap();
        assert_eq!(c.len(), 3);
        assert!(!c[0].http_only && c[1].http_only && c[1].secure);
        assert_eq!(c[1].value, "43%3Aabc");
        assert_eq!(persistent_expiry(&c[2]), None);
        assert_eq!(c[2].domain, "example.com");
    }

    #[test]
    fn a_facebook_cookie_string_becomes_facebook_cookies_and_anything_else_is_refused() {
        let c = parse_any("sb=abc; datr=def; c_user=100000000000001; xs=43%3Ax; fr=0f").unwrap();
        assert_eq!(c.len(), 5);
        assert!(c.iter().all(|x| x.domain == ".facebook.com" && x.secure && persistent_expiry(x).is_some()));
        assert!(c.iter().find(|x| x.name == "xs").unwrap().http_only);
        assert!(!c.iter().find(|x| x.name == "c_user").unwrap().http_only);
        assert!(parse_any("foo=bar; baz=1").is_err(), "no domain, not recognisably Facebook");
        assert!(parse_any("").is_err());
        assert!(parse_any("[{\"name\":\"a\"}]").is_err(), "a cookie needs a domain");
    }
}



// ---- Portable copies: cookies and saved passwords moving between machines ----
//
// A cookie or a saved password is sealed with a key that belongs to one machine (on Windows
// a DPAPI key tied to the Windows account, on macOS a fixed one, but a different cipher). The
// raw database from one machine is gibberish on the next, which is how a profile arrived on
// a second computer logged out with its passwords gone. So what travels is a copy of the
// database with every sealed value replaced by its plaintext behind a marker; the receiving
// machine seals each one again with its own key.

/// Starts a value in a portable copy: the bytes after it are the plaintext.
const PORTABLE_MARK: &[u8] = b"hir-plain:";

/// Which sealed database a portable copy is made from.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sealed {
    /// `Network/Cookies`: table `cookies`, column `encrypted_value`, host-prefixed.
    Cookies,
    /// `Login Data` and `Login Data For Account`: table `logins`, column `password_value`.
    Logins,
}

impl Sealed {
    fn table_column(self) -> (&'static str, &'static str) {
        match self {
            Sealed::Cookies => ("cookies", "encrypted_value"),
            Sealed::Logins => ("logins", "password_value"),
        }
    }
    /// The columns that make a row "the same one" in two databases of this kind.
    fn key_columns(self) -> &'static [&'static str] {
        match self {
            Sealed::Cookies => &["host_key", "top_frame_site_key", "has_cross_site_ancestor", "name", "path", "source_scheme", "source_port"],
            Sealed::Logins => &["origin_url", "username_element", "username_value", "password_element", "signon_realm"],
        }
    }
    /// How recent a row is, best column first (the first one both databases have is used).
    fn freshness_columns(self) -> &'static [&'static str] {
        match self {
            Sealed::Cookies => &["last_update_utc", "creation_utc"],
            Sealed::Logins => &["date_password_modified", "date_created"],
        }
    }
    /// Query returning (rowid, sealed value, host) for every row.
    fn select(self) -> &'static str {
        match self {
            Sealed::Cookies => "SELECT rowid, encrypted_value, host_key FROM cookies",
            Sealed::Logins => "SELECT rowid, password_value, '' FROM logins",
        }
    }
}

fn scratch_copy(src: &Path) -> Result<PathBuf> {
    let tmp = std::env::temp_dir().join(format!("hir-portable-{}.db", uuid::Uuid::new_v4()));
    std::fs::copy(src, &tmp).with_context(|| format!("copy {}", src.display()))?;
    Ok(tmp)
}

fn remove_db(path: &Path) {
    let _ = std::fs::remove_file(path);
    for suffix in ["-journal", "-wal", "-shm"] {
        let mut o = path.as_os_str().to_owned();
        o.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(o));
    }
}

/// What `portable_copy` found in the database: rows in all, rows turned into plaintext, rows
/// dropped because this machine could not open them. All of them dropped means the key this
/// machine seals with is not the key the browser sealed with — the login cannot travel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PortableStats {
    pub rows: usize,
    pub portable: usize,
    pub dropped: usize,
}

/// A copy of the database at `src` with every value this machine sealed turned into
/// plaintext. A value this machine cannot open (sealed by another one long ago) is dropped
/// with its row, rather than sent on as noise. The original is only read.
pub(crate) fn portable_copy(udd: &Path, src: &Path, kind: Sealed) -> Result<(Vec<u8>, PortableStats)> {
    let tmp = scratch_copy(src)?;
    let result = (|| -> Result<(Vec<u8>, PortableStats)> {
        let crypt = Crypt::open(udd)?;
        let (table, column) = kind.table_column();
        let conn = rusqlite::Connection::open(&tmp).with_context(|| format!("open {}", tmp.display()))?;
        let rows: Vec<(i64, Vec<u8>)> = {
            let mut stmt = conn.prepare(kind.select())?;
            let it = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)))?;
            it.filter_map(|r| r.ok()).collect()
        };
        let mut stats = PortableStats { rows: rows.len(), ..Default::default() };
        for (rowid, blob) in rows {
            if blob.len() < 3 || &blob[..3] != b"v10" {
                continue; // empty, or stored in the clear: nothing to convert
            }
            match crypt.decrypt_blob(&blob, kind == Sealed::Cookies) {
                Some(plain) => {
                    let mut out = PORTABLE_MARK.to_vec();
                    out.extend_from_slice(&plain);
                    conn.execute(&format!("UPDATE {table} SET {column} = ?1 WHERE rowid = ?2"), rusqlite::params![out, rowid])?;
                    stats.portable += 1;
                }
                None => {
                    conn.execute(&format!("DELETE FROM {table} WHERE rowid = ?1"), rusqlite::params![rowid])?;
                    stats.dropped += 1;
                }
            }
        }
        drop(conn);
        Ok((std::fs::read(&tmp)?, stats))
    })();
    remove_db(&tmp);
    result
}

/// Seals every portable value in the database file at `db` with THIS machine's key, in place.
/// Returns how many were converted.
pub(crate) fn localize_file(udd: &Path, db: &Path, kind: Sealed) -> Result<usize> {
    let crypt = Crypt::open(udd)?;
    let (table, column) = kind.table_column();
    let conn = rusqlite::Connection::open(db).with_context(|| format!("open {}", db.display()))?;
    let rows: Vec<(i64, Vec<u8>, String)> = {
        let mut stmt = conn.prepare(kind.select())?;
        let it = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?, r.get::<_, String>(2)?)))?;
        it.filter_map(|r| r.ok()).collect()
    };
    let mut n = 0;
    for (rowid, blob, host) in rows {
        let Some(plain) = blob.strip_prefix(PORTABLE_MARK) else { continue };
        let sealed = match kind {
            Sealed::Cookies => crypt.encrypt(&host, &String::from_utf8_lossy(plain)),
            Sealed::Logins => crypt.encrypt_plain(plain),
        };
        conn.execute(&format!("UPDATE {table} SET {column} = ?1 WHERE rowid = ?2"), rusqlite::params![sealed, rowid])?;
        n += 1;
    }
    Ok(n)
}

/// Takes the rows of `from` into `into` — never replacing one by an older or equal one, never
/// dropping a row `into` has that `from` lacks. A row is the same row when its key columns match
/// (a cookie's host, name, path…); where both have it, the more recently updated one stays.
///
/// This is how a login arriving from another machine is taken. Replacing the local database by
/// the incoming one meant that any machine that had gone logged-out (a fresh profile, a machine
/// that could not restore its login) and pushed that state took everyone else's login away on
/// their next background pull. Merged, a worse copy can only add what was missing.
///
/// Both files must be sealed with the same key (the incoming one is resealed first). Returns how
/// many rows were added or updated.
pub(crate) fn merge_sealed(into: &Path, from: &Path, kind: Sealed) -> Result<usize> {
    let (table, _) = kind.table_column();
    let conn = rusqlite::Connection::open(into).with_context(|| format!("open {}", into.display()))?;
    conn.execute("ATTACH DATABASE ?1 AS inc", [from.to_string_lossy().as_ref()])?;
    // (column, is part of the primary key) for one of the two databases.
    let columns = |schema: &str| -> Result<Vec<(String, bool)>> {
        let mut st = conn.prepare(&format!("PRAGMA {schema}.table_info(\"{table}\")"))?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, i64>(5)? != 0)))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    };
    let here = columns("main")?;
    let there = columns("inc")?;
    // What both have, without the primary key (those numbers belong to each database alone).
    let common: Vec<String> = here
        .iter()
        .filter(|(n, pk)| !*pk && there.iter().any(|(m, _)| m == n))
        .map(|(n, _)| n.clone())
        .collect();
    let has = |c: &str| common.iter().any(|x| x == c);
    let key: Vec<&str> = kind.key_columns().iter().copied().filter(|k| has(k)).collect();
    // Without a way to tell two rows are the same, changing anything is a guess: change nothing.
    if common.is_empty() || key.len() < 2 {
        return Ok(0);
    }
    let fresh = kind.freshness_columns().iter().copied().find(|c| has(c));
    let list = common.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
    let from_list = common.iter().map(|c| format!("i.\"{c}\"")).collect::<Vec<_>>().join(", ");
    let on = key.iter().map(|k| format!("m.\"{k}\" IS i.\"{k}\"")).collect::<Vec<_>>().join(" AND ");
    let wanted = match fresh {
        Some(f) => format!("m.rowid IS NULL OR i.\"{f}\" > m.\"{f}\""),
        None => "m.rowid IS NULL".to_string(),
    };
    let changed = conn.execute(
        &format!(
            "INSERT OR REPLACE INTO main.\"{table}\" ({list}) \
             SELECT {from_list} FROM inc.\"{table}\" i LEFT JOIN main.\"{table}\" m ON {on} WHERE {wanted}"
        ),
        [],
    )?;
    conn.execute("DETACH DATABASE inc", [])?;
    Ok(changed)
}

/// Of the sealed rows already in the database at `db` (written by this machine's own browser),
/// how many can be opened with the key this launcher believes the browser uses: (looked at,
/// opened). Looks at up to 300. `opened == 0` while `looked at > 0` means the assumption about
/// the key is wrong on this machine, and no login written with it will ever be read.
pub(crate) fn readable_rows(udd: &Path, db: &Path, kind: Sealed) -> Option<(usize, usize)> {
    let (table, column) = kind.table_column();
    let conn = rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let crypt = Crypt::open(udd).ok()?;
    let mut st = conn.prepare(&format!("SELECT {column} FROM {table} WHERE length({column}) > 3 LIMIT 300")).ok()?;
    let blobs: Vec<Vec<u8>> = st.query_map([], |r| r.get::<_, Vec<u8>>(0)).ok()?.filter_map(|r| r.ok()).collect();
    let sealed: Vec<&Vec<u8>> = blobs.iter().filter(|b| b.starts_with(b"v10")).collect();
    let opened = sealed.iter().filter(|b| crypt.decrypt_blob(b, kind == Sealed::Cookies).is_some()).count();
    Some((sealed.len(), opened))
}

/// Deletes the sealed rows of the database at `db` that this machine's key cannot open, and
/// returns how many. Such a row is no use to anyone: the browser here will not read it either, and
/// it only gets in the way of a good one arriving — a merge keeps whichever of two rows is newer,
/// and a newer row that cannot be opened would beat the one that can.
pub(crate) fn purge_unreadable(udd: &Path, db: &Path, kind: Sealed) -> Result<usize> {
    let (table, column) = kind.table_column();
    let crypt = Crypt::open(udd)?;
    let conn = rusqlite::Connection::open(db).with_context(|| format!("open {}", db.display()))?;
    let rows: Vec<(i64, Vec<u8>)> = {
        let mut st = conn.prepare(&format!("SELECT rowid, {column} FROM {table}"))?;
        let it = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)))?;
        it.filter_map(|r| r.ok()).collect()
    };
    let mut gone = 0;
    for (rowid, blob) in rows {
        // Empty or stored in the clear: not sealed, so nothing to be unable to open.
        if !blob.starts_with(b"v10") {
            continue;
        }
        if crypt.decrypt_blob(&blob, kind == Sealed::Cookies).is_none() {
            conn.execute(&format!("DELETE FROM {table} WHERE rowid = ?1"), [rowid])?;
            gone += 1;
        }
    }
    Ok(gone)
}

/// How many rows the sealed database at `db` holds (`None` when it cannot be read).
pub(crate) fn row_count(db: &Path, kind: Sealed) -> Option<i64> {
    let (table, _) = kind.table_column();
    let conn = rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    conn.query_row(&format!("SELECT count(1) FROM {table}"), [], |r| r.get(0)).ok()
}

/// Where a profile's cookie database is, for the sync (which wants the file that exists).
pub(crate) fn cookie_db(udd: &Path) -> PathBuf {
    cookies_db_path(udd)
}

#[cfg(test)]
mod merge_tests {
    use super::*;

    fn logins_db(path: &Path, rows: &[(&str, &str, &str, i64)]) {
        let conn = rusqlite::Connection::open(path).unwrap();
        conn.execute_batch(
            "CREATE TABLE logins (id INTEGER PRIMARY KEY AUTOINCREMENT, origin_url VARCHAR NOT NULL, \
             username_element VARCHAR, username_value VARCHAR, password_element VARCHAR, \
             signon_realm VARCHAR NOT NULL, password_value BLOB, date_created INTEGER NOT NULL, \
             date_password_modified INTEGER NOT NULL); \
             CREATE UNIQUE INDEX u ON logins (origin_url, username_element, username_value, password_element, signon_realm);",
        )
        .unwrap();
        for (site, user, pw, modified) in rows {
            conn.execute(
                "INSERT INTO logins (origin_url, username_element, username_value, password_element, signon_realm, password_value, date_created, date_password_modified) \
                 VALUES (?1, 'u', ?2, 'p', ?1, ?3, 1, ?4)",
                rusqlite::params![site, user, pw.as_bytes(), modified],
            )
            .unwrap();
        }
    }

    fn password(path: &Path, site: &str, user: &str) -> Option<String> {
        let conn = rusqlite::Connection::open(path).unwrap();
        conn.query_row(
            "SELECT password_value FROM logins WHERE origin_url = ?1 AND username_value = ?2",
            [site, user],
            |r| r.get::<_, Vec<u8>>(0),
        )
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
    }

    /// Saved passwords merge the same way as cookies: the more recently changed one stays, a login
    /// only one side has is kept, and an incoming database with different columns changes nothing
    /// it cannot match.
    #[test]
    fn saved_passwords_are_merged_by_site_and_user_and_the_newer_change_wins() {
        let dir = std::env::temp_dir().join(format!("hir-merge-logins-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let (here, there) = (dir.join("here.db"), dir.join("there.db"));
        logins_db(&here, &[("https://a.test", "ann", "mine-newer", 20), ("https://b.test", "bob", "only-here", 5)]);
        logins_db(&there, &[("https://a.test", "ann", "theirs-older", 10), ("https://c.test", "cy", "only-there", 7)]);

        let changed = merge_sealed(&here, &there, Sealed::Logins).unwrap();
        assert_eq!(changed, 1, "only the login this machine lacked was taken");
        assert_eq!(password(&here, "https://a.test", "ann").as_deref(), Some("mine-newer"), "the newer change stays");
        assert_eq!(password(&here, "https://b.test", "bob").as_deref(), Some("only-here"));
        assert_eq!(password(&here, "https://c.test", "cy").as_deref(), Some("only-there"));

        // Now the incoming one is the newer change for ann: it replaces.
        let newer = dir.join("newer.db");
        logins_db(&newer, &[("https://a.test", "ann", "theirs-newer", 99)]);
        assert_eq!(merge_sealed(&here, &newer, Sealed::Logins).unwrap(), 1);
        assert_eq!(password(&here, "https://a.test", "ann").as_deref(), Some("theirs-newer"));

        // A database that is not shaped like this one is left alone rather than guessed at.
        let odd = dir.join("odd.db");
        rusqlite::Connection::open(&odd).unwrap().execute_batch("CREATE TABLE logins (x INTEGER);").unwrap();
        assert_eq!(merge_sealed(&here, &odd, Sealed::Logins).unwrap(), 0);
        assert_eq!(password(&here, "https://a.test", "ann").as_deref(), Some("theirs-newer"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
