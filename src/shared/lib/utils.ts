import { invoke } from "@tauri-apps/api/core";

// Host OS of the launcher window (never spoofed) — drives default OS tab + titlebar.
export function detectHostOs(): "macOS" | "Windows" | "Linux" {
  const ua = navigator.userAgent;
  if (/Windows/i.test(ua)) return "Windows";
  if (/Macintosh|Mac OS X/i.test(ua)) return "macOS";
  if (/Linux|X11|CrOS/i.test(ua)) return "Linux";
  return "macOS";
}

export const HOST_OS = detectHostOs();

/// "@1700000000" → "May 26, 14:30" (UTC for cross-timezone consistency).
export function fmtTs(stamp: string): string {
  if (!stamp.startsWith("@")) return stamp;
  const n = parseInt(stamp.slice(1), 10);
  if (!Number.isFinite(n)) return stamp;
  const d = new Date(n * 1000);
  return d.toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
}

/// Format ms uptime as "1h 23m" / "12m 30s" / "45s".
export function fmtUptime(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${sec.toString().padStart(2, "0")}s`;
  return `${sec}s`;
}

/// Byte count as "4.2 MB". Base 1024, which is what a file manager shows.
export function fmtBytes(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(units.length - 1, Math.floor(Math.log(n) / Math.log(1024)));
  const v = n / 1024 ** i;
  return `${v >= 100 || i === 0 ? Math.round(v) : v.toFixed(1)} ${units[i]}`;
}

/// Whole days from now until a unix-second instant; 0 once it has passed.
export function daysUntil(unixSecs: number): number {
  return Math.max(0, Math.ceil((unixSecs * 1000 - Date.now()) / 86_400_000));
}

// Single UTM tag appended to every outbound proxyshard.com link.
export const UTM_QS = "utm_source=shardx&utm_medium=referral&utm_campaign=shardx-launcher";
export const withUtm = (url: string) => url + (url.includes("?") ? "&" : "?") + UTM_QS;

// ProxyShard dashboard (account, billing, API key).
export const DASHBOARD_URL = withUtm("https://dashboard.proxyshard.com/");

// Docs URL behind the proxy UDP/No-UDP pill.
export const UDP_DOCS_URL = withUtm("https://docs.proxyshard.com/eng/our-products/about-udp");

export const GH_REPO_URL = "https://github.com/ProxyShard/ShardBrowser";

/// Build accept-language chain (primary → base → English fallback).
export function deriveAcceptLanguage(loc: string): string {
  if (!loc) return "en-US,en;q=0.9";
  const base = loc.split("-")[0];
  if (loc === "en-US") return "en-US,en;q=0.9";
  return `${loc},${base};q=0.9,en-US;q=0.8,en;q=0.7`;
}

export function deriveLanguagesArray(loc: string): string[] {
  if (!loc) return ["en-US", "en"];
  const base = loc.split("-")[0];
  if (loc === "en-US") return ["en-US", "en"];
  return [loc, base, "en-US", "en"];
}

// 12-char lowercase alnum sticky-session id (matches ProxyShard's `sid` form).
export const randSid = () => {
  const a = "abcdefghijklmnopqrstuvwxyz0123456789";
  let s = "";
  for (let i = 0; i < 12; i++) s += a[Math.floor(Math.random() * a.length)];
  return s;
};

export const fmtCents = (c: number) => `$${(Number(c || 0) / 100).toFixed(2)}`;

export const fmtGB = (bytes: number) => {
  const gb = Number(bytes || 0) / 1024 ** 3;
  return `${gb >= 100 ? gb.toFixed(0) : gb.toFixed(2)} GB`;
};

export const isDcIsp = (name: string) => /datacenter|isp/i.test(name);

/// available-count product code for a base product name.
export const availCode = (name: string) => (/datacenter/i.test(name) ? "dc" : /isp/i.test(name) ? "isp" : "");

export const readTextFile = (path: string) => invoke<string>("read_text_file", { path });
