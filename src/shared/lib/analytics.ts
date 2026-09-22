import { getVersion } from "@tauri-apps/api/app";

// Launcher-window analytics only: never the panels, never a profile's browser.
//
// The hit is built and sent here rather than by gtag.js. A release window runs
// on `tauri://localhost`, where the remote script does not send — and it needs
// no cookie, no secure context and no third-party script to work.

/** GA4 Web data stream. A measurement id is public by design. */
const MEASUREMENT_ID = "G-PLHRZ45072";
const ENDPOINT = "https://www.google-analytics.com/g/collect";
/** Reported as the page: GA4 would otherwise log `tauri://localhost`. */
const APP_URL = "https://launcher.proxyshard.com/";

const CLIENT_ID_KEY = "shardx-analytics-client-id";
const SESSION_COUNT_KEY = "shardx-analytics-session-count";

/** One id per install; without it every launch would count as a new user. */
function clientId(): string {
  try {
    const stored = localStorage.getItem(CLIENT_ID_KEY);
    if (stored) return stored;
    const fresh = randomId();
    localStorage.setItem(CLIENT_ID_KEY, fresh);
    return fresh;
  } catch {
    return randomId();
  }
}

/** Which session this is for this install, counted from 1.
 *
 *  GA4 decides new against returning from two fields: `_fv`, meaning this is
 *  the user's first visit ever, and `sct`, the ordinal of the session. Both
 *  used to be sent on every launch — `_fv` unconditionally and `sct` as a
 *  literal 1 — so every start filed a `first_visit` and nobody was ever
 *  counted as coming back. gtag.js keeps the same two numbers in the `_ga`
 *  cookie; a custom-scheme window has no cookie, so they live here beside the
 *  client id.
 *
 *  `null` when storage cannot be read: then the client id is random too, so
 *  the hit is a stranger either way, and claiming a first visit on top of that
 *  would inflate new users instead of merely failing to recognise one. */
function nextSessionOrdinal(): number | null {
  try {
    const prev = Number(localStorage.getItem(SESSION_COUNT_KEY) ?? 0);
    // No counter but a client id already stored means an install that predates
    // this counting: it is somebody who has been here, so it starts at 2 and
    // never claims a first visit. Otherwise the whole existing audience would
    // be filed as new once, on the update that fixed exactly that.
    const known = !prev && localStorage.getItem(CLIENT_ID_KEY) !== null;
    const next = Number.isFinite(prev) && prev > 0 ? prev + 1 : known ? 2 : 1;
    localStorage.setItem(SESSION_COUNT_KEY, String(next));
    return next;
  } catch {
    return null;
  }
}

/// `crypto.randomUUID` needs a secure context, which a custom-scheme window is
/// not guaranteed to be.
function randomId(): string {
  try {
    if (typeof crypto?.randomUUID === "function") return crypto.randomUUID();
  } catch { /* not a secure context */ }
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 12)}`;
}

function hostOs(): string {
  const ua = navigator.userAgent;
  if (/Windows/i.test(ua)) return "Windows";
  if (/Macintosh|Mac OS X/i.test(ua)) return "macOS";
  if (/Linux|X11|CrOS/i.test(ua)) return "Linux";
  return "unknown";
}

/** Why the last send did what it did; readable as `__shardxAnalytics`. */
type Status = { sent: boolean; reason: string; client_id?: string; version?: string };

function report(status: Status): void {
  (window as any).__shardxAnalytics = status;
  console.info("[analytics]", status);
}

let version = "unknown";
let session = "";
/** This install's session ordinal, and whether it is its very first. */
let sessionOrdinal: number | null = null;
let ready: Promise<void> | null = null;
/** First hit of the session carries `_ss`; the rest carry time since the last. */
let first = true;
let lastHit = 0;
/** Last section sent, and when — StrictMode mounts an effect twice in dev. */
let lastSection = "";
let lastSectionAt = 0;

export function initAnalytics(): Promise<void> {
  if (!ready) {
    ready = (async () => {
      version = await getVersion().catch(() => "dev");
      session = String(Math.floor(Date.now() / 1000));
      // Counted once per launch, not once per hit.
      sessionOrdinal = nextSessionOrdinal();
    })().catch((e) => {
      // Never an unhandled rejection: analytics must not break the window.
      report({ sent: false, reason: `failed: ${String(e)}` });
    });
  }
  return ready;
}

/** Which part of the launcher is on screen. Sections only — nothing a profile
 *  visits is ever seen here, let alone reported. */
export async function trackSection(section: string): Promise<void> {
  const now = Date.now();
  if (section === lastSection && now - lastSectionAt < 1000) return;
  lastSection = section;
  lastSectionAt = now;
  await initAnalytics();
  await send("page_view", {}, section);
}

/** One event. `sct`/`seg` are what make GA4 count a user rather than just an event. */
export async function send(
  name: string,
  params: Record<string, string | number> = {},
  section = "",
): Promise<void> {
  if (!MEASUREMENT_ID || !session) return;
  const cid = clientId();
  const now = Date.now();
  // Engagement time since the previous hit; without it GA reports sessions as
  // zero-length however long the app was open.
  const engaged = first ? 0 : Math.min(now - lastHit, 30 * 60 * 1000);
  const isFirst = first;
  // Flipped before the request, not after: two hits in flight at once would
  // otherwise both call themselves the first of the session.
  first = false;
  lastHit = now;
  const q = new URLSearchParams({
    v: "2",
    tid: MEASUREMENT_ID,
    cid,
    sid: session,
    // The session's ordinal for this install; 1 only on the first launch.
    sct: String(sessionOrdinal ?? 1),
    seg: "1",
    _s: "1",
    _p: String(Date.now()),
    // No advertising in this app, so nothing here should feed one.
    npa: "1",
    en: name,
    dl: section ? `${APP_URL}${section}` : APP_URL,
    dt: section ? `ShardX Launcher — ${section}` : "ShardX Launcher",
    ul: (navigator.language || "en").toLowerCase(),
    sr: `${screen.width}x${screen.height}`,
    "ep.app_version": version,
    "ep.os": hostOs(),
    "ep.env": import.meta.env.DEV ? "dev" : "prod",
    // `_ss` starts a session and belongs on the first hit of every launch.
    // `_fv` says the user has never been here, so it belongs on exactly one
    // launch in the life of an install.
    ...(isFirst
      ? { _ss: "1", ...(sessionOrdinal === 1 ? { _fv: "1" } : {}) }
      : { _et: String(engaged) }),
  });
  for (const [k, v] of Object.entries(params)) q.set(`ep.${k}`, String(v));

  try {
    await fetch(`${ENDPOINT}?${q}`, { method: "POST", mode: "no-cors", keepalive: true });
    report({ sent: true, reason: `${name}${section ? ` ${section}` : ""}`, client_id: cid, version });
  } catch (e) {
    report({ sent: false, reason: `network: ${String(e)}`, client_id: cid, version });
  }
}
