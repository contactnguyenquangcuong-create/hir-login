//! Tells the company's central server (`web-quan-tri/`, Worker `hirlogin-web`) what this machine did,
//! twice a day, at 9:00 and 17:00 Vietnam time. This is for management only: no profile data travels,
//! and this has nothing to do with team sync.
//!
//! Everything is first written to a log on disk (`fleet-log.json`): which profiles were open and when,
//! automation runs, visits per website. So nothing is lost when the machine loses power or has no
//! network: the log also says which round was last delivered, and a round that was missed is sent as
//! soon as the app is running again. Sending the same thing twice is harmless: sessions are keyed by
//! their start, and website counts are totals for the day, not increments.
//!
//! Sent: this machine's name and version, how many profiles it has, the name / folder / times of the
//! profiles it opened, automation runs and their counts, the team server it is on, and how many times
//! each website's host name was visited. Never: the rest of an address (path, search terms), page
//! contents, the list of its other profiles, cookies, passwords, proxy details.
//!
//! Identified by the license key and machine id, which the server checks with the key server.

use crate::{cloud_sync, license, process, profile, settings};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const REPORT_URL: &str = "https://hirlogin-web.hir-contact-vn.workers.dev/api/bao-cao";
const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 24 * HOUR_MS;
const VIETNAM_MS: i64 = 7 * HOUR_MS;
/// The rounds of the day, Vietnam time.
const ROUND_HOURS: [i64; 2] = [9, 17];
/// Machines spread over this long after the hour, so they do not all arrive in the same second.
const SPREAD_MS: i64 = 10 * 60 * 1000;
/// A round delivered later than this after its time is a catch-up, and the admin sees it as one.
const LATE_MS: i64 = 30 * 60 * 1000;
/// Waits between tries while the server cannot be reached; the last one repeats.
const RETRY_MS: [i64; 5] = [60_000, 2 * 60_000, 5 * 60_000, 15 * 60_000, 30 * 60_000];
const WATCH_EVERY: Duration = Duration::from_secs(5);
const HOUSEKEEPING_MS: i64 = 60_000;
/// How often the log notes that the app is alive: after a power cut, a session ends at that time.
const ALIVE_EVERY_MS: i64 = 5 * 60 * 1000;
/// One request's worth; a long log goes up in several.
const SESSIONS_PER_REQUEST: usize = 400;
const RUNS_PER_REQUEST: usize = 200;
const SITES_PER_REQUEST: usize = 1000;
const REQUESTS_PER_ROUND: usize = 50;
/// Website totals are kept this long for the days the server may still need them for.
const KEEP_SITE_DAYS: i64 = 8;
/// A log that could not be delivered for a very long time does not grow without end.
const KEEP_SESSIONS: usize = 5000;
const SCAN_TRIES: u32 = 3;
const NOTES_KEPT: usize = 100;
/// Microseconds from 1601-01-01 (Chromium's clock) to 1970-01-01.
const CHROMIUM_EPOCH_US: i64 = 11_644_473_600_000_000;

type ReadVisits<'a> = &'a dyn Fn(&str, i64, i64) -> anyhow::Result<Vec<(String, i64)>>;

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn to_chromium_us(ms: i64) -> i64 {
    ms * 1000 + CHROMIUM_EPOCH_US
}

/// The start of the latest round at or before `now`.
fn latest_round(now: i64) -> i64 {
    let local = now + VIETNAM_MS;
    let day = local.div_euclid(DAY_MS) * DAY_MS;
    let mut best = day + (ROUND_HOURS[ROUND_HOURS.len() - 1] - 24) * HOUR_MS;
    for h in ROUND_HOURS {
        if day + h * HOUR_MS <= local {
            best = day + h * HOUR_MS;
        }
    }
    best - VIETNAM_MS
}

/// This machine's place within `SPREAD_MS`: the same every day, different between machines.
fn spread_of(machine_id: &str) -> i64 {
    let sum = machine_id.bytes().fold(0u64, |a, b| a.wrapping_mul(31).wrapping_add(b as u64));
    (sum % SPREAD_MS as u64) as i64
}

fn time_text(ms: i64) -> String {
    chrono::DateTime::from_timestamp((ms + VIETNAM_MS) / 1000, 0).map(|t| t.format("%d/%m %H:%M").to_string()).unwrap_or_default()
}

/// The host name of a visited address, for counting: no path, no search terms, no login. None for
/// what is not a website someone went to: this machine, its network, an address with no host.
fn site_of(address: &str) -> Option<String> {
    let host = url::Url::parse(address).ok()?.host()?.to_owned();
    match host {
        url::Host::Ipv4(ip) if ip.is_private() || ip.is_loopback() || ip.is_link_local() => None,
        url::Host::Ipv6(ip) if ip.is_loopback() => None,
        url::Host::Ipv4(ip) => Some(ip.to_string()),
        url::Host::Ipv6(ip) => Some(ip.to_string()),
        url::Host::Domain(d) => {
            let d = d.to_ascii_lowercase();
            if d == "localhost" || d.ends_with(".localhost") || d.ends_with(".local") || !d.contains('.') {
                return None;
            }
            let bare = d.strip_prefix("www.").or_else(|| d.strip_prefix("m.")).unwrap_or(&d);
            (bare.len() <= 120).then(|| bare.to_string())
        }
    }
}

/// The day of a Chromium visit time, in Vietnam, as the admin pages count days.
fn day_of(visit_us: i64) -> String {
    let secs = (visit_us - CHROMIUM_EPOCH_US) / 1_000_000 + VIETNAM_MS / 1000;
    chrono::DateTime::from_timestamp(secs, 0).map(|t| t.format("%Y-%m-%d").to_string()).unwrap_or_default()
}

/// Visits a person made between two times, as (address, time), from a profile's History file.
/// Read in place, not from a copy: the file can be tens of MB. `immutable` takes no lock, so the
/// browser is not held up; the time index means only the new rows are touched. On the real engine:
/// under 1 ms a read, and no error in 2,670 reads made while it was writing.
/// Not counted: the steps of a redirect (only where it ended, the CHAIN_END flag), frames inside a
/// page (core types 3 and 4), reloads (8), which would make a page that refreshes itself look busy.
fn read_visits(history: &Path, after_us: i64, until_us: i64) -> anyhow::Result<Vec<(String, i64)>> {
    if !history.exists() {
        return Ok(Vec::new());
    }
    let address = url::Url::from_file_path(history).map_err(|_| anyhow::anyhow!("not a file path"))?;
    let db = rusqlite::Connection::open_with_flags(
        format!("{address}?immutable=1"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let mut q = db.prepare(
        "select u.url, v.visit_time from visits v join urls u on u.id = v.url
         where v.visit_time > ?1 and v.visit_time <= ?2 and (v.transition & 536870912) != 0 and (v.transition & 255) not in (3, 4, 8)
         order by v.visit_time limit 50000",
    )?;
    let rows = q.query_map([after_us, until_us], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn history_visits(profile_id: &str, after_us: i64, until_us: i64) -> anyhow::Result<Vec<(String, i64)>> {
    let history = crate::store::user_data_root()?.join(profile_id).join("Default").join("History");
    read_visits(&history, after_us, until_us)
}

/// One time a profile was open on this machine. A team profile's History travels with it, so visits
/// are counted only within this: what happened on another machine must not be counted here.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
struct Session {
    id: String,
    ten: String,
    thu_muc: String,
    bat_dau: i64,
    ket_thuc: Option<i64>,
    /// The app or the machine stopped with it open: `ket_thuc` is when it was last known alive.
    #[serde(default)]
    dot_ngot: bool,
    #[serde(default)]
    read_until_us: i64,
    #[serde(default)]
    scan_failures: u32,
    /// Closed, and its last visits still have to be read.
    #[serde(default)]
    needs_scan: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub(crate) struct RunEntry {
    du_an: String,
    bat_dau: i64,
    ket_thuc: Option<i64>,
    luong: u32,
    dong_ok: u32,
    dong_loi: u32,
}

/// Total visits to one host name on one day. A total, so that sending it twice changes nothing.
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub(crate) struct SiteVisits {
    ngay: String,
    ten_mien: String,
    luot: u64,
}

/// A session as the server sees it: not how far its History has been read.
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub(crate) struct WireSession {
    id: String,
    ten: String,
    thu_muc: String,
    bat_dau: i64,
    ket_thuc: Option<i64>,
    dot_ngot: bool,
}

/// What one request carries.
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
    /// The round this delivers (ms), and whether it comes late.
    dot: i64,
    gui_bu: bool,
    phien: Vec<WireSession>,
    tu_dong: Vec<RunEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    trang_web: Vec<SiteVisits>,
}

/// A part of the log, picked to go up in one request.
#[derive(Debug, Default)]
struct Batch {
    phien: Vec<WireSession>,
    tu_dong: Vec<RunEntry>,
    trang_web: Vec<SiteVisits>,
    days: Vec<String>,
    more: bool,
}

/// The log on disk: what happened, and which round was last delivered.
#[derive(Debug, Serialize, Deserialize, Default)]
struct Checklog {
    /// The latest round the server has.
    #[serde(default)]
    delivered_round: i64,
    #[serde(default)]
    last_delivery: i64,
    #[serde(default)]
    alive_at: i64,
    /// Open now, or closed and not yet delivered.
    #[serde(default)]
    sessions: Vec<Session>,
    #[serde(default)]
    runs: Vec<RunEntry>,
    /// day -> host name -> visits.
    #[serde(default)]
    sites: BTreeMap<String, BTreeMap<String, u64>>,
    /// Days whose totals changed since they were last delivered.
    #[serde(default)]
    sites_changed: BTreeSet<String>,
    /// The last things this did, for looking into a machine that does not report.
    #[serde(default)]
    notes: Vec<String>,
    #[serde(skip)]
    started: bool,
}

impl Checklog {
    fn note(&mut self, now: i64, text: String) {
        self.notes.push(format!("{} {text}", time_text(now)));
        if self.notes.len() > NOTES_KEPT {
            let extra = self.notes.len() - NOTES_KEPT;
            self.notes.drain(0..extra);
        }
    }

    fn add_visits(&mut self, visits: &[(String, i64)]) {
        for (address, at) in visits {
            if let Some(site) = site_of(address) {
                let day = day_of(*at);
                *self.sites.entry(day.clone()).or_default().entry(site).or_default() += 1;
                self.sites_changed.insert(day);
            }
        }
    }

    fn scan_session(&mut self, index: usize, until_ms: i64, read: ReadVisits) {
        let (id, after) = {
            let s = &self.sessions[index];
            (s.id.clone(), s.read_until_us)
        };
        match read(&id, after, to_chromium_us(until_ms)) {
            Ok(visits) => {
                let s = &mut self.sessions[index];
                s.read_until_us = visits.iter().map(|(_, at)| *at).max().unwrap_or(after).max(after);
                s.scan_failures = 0;
                s.needs_scan = false;
                self.add_visits(&visits);
            }
            Err(e) => {
                let s = &mut self.sessions[index];
                s.scan_failures += 1;
                // Closed and still unreadable after a few tries: the session is reported without them.
                if s.needs_scan && s.scan_failures >= SCAN_TRIES {
                    s.needs_scan = false;
                }
                eprintln!("[fleet-report] history not read: {e:#}");
            }
        }
    }

    /// Brings the log up to date with what is open now. Says whether anything changed.
    fn observe(&mut self, now: i64, running: &[(String, u64)], names: &dyn Fn(&str) -> (String, String), read: ReadVisits) -> bool {
        let mut changed = false;
        let open: HashSet<&str> = running.iter().map(|(id, _)| id.as_str()).collect();
        let first_look = !self.started;
        self.started = true;

        // Open in the log but not any more. On the first look after a start, the app or the machine
        // stopped without closing them: they end when it was last known to be alive.
        for i in 0..self.sessions.len() {
            if self.sessions[i].ket_thuc.is_some() || open.contains(self.sessions[i].id.as_str()) {
                continue;
            }
            let end = if first_look && self.alive_at > 0 { self.alive_at.max(self.sessions[i].bat_dau) } else { now };
            self.sessions[i].ket_thuc = Some(end);
            self.sessions[i].needs_scan = true;
            if first_look {
                self.sessions[i].dot_ngot = true;
                let ten = self.sessions[i].ten.clone();
                self.note(now, format!("\"{ten}\" còn mở khi app hoặc máy tắt (có thể mất điện), tính tới {}", time_text(end)));
            }
            // A little past the end after a power cut: the last save of `alive_at` may be behind.
            self.scan_session(i, if first_look { end + ALIVE_EVERY_MS } else { end }, read);
            changed = true;
        }
        // Automation does not outlive the app.
        if first_look {
            let alive = self.alive_at;
            for r in self.runs.iter_mut().filter(|r| r.ket_thuc.is_none()) {
                r.ket_thuc = Some(if alive > 0 { alive.max(r.bat_dau) } else { now });
                changed = true;
            }
        }
        // Open now and new to the log.
        for (id, uptime_ms) in running {
            if self.sessions.iter().any(|s| s.id == *id && s.ket_thuc.is_none()) {
                continue;
            }
            let bat_dau = now - *uptime_ms as i64;
            let (ten, thu_muc) = names(id);
            self.sessions.push(Session { id: id.clone(), ten, thu_muc, bat_dau, read_until_us: to_chromium_us(bat_dau), ..Default::default() });
            changed = true;
        }
        changed
    }

    /// Automation runs: from what is going now, closing those that are not any more. Says whether
    /// one began or ended; counts moving on are saved with the next note that the app is alive.
    fn observe_runs(&mut self, now: i64, runs: &[RunEntry]) -> bool {
        let mut changed = false;
        for live in runs {
            match self.runs.iter_mut().find(|r| r.du_an == live.du_an && r.bat_dau == live.bat_dau) {
                Some(r) => *r = live.clone(),
                None => {
                    self.runs.push(live.clone());
                    changed = true;
                }
            }
        }
        for r in self.runs.iter_mut().filter(|r| r.ket_thuc.is_none()) {
            if !runs.iter().any(|l| l.du_an == r.du_an && l.bat_dau == r.bat_dau) {
                r.ket_thuc = Some(now);
                changed = true;
            }
        }
        changed
    }

    /// Reads what is new for the open sessions, and tries again for closed ones not read yet.
    fn scan_all(&mut self, now: i64, read: ReadVisits) {
        for i in 0..self.sessions.len() {
            let s = &self.sessions[i];
            if s.ket_thuc.is_none() {
                self.scan_session(i, now, read);
            } else if s.needs_scan {
                let until = s.ket_thuc.unwrap_or(now) + if s.dot_ngot { ALIVE_EVERY_MS } else { 0 };
                self.scan_session(i, until, read);
            }
        }
    }

    /// The round to deliver now, if one is due. A round missed for any reason stays due until it is
    /// delivered: the machine was off at 9:00, so it goes when the app is next running.
    fn due_round(&self, now: i64, spread: i64) -> Option<i64> {
        let round = latest_round(now);
        (self.delivered_round < round && now >= round + spread).then_some(round)
    }

    /// The next part of the log to send: every open session (the server rebuilds its list of what is
    /// open from this), and whatever has not been delivered, up to what fits one request.
    fn next_batch(&self) -> Batch {
        let mut b = Batch::default();
        let wire = |s: &Session| WireSession { id: s.id.clone(), ten: s.ten.clone(), thu_muc: s.thu_muc.clone(), bat_dau: s.bat_dau, ket_thuc: s.ket_thuc, dot_ngot: s.dot_ngot };
        // Not those with visits still to read: they stay until they are read, then go.
        let closed: Vec<&Session> = self.sessions.iter().filter(|s| s.ket_thuc.is_some() && !s.needs_scan).collect();
        b.more |= closed.len() > SESSIONS_PER_REQUEST;
        b.phien = self.sessions.iter().filter(|s| s.ket_thuc.is_none()).map(wire).collect();
        b.phien.extend(closed.into_iter().take(SESSIONS_PER_REQUEST).map(wire));
        let done: Vec<&RunEntry> = self.runs.iter().filter(|r| r.ket_thuc.is_some()).collect();
        b.more |= done.len() > RUNS_PER_REQUEST;
        b.tu_dong = self.runs.iter().filter(|r| r.ket_thuc.is_none()).cloned().collect();
        b.tu_dong.extend(done.into_iter().take(RUNS_PER_REQUEST).cloned());
        for day in &self.sites_changed {
            let rows = self.sites.get(day).map(|m| m.len()).unwrap_or(0);
            if !b.days.is_empty() && b.trang_web.len() + rows > SITES_PER_REQUEST {
                b.more = true;
                break;
            }
            b.days.push(day.clone());
            for (host, n) in self.sites.get(day).into_iter().flatten() {
                b.trang_web.push(SiteVisits { ngay: day.clone(), ten_mien: host.clone(), luot: *n });
            }
        }
        b
    }

    /// The server has this part.
    fn delivered(&mut self, b: &Batch) {
        let sent: HashSet<(&str, i64)> = b.phien.iter().filter(|s| s.ket_thuc.is_some()).map(|s| (s.id.as_str(), s.bat_dau)).collect();
        self.sessions.retain(|s| s.ket_thuc.is_none() || !sent.contains(&(s.id.as_str(), s.bat_dau)));
        let done: HashSet<(&str, i64)> = b.tu_dong.iter().filter(|r| r.ket_thuc.is_some()).map(|r| (r.du_an.as_str(), r.bat_dau)).collect();
        self.runs.retain(|r| r.ket_thuc.is_none() || !done.contains(&(r.du_an.as_str(), r.bat_dau)));
        for day in &b.days {
            self.sites_changed.remove(day);
        }
    }

    /// Forgets what cannot matter any more, so the log stays small.
    fn tidy(&mut self, now: i64) {
        let cut = day_of(to_chromium_us(now - KEEP_SITE_DAYS * DAY_MS));
        let changed = self.sites_changed.clone();
        self.sites.retain(|day, _| *day >= cut || changed.contains(day));
        if self.sessions.len() > KEEP_SESSIONS {
            let extra = self.sessions.len() - KEEP_SESSIONS;
            let mut dropped = 0;
            self.sessions.retain(|s| {
                if dropped < extra && s.ket_thuc.is_some() {
                    dropped += 1;
                    false
                } else {
                    true
                }
            });
        }
    }
}

fn log_path() -> anyhow::Result<std::path::PathBuf> {
    Ok(crate::store::user_files_root()?.join("fleet-log.json"))
}

fn load_log(path: &Path) -> Checklog {
    match std::fs::read(path) {
        Ok(body) => serde_json::from_slice(&body).unwrap_or_else(|e| {
            // Damaged: what it held is gone, reporting goes on. The next round is due at once.
            eprintln!("[fleet-report] log unreadable, starting a new one: {e}");
            let _ = std::fs::rename(path, path.with_extension("json.damaged"));
            Checklog::default()
        }),
        Err(_) => Checklog::default(),
    }
}

/// On the disk before the old one is replaced, so that a power cut leaves one or the other, whole.
fn save_log(path: &Path, log: &Checklog) {
    use std::io::Write;
    let tmp = path.with_extension("json.tmp");
    let done = (|| -> anyhow::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&serde_json::to_vec(log)?)?;
        f.sync_all()?;
        drop(f);
        crate::winfs::rename_replace(&tmp, path)?;
        Ok(())
    })();
    if let Err(e) = done {
        eprintln!("[fleet-report] log not saved: {e:#}");
    }
}

/// Host and port only: the address alone tells the admin which team server this is.
fn server_label(url: &str) -> String {
    url.trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/').chars().take(120).collect()
}

fn current_runs() -> Vec<RunEntry> {
    #[cfg(feature = "automation")]
    return crate::runner::all()
        .into_iter()
        .filter(|r| r.running)
        .map(|r| RunEntry { du_an: r.project_name, bat_dau: r.started_at as i64 * 1000, ket_thuc: None, luong: r.workers.len() as u32, dong_ok: r.rows_ok, dong_loi: r.rows_failed })
        .collect();
    #[cfg(not(feature = "automation"))]
    Vec::new()
}

fn running_profiles() -> Vec<(String, u64)> {
    process::Tracker::shared().running().into_iter().map(|r| (r.profile_id, r.uptime_ms)).collect()
}

/// The name and folder of a profile, read when it opens.
fn names_of(id: &str) -> (String, String) {
    let stored = profile::load_raw(id).ok();
    (
        stored.as_ref().and_then(|s| s.config.get("name")).and_then(|v| v.as_str()).unwrap_or_default().to_string(),
        stored.map(|s| s.meta.folder).unwrap_or_default(),
    )
}

/// How many profiles there are, without reading them: a machine can have thousands.
fn count_profiles() -> usize {
    crate::store::profiles_dir()
        .and_then(|d| Ok(std::fs::read_dir(d)?.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "json")).count()))
        .unwrap_or(0)
}

/// This machine's name. `hostname` starts a process, so it is asked once; a name set in the settings
/// is looked at every time.
fn machine_name(sync: &settings::SyncConfig) -> String {
    static NAME: OnceLock<Mutex<Option<(Option<String>, String)>>> = OnceLock::new();
    let mut known = NAME.get_or_init(Mutex::default).lock().unwrap_or_else(|e| e.into_inner());
    if let Some((set, name)) = known.as_ref() {
        if *set == sync.device_name {
            return name.clone();
        }
    }
    let name = cloud_sync::device_name(sync);
    *known = Some((sync.device_name.clone(), name.clone()));
    name
}

#[derive(Debug, Deserialize, Default, PartialEq)]
pub(crate) struct Reply {
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    key_loi: Option<String>,
}

fn http() -> reqwest::Client {
    reqwest::Client::builder().timeout(Duration::from_secs(30)).build().unwrap_or_default()
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

/// The fields of a report that do not come from the log.
fn report_head(key: String, machine_id: String, person: String, round: i64, late: bool) -> Report {
    let sync = settings::load().map(|s| s.sync).unwrap_or_default();
    let may_chu = if sync.enabled { sync.server_url.as_deref().map(server_label).unwrap_or_default() } else { String::new() };
    Report {
        key,
        ma_may: machine_id,
        ten_nguoi: person,
        ten_may: machine_name(&sync),
        phien_ban: env!("CARGO_PKG_VERSION").to_string(),
        he_dieu_hanh: std::env::consts::OS.to_string(),
        may_chu,
        tong_profile: count_profiles(),
        cho_tai_len: cloud_sync::pending_count(),
        dot: round,
        gui_bu: late,
        ..Default::default()
    }
}

/// Delivers everything in the log for a round, in as many requests as it takes. Each part the server
/// answers for leaves the log at once, so a failure half-way only repeats what is left.
async fn deliver_round(client: &reqwest::Client, url: &str, log: &mut Checklog, head: &Report) -> anyhow::Result<Reply> {
    let mut last = Reply::default();
    for _ in 0..REQUESTS_PER_ROUND {
        let batch = log.next_batch();
        let report = Report { phien: batch.phien.clone(), tu_dong: batch.tu_dong.clone(), trang_web: batch.trang_web.clone(), ..head.clone() };
        last = send(client, url, &report).await?;
        if !last.ok {
            return Ok(last); // refused: nothing was kept, the log keeps it all
        }
        log.delivered(&batch);
        if !batch.more {
            break;
        }
    }
    Ok(last)
}

fn report_url() -> String {
    #[cfg(debug_assertions)]
    if let Ok(u) = std::env::var("HIR_LOGIN_REPORT_URL") {
        return u;
    }
    REPORT_URL.to_string()
}

pub async fn run_forever() {
    tokio::time::sleep(Duration::from_secs(8)).await;
    let Ok(path) = log_path() else { return };
    let (client, url) = (http(), report_url());
    let mut log = load_log(&path);
    let mut last_open: Vec<String> = Vec::new();
    let mut next_housekeeping = 0i64;
    let mut failures = 0usize;
    let mut next_try = 0i64;
    let mut last_recheck: Option<Instant> = None;
    loop {
        let now = now_ms();
        let mut open: Vec<String> = process::Tracker::shared().running().into_iter().map(|r| r.profile_id).collect();
        open.sort();
        let housekeeping = now >= next_housekeeping;
        if open != last_open || !log.started || housekeeping {
            last_open = open;
            let mut changed = log.observe(now, &running_profiles(), &names_of, &history_visits);
            if housekeeping {
                next_housekeeping = now + HOUSEKEEPING_MS;
                changed |= log.observe_runs(now, &current_runs());
                if now - log.alive_at >= ALIVE_EVERY_MS {
                    log.alive_at = now;
                    log.scan_all(now, &history_visits);
                    log.tidy(now);
                    changed = true;
                }
            }
            if changed {
                save_log(&path, &log);
            }
        }

        if housekeeping && now >= next_try {
            if let Some((key, machine_id, person)) = license::report_identity() {
                if let Some(round) = log.due_round(now, spread_of(&machine_id)) {
                    log.scan_all(now, &history_visits);
                    let late = now > round + SPREAD_MS + LATE_MS;
                    let head = tokio::task::spawn_blocking(move || report_head(key, machine_id, person, round, late)).await.unwrap_or_default();
                    match deliver_round(&client, &url, &mut log, &head).await {
                        Ok(reply) if reply.ok => {
                            log.delivered_round = round;
                            log.last_delivery = now;
                            log.note(now, format!("Đã gửi đợt {}{}", time_text(round), if late { " (gửi bù)" } else { "" }));
                            failures = 0;
                            next_try = 0;
                            if reply.key_loi.is_some() && last_recheck.is_none_or(|t| t.elapsed() >= Duration::from_secs(1800)) {
                                last_recheck = Some(Instant::now());
                                license::recheck_now().await;
                            }
                        }
                        Ok(reply) => {
                            log.note(now, format!("Máy chủ từ chối đợt {}: {}", time_text(round), reply.key_loi.as_deref().unwrap_or("")));
                            next_try = now + RETRY_MS[failures.min(RETRY_MS.len() - 1)];
                            failures += 1;
                            if last_recheck.is_none_or(|t| t.elapsed() >= Duration::from_secs(1800)) {
                                last_recheck = Some(Instant::now());
                                license::recheck_now().await;
                            }
                        }
                        Err(e) => {
                            eprintln!("[fleet-report] not sent: {e:#}");
                            if failures == 0 {
                                log.note(now, format!("Chưa gửi được đợt {}, sẽ tự thử lại: {e:#}", time_text(round)));
                            }
                            next_try = now + RETRY_MS[failures.min(RETRY_MS.len() - 1)];
                            failures += 1;
                        }
                    }
                    save_log(&path, &log);
                }
            }
        }
        tokio::time::sleep(WATCH_EVERY).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{extract::State, http::StatusCode, routing::post as axum_post, Json, Router};
    use std::sync::Arc;

    /// A Vietnam time as ms.
    fn vn(day: u32, hour: u32, minute: u32) -> i64 {
        chrono::DateTime::parse_from_rfc3339(&format!("2026-10-{day:02}T{hour:02}:{minute:02}:00+07:00")).unwrap().timestamp_millis()
    }
    fn no_names(_: &str) -> (String, String) {
        ("Tên".to_string(), "Folder".to_string())
    }
    fn no_visits(_: &str, _: i64, _: i64) -> anyhow::Result<Vec<(String, i64)>> {
        Ok(Vec::new())
    }

    #[test]
    fn the_rounds_are_nine_and_five_in_vietnam() {
        assert_eq!(latest_round(vn(30, 9, 0)), vn(30, 9, 0));
        assert_eq!(latest_round(vn(30, 8, 59)), vn(29, 17, 0), "before nine, the last round was yesterday's");
        assert_eq!(latest_round(vn(30, 12, 0)), vn(30, 9, 0));
        assert_eq!(latest_round(vn(30, 17, 0)), vn(30, 17, 0));
        assert_eq!(latest_round(vn(30, 23, 59)), vn(30, 17, 0));
        assert_eq!(latest_round(vn(31, 0, 0)), vn(30, 17, 0), "just past midnight");
    }

    #[test]
    fn a_round_is_due_after_its_time_and_stays_due_until_delivered() {
        let mut log = Checklog::default();
        assert_eq!(log.due_round(vn(30, 8, 0), 0), Some(vn(29, 17, 0)), "a new machine reports as soon as it can");
        log.delivered_round = vn(29, 17, 0);
        assert_eq!(log.due_round(vn(30, 8, 59), 0), None, "nothing between rounds");
        assert_eq!(log.due_round(vn(30, 9, 0), 0), Some(vn(30, 9, 0)));
        let spread = 7 * 60 * 1000;
        assert_eq!(log.due_round(vn(30, 9, 6), spread), None, "waits its place in the spread");
        assert_eq!(log.due_round(vn(30, 9, 7), spread), Some(vn(30, 9, 0)));
        // Off at nine, back on at two: delivered then. Not again until five.
        assert_eq!(log.due_round(vn(30, 14, 0), spread), Some(vn(30, 9, 0)));
        log.delivered_round = vn(30, 9, 0);
        assert_eq!(log.due_round(vn(30, 16, 59), spread), None);
        assert_eq!(log.due_round(vn(30, 17, 7), spread), Some(vn(30, 17, 0)));
        // Off for days: one delivery, with everything in it, not one for each round missed.
        assert_eq!(log.due_round(vn(30, 18, 0) + 3 * DAY_MS, spread), Some(vn(30, 17, 0) + 3 * DAY_MS));
        log.delivered_round = vn(30, 17, 0) + 3 * DAY_MS;
        assert_eq!(log.due_round(vn(30, 18, 0) + 3 * DAY_MS, spread), None);
    }

    #[test]
    fn machines_spread_out_and_each_keeps_its_place() {
        let (a, b) = (spread_of("machine-a"), spread_of("machine-b"));
        assert_ne!(a, b);
        assert_eq!(a, spread_of("machine-a"));
        for id in ["", "x", "machine-a", &"z".repeat(64)] {
            assert!((0..SPREAD_MS).contains(&spread_of(id)), "{id}");
        }
    }

    #[test]
    fn a_session_runs_from_open_to_close_and_visits_count_only_within_it() {
        let mut log = Checklog::default();
        let t = vn(30, 10, 0);
        let us = to_chromium_us(t);
        let read = move |_: &str, after: i64, until: i64| -> anyhow::Result<Vec<(String, i64)>> {
            let all = vec![("https://other-machine.example/".to_string(), us - 7_200_000_000), ("https://here.example/1".to_string(), us + 1_000_000), ("https://here.example/2".to_string(), us + 3_000_000), ("https://after.example/".to_string(), us + 60_000_000)];
            Ok(all.into_iter().filter(|(_, at)| *at > after && *at <= until).collect())
        };
        // Opened one minute ago.
        assert!(log.observe(t, &[("p1".into(), 60_000)], &no_names, &read));
        assert_eq!(log.sessions[0].bat_dau, t - 60_000);
        assert_eq!(log.sessions[0].ten, "Tên");
        assert!(!log.observe(t + 5000, &[("p1".into(), 65_000)], &no_names, &read), "nothing new");
        // Closed.
        assert!(log.observe(t + 10_000, &[], &no_names, &read));
        assert_eq!(log.sessions[0].ket_thuc, Some(t + 10_000));
        assert!(!log.sessions[0].dot_ngot);
        assert_eq!(log.sites["2026-10-30"]["here.example"], 2, "read at the close");
        assert!(!log.sites["2026-10-30"].contains_key("other-machine.example"), "before it opened here");
        assert!(!log.sites["2026-10-30"].contains_key("after.example"), "after it closed here");
        // Reading again changes nothing.
        log.scan_all(t + 20_000, &read);
        assert_eq!(log.sites["2026-10-30"]["here.example"], 2);
    }

    #[test]
    fn visits_of_an_open_profile_are_counted_once_however_often_it_is_read() {
        let mut log = Checklog::default();
        let t = vn(30, 10, 0);
        let us = to_chromium_us(t);
        let read = move |_: &str, after: i64, until: i64| -> anyhow::Result<Vec<(String, i64)>> {
            Ok((1..=5).map(|i| (format!("https://a.example/{i}"), us + i * 60_000_000)).filter(|(_, at)| *at > after && *at <= until).collect())
        };
        log.observe(t, &[("p1".into(), 0)], &no_names, &read);
        for minute in [2, 2, 3, 5, 5, 9] {
            log.scan_all(t + minute * 60_000, &read);
        }
        assert_eq!(log.sites["2026-10-30"]["a.example"], 5);
    }

    /// Power cut at 10:17. The log says the app was alive at 10:15 and the profile was open. Next
    /// start: the session ends at 10:15, its visits are counted, the round missed is due.
    #[test]
    fn after_a_power_cut_nothing_is_lost_and_the_missed_round_goes_up() {
        let dir = std::env::temp_dir().join(format!("hir-fleet-log-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("fleet-log.json");
        let us = to_chromium_us(vn(30, 10, 0));
        let read = move |_: &str, after: i64, until: i64| -> anyhow::Result<Vec<(String, i64)>> {
            Ok(vec![("https://a.example/".to_string(), us + 1_000_000), ("https://b.example/".to_string(), us + 600_000_000)].into_iter().filter(|(_, at)| *at > after && *at <= until).collect())
        };
        {
            let mut log = Checklog::default();
            log.delivered_round = vn(30, 9, 0);
            log.observe(vn(30, 10, 0), &[("p1".into(), 0)], &no_names, &read);
            log.observe_runs(vn(30, 10, 0), &[RunEntry { du_an: "Đăng bài".into(), bat_dau: vn(30, 10, 0), ..Default::default() }]);
            log.alive_at = vn(30, 10, 15);
            save_log(&path, &log);
            // The power goes: no close, no goodbye.
        }
        let mut log = load_log(&path);
        assert_eq!(log.sessions.len(), 1, "the open session survived on disk");
        assert_eq!(log.sessions[0].ket_thuc, None);
        // Back on the next morning, with the profile not running.
        let back = vn(31, 8, 30);
        log.observe(back, &[], &no_names, &read);
        assert_eq!(log.sessions[0].ket_thuc, Some(vn(30, 10, 15)), "ends when it was last known alive, not this morning");
        assert!(log.sessions[0].dot_ngot);
        assert_eq!(log.runs[0].ket_thuc, Some(vn(30, 10, 15)), "so does the automation run");
        assert_eq!(log.sites["2026-10-30"]["a.example"], 1);
        assert_eq!(log.sites["2026-10-30"]["b.example"], 1);
        assert!(log.notes.iter().any(|n| n.contains("mất điện")));
        assert_eq!(log.due_round(back, 0), Some(vn(30, 17, 0)), "the round missed at five goes up now");
        let b = log.next_batch();
        assert_eq!(b.phien.len(), 1);
        assert_eq!(b.phien[0].ket_thuc, Some(vn(30, 10, 15)));
        assert!(b.phien[0].dot_ngot);
        assert_eq!(b.trang_web.len(), 2);
        assert_eq!(b.tu_dong.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_browser_still_open_after_the_app_restarts_is_the_same_session() {
        let mut log = Checklog::default();
        log.observe(vn(30, 10, 0), &[("p1".into(), 0)], &no_names, &no_visits);
        log.started = false; // the app restarted (an update), the browser did not close
        log.alive_at = vn(30, 10, 5);
        log.observe(vn(30, 10, 6), &[("p1".into(), 2_000)], &no_names, &no_visits);
        assert_eq!(log.sessions.len(), 1);
        assert_eq!(log.sessions[0].bat_dau, vn(30, 10, 0));
        assert_eq!(log.sessions[0].ket_thuc, None);
    }

    #[test]
    fn a_session_whose_history_cannot_be_read_waits_a_little_then_goes_without_it() {
        let mut log = Checklog::default();
        let fails = |_: &str, _: i64, _: i64| -> anyhow::Result<Vec<(String, i64)>> { anyhow::bail!("locked") };
        log.observe(vn(30, 10, 0), &[("p1".into(), 0)], &no_names, &no_visits);
        log.observe(vn(30, 11, 0), &[], &no_names, &fails);
        assert!(log.sessions[0].needs_scan);
        assert!(log.next_batch().phien.is_empty(), "held back while it may still be read");
        log.scan_all(vn(30, 11, 1), &fails);
        assert!(log.sessions[0].needs_scan);
        log.scan_all(vn(30, 11, 2), &fails);
        assert!(!log.sessions[0].needs_scan, "given up");
        assert_eq!(log.next_batch().phien.len(), 1, "the session itself is still reported");
    }

    #[test]
    fn a_damaged_log_starts_over_instead_of_stopping_the_reports() {
        let dir = std::env::temp_dir().join(format!("hir-fleet-log-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("fleet-log.json");
        std::fs::write(&path, vec![0u8; 500]).unwrap();
        let log = load_log(&path);
        assert_eq!(log.delivered_round, 0, "due at once");
        assert!(dir.join("fleet-log.json.damaged").exists(), "set aside");
        assert_eq!(load_log(&dir.join("no-such-file")).delivered_round, 0);
        let mut log = Checklog::default();
        log.delivered_round = 5;
        save_log(&path, &log);
        assert_eq!(load_log(&path).delivered_round, 5);
        assert!(!dir.join("fleet-log.json.tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn what_was_delivered_leaves_the_log_and_the_rest_stays() {
        let mut log = Checklog::default();
        log.observe(vn(30, 10, 0), &[("open".into(), 0), ("closes".into(), 0)], &no_names, &no_visits);
        log.observe(vn(30, 10, 5), &[("open".into(), 300_000)], &no_names, &no_visits);
        let at = to_chromium_us(vn(30, 10, 1));
        log.add_visits(&[("https://a.example/1".into(), at), ("https://a.example/2".into(), at), ("http://localhost/".into(), at)]);
        let b = log.next_batch();
        assert_eq!(b.phien.len(), 2, "open and closed");
        assert_eq!(b.trang_web, vec![SiteVisits { ngay: "2026-10-30".into(), ten_mien: "a.example".into(), luot: 2 }]);
        // Not delivered: all still there, the same totals again.
        assert_eq!(log.next_batch().trang_web, b.trang_web);
        log.delivered(&b);
        assert_eq!(log.sessions.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), vec!["open"], "the open one stays, the closed one is done");
        let c = log.next_batch();
        assert_eq!(c.phien.len(), 1, "what is open is always told");
        assert!(c.trang_web.is_empty(), "nothing new");
        // More visits on the same day: the total goes up, a total again, not an increment.
        log.add_visits(&[("https://a.example/3".into(), at)]);
        assert_eq!(log.next_batch().trang_web[0].luot, 3);
    }

    #[test]
    fn a_long_log_goes_up_in_parts_and_nothing_is_left_behind() {
        let mut log = Checklog::default();
        log.started = true;
        for i in 0..1000 {
            log.sessions.push(Session { id: format!("p{i}"), bat_dau: i, ket_thuc: Some(i + 1), ..Default::default() });
        }
        let day = |d: i64| day_of(to_chromium_us(vn(20, 12, 0) + d * DAY_MS));
        for d in 0..5 {
            for s in 0..600 {
                log.sites.entry(day(d)).or_default().insert(format!("s{s}.example"), 1);
            }
            log.sites_changed.insert(day(d));
        }
        let (mut sessions, mut rows, mut parts) = (0, 0, 0);
        loop {
            let b = log.next_batch();
            assert!(b.phien.len() <= SESSIONS_PER_REQUEST);
            assert!(b.trang_web.len() <= SITES_PER_REQUEST);
            assert!(serde_json::to_vec(&b.phien).unwrap().len() + serde_json::to_vec(&b.trang_web).unwrap().len() < 900_000, "fits what the server accepts");
            sessions += b.phien.len();
            rows += b.trang_web.len();
            parts += 1;
            log.delivered(&b);
            if !b.more {
                break;
            }
        }
        assert_eq!((sessions, rows), (1000, 3000));
        assert!(parts < REQUESTS_PER_ROUND, "{parts}");
        assert!(log.sessions.is_empty() && log.sites_changed.is_empty());
    }

    #[test]
    fn old_totals_are_forgotten_only_once_delivered() {
        let mut log = Checklog::default();
        let old = to_chromium_us(vn(10, 12, 0));
        log.add_visits(&[("https://a.example/".into(), old)]);
        log.tidy(vn(30, 12, 0));
        assert!(log.sites.contains_key("2026-10-10"), "not delivered yet: kept");
        let b = log.next_batch();
        log.delivered(&b);
        log.tidy(vn(30, 12, 0));
        assert!(log.sites.is_empty(), "delivered and old: gone");
        let recent = to_chromium_us(vn(29, 12, 0));
        log.add_visits(&[("https://a.example/".into(), recent)]);
        let b = log.next_batch();
        log.delivered(&b);
        log.tidy(vn(30, 12, 0));
        assert!(log.sites.contains_key("2026-10-29"), "may still get more: kept, so the total stays a total");
    }

    #[test]
    fn automation_runs_are_kept_until_delivered_and_close_by_themselves() {
        let mut log = Checklog::default();
        let run = RunEntry { du_an: "Đăng bài".into(), bat_dau: 1000, ket_thuc: None, luong: 3, dong_ok: 1, dong_loi: 0 };
        assert!(log.observe_runs(2000, std::slice::from_ref(&run)));
        assert!(!log.observe_runs(3000, std::slice::from_ref(&run)), "nothing began or ended");
        assert!(!log.observe_runs(4000, &[RunEntry { dong_ok: 9, ..run.clone() }]), "counts moving on do not need saving at once");
        assert_eq!(log.runs[0].dong_ok, 9);
        let b = log.next_batch();
        log.delivered(&b);
        assert_eq!(log.runs.len(), 1, "still going");
        assert!(log.observe_runs(6000, &[]));
        assert_eq!(log.runs[0].ket_thuc, Some(6000));
        let b = log.next_batch();
        assert_eq!(b.tu_dong.len(), 1);
        log.delivered(&b);
        assert!(log.runs.is_empty());
    }

    type Script = Arc<Mutex<Vec<(u16, serde_json::Value)>>>;
    type Seen = Arc<Mutex<Vec<serde_json::Value>>>;

    /// A server answering from a script; past its end it answers ok.
    async fn server(script: Vec<(u16, serde_json::Value)>) -> (String, Seen) {
        let seen: Seen = Arc::default();
        let script: Script = Arc::new(Mutex::new(script));
        let app = Router::new().route(
            "/r",
            axum_post(|State((seen, script)): State<(Seen, Script)>, Json(body): Json<serde_json::Value>| async move {
                seen.lock().unwrap().push(body);
                let (code, reply) = {
                    let mut s = script.lock().unwrap();
                    if s.is_empty() { (200, serde_json::json!({"ok": true})) } else { s.remove(0) }
                };
                (StatusCode::from_u16(code).unwrap(), Json(reply))
            }),
        ).with_state((seen.clone(), script));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, app).await.ok() });
        (format!("http://127.0.0.1:{port}/r"), seen)
    }

    fn log_with_sessions(n: i64) -> Checklog {
        let mut log = Checklog::default();
        log.started = true;
        for i in 0..n {
            log.sessions.push(Session { id: format!("p{i}"), bat_dau: i, ket_thuc: Some(i + 1), ..Default::default() });
        }
        log
    }

    #[tokio::test]
    async fn a_round_is_delivered_in_parts_and_a_failure_half_way_repeats_only_the_rest() {
        let head = Report { dot: 7, gui_bu: true, ..Default::default() };
        // The second part fails: the first has left the log, the rest has not.
        let (url, seen) = server(vec![(200, serde_json::json!({"ok": true})), (503, serde_json::json!({}))]).await;
        let mut log = log_with_sessions(1000);
        assert!(deliver_round(&http(), &url, &mut log, &head).await.is_err());
        assert_eq!(log.sessions.len(), 600);
        assert_eq!(seen.lock().unwrap().len(), 2);
        assert_eq!(seen.lock().unwrap()[0]["dot"], 7);
        assert_eq!(seen.lock().unwrap()[0]["gui_bu"], true);
        // Tried again: the rest goes up, and the log is empty.
        let (url, seen) = server(vec![]).await;
        let r = deliver_round(&http(), &url, &mut log, &head).await.unwrap();
        assert!(r.ok);
        assert!(log.sessions.is_empty());
        assert_eq!(seen.lock().unwrap().len(), 2, "400 and 200");
    }

    #[tokio::test]
    async fn a_refused_machine_keeps_its_log_and_learns_about_its_key() {
        let (url, _) = server(vec![(403, serde_json::json!({"ok": false, "key_loi": "CHUA_KICH_HOAT"}))]).await;
        let mut log = log_with_sessions(3);
        let r = deliver_round(&http(), &url, &mut log, &Report::default()).await.unwrap();
        assert!(!r.ok);
        assert_eq!(r.key_loi.as_deref(), Some("CHUA_KICH_HOAT"));
        assert_eq!(log.sessions.len(), 3, "nothing was kept by the server: nothing leaves the log");
        // Nobody listening at all: an error, and the log is untouched.
        assert!(deliver_round(&http(), "http://127.0.0.1:9/r", &mut log, &Report::default()).await.is_err());
        assert_eq!(log.sessions.len(), 3);
    }

    /// Management only: the report holds no profile data and no list of profiles.
    #[test]
    fn the_report_carries_no_profile_data_and_stays_small() {
        let idle = Report { key: "HLG-ABCD-EFGH-JKLM".into(), ma_may: "a".repeat(64), ten_may: "DESKTOP-ABC123".into(), phien_ban: "0.2.44".into(), he_dieu_hanh: "windows".into(), may_chu: "100.100.100.100:8787".into(), tong_profile: 5000, ..Default::default() };
        let size = serde_json::to_vec(&idle).unwrap().len();
        assert!(size < 400, "an idle report is {size} bytes");
        assert!(!serde_json::to_string(&idle).unwrap().contains("trang_web"), "no empty list");
        let mut log = Checklog::default();
        log.observe(vn(30, 10, 0), &[("p1".into(), 0)], &no_names, &no_visits);
        log.add_visits(&[("https://shopee.vn/search?keyword=secret&token=abc".into(), to_chromium_us(vn(30, 10, 1)))]);
        let b = log.next_batch();
        let sent = Report { phien: b.phien, trang_web: b.trang_web, ..idle };
        let text = serde_json::to_string(&sent).unwrap().to_lowercase();
        for word in ["proxy", "cookie", "password", "url", "token", "bundle", "keyword", "search", "secret", "read_until"] {
            assert!(!text.contains(word), "{word} in {text}");
        }
        assert!(text.contains("shopee.vn"));
    }

    /// The tables of Chromium's History, with the transition values read from the real engine.
    fn history_with(rows: &[(&str, i64, i64)]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("hir-history-test-{}", uuid::Uuid::new_v4().simple()));
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch("create table urls(id integer primary key, url text); create table visits(id integer primary key, url integer, visit_time integer, transition integer);").unwrap();
        for (i, (u, at, tr)) in rows.iter().enumerate() {
            db.execute("insert into urls(id, url) values (?1, ?2)", rusqlite::params![i as i64 + 1, u]).unwrap();
            db.execute("insert into visits(url, visit_time, transition) values (?1, ?2, ?3)", rusqlite::params![i as i64 + 1, at, tr]).unwrap();
        }
        path
    }
    // As signed 32-bit, the way Chromium stores them.
    const TYPED: i64 = 0x38000001;
    const REDIRECT_START: i64 = 0x18000001;
    const REDIRECT_END: i64 = 0xA8000001u32 as i32 as i64;
    const RELOAD: i64 = 0x30000008;
    const FRAME: i64 = 0x30000003;

    #[test]
    fn only_real_visits_within_the_session_are_counted() {
        let h = history_with(&[
            ("https://old.example/from-another-machine", 100, TYPED),
            ("https://shopee.vn/search?keyword=secret", 200, TYPED),
            ("https://short.link/x", 201, REDIRECT_START),
            ("https://final.example/landing", 202, REDIRECT_END),
            ("https://shopee.vn/search", 203, RELOAD),
            ("https://ads.example/frame", 204, FRAME),
            ("https://later.example/after-it-closed", 900, TYPED),
        ]);
        let got = read_visits(&h, 100, 500).unwrap();
        std::fs::remove_file(&h).unwrap();
        assert_eq!(got, vec![("https://shopee.vn/search?keyword=secret".to_string(), 200), ("https://final.example/landing".to_string(), 202)]);
        assert!(read_visits(Path::new("no-such-history"), 0, 1).unwrap().is_empty(), "a profile that went nowhere has no file");
    }

    #[test]
    fn a_history_in_a_folder_with_spaces_and_vietnamese_is_read_in_place() {
        let dir = std::env::temp_dir().join(format!("hir thư mục {}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = history_with(&[("https://a.example/", 200, TYPED)]);
        let h = dir.join("History");
        std::fs::copy(&src, &h).unwrap();
        std::fs::remove_file(&src).unwrap();
        assert_eq!(read_visits(&h, 100, i64::MAX).unwrap().len(), 1);
        // Held by another connection the browser's way, in the middle of writing.
        let writer = rusqlite::Connection::open(&h).unwrap();
        writer.execute_batch("pragma locking_mode = exclusive; begin exclusive; insert into visits(url, visit_time, transition) values (1, 300, 939524097);").unwrap();
        assert_eq!(read_visits(&h, 100, i64::MAX).unwrap().len(), 1, "a lock held by the browser does not get in the way");
        writer.execute_batch("commit;").unwrap();
        drop(writer);
        assert_eq!(read_visits(&h, 100, i64::MAX).unwrap().len(), 2);
        // A damaged file is an error to try again, not a crash.
        std::fs::write(&h, b"not a database").unwrap();
        assert!(read_visits(&h, 100, i64::MAX).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_visit_becomes_a_host_name_and_nothing_else() {
        assert_eq!(site_of("https://www.shopee.vn/search?keyword=secret&token=abc#x").as_deref(), Some("shopee.vn"));
        assert_eq!(site_of("https://user:pass@M.Facebook.com:8443/me").as_deref(), Some("facebook.com"));
        assert_eq!(site_of("https://banhang.shopee.vn/portal").as_deref(), Some("banhang.shopee.vn"));
        assert_eq!(site_of("https://8.8.8.8/x").as_deref(), Some("8.8.8.8"));
        for no in ["http://localhost:3000/", "http://127.0.0.1/", "http://192.168.1.1/admin", "http://10.0.0.5/", "http://router/", "http://nas.local/", "chrome://settings/", "about:blank", "file:///C:/a.txt", "not an address"] {
            assert_eq!(site_of(no), None, "{no}");
        }
    }

    #[test]
    fn days_are_counted_in_vietnam_time() {
        // 2026-10-29 17:30 UTC is already the 30th in Vietnam.
        let us = chrono::DateTime::parse_from_rfc3339("2026-10-29T17:30:00Z").unwrap().timestamp_micros() + CHROMIUM_EPOCH_US;
        assert_eq!(day_of(us), "2026-10-30");
        assert_eq!(day_of(us - 3_600_000_000), "2026-10-29");
    }

    #[test]
    fn the_server_address_is_short() {
        assert_eq!(server_label("http://100.1.2.3:8787/"), "100.1.2.3:8787");
    }
}
