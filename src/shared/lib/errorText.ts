import { t } from "../i18n";

type Rule = {
  re: RegExp;
  key: string;
  vars?: (m: RegExpMatchArray) => Record<string, string>;
};

// Messages come from the Rust side in English, written for whoever reads a log.
// The ones a person can actually run into are turned into a sentence that says
// what happened and what to do, in the language chosen in Settings; anything
// not listed passes through unchanged.
const RULES: Rule[] = [
  { re: /this profile is in use by (.+?)\s*[—-]\s*try again/i, key: "err.profileInUse", vars: (m) => ({ who: m[1] }) },
  { re: /profile .* is already running/i, key: "err.alreadyRunning" },
  { re: /you do not hold the lock/i, key: "err.lockLost" },
  { re: /permission denied: no access/i, key: "err.noAccess" },
  { re: /permission denied: you may use/i, key: "err.noEdit" },
  { re: /permission denied: only an admin or manager/i, key: "err.noAdd" },
  { re: /permission denied: you may not move/i, key: "err.noMove" },
  { re: /permission denied: you may not delete/i, key: "err.noDelete" },
  { re: /permission denied: only the admin/i, key: "err.noAdmin" },
  { re: /permission denied/i, key: "err.noPerm" },
  { re: /sync server rejected .*\b(401|403)\b|unauthorized/i, key: "err.syncUnauthorized" },
  { re: /sync server rejected the [a-z ]+?:?\s*(\d{3})/i, key: "err.syncRejected", vars: (m) => ({ code: m[1] }) },
  { re: /contact sync server|sync server error|events rejected|library .* rejected/i, key: "err.syncUnreachable" },
  { re: /sync is not enabled/i, key: "err.syncOff" },
  { re: /proxy did not answer the UDP/i, key: "err.proxyUdp" },
  { re: /proxy did not answer/i, key: "err.proxyNoAnswer" },
  { re: /not SOCKS5/i, key: "err.proxyWrongType" },
  { re: /auth failed|no acceptable auth method/i, key: "err.proxyAuth" },
  { re: /CONNECT failed/i, key: "err.proxyConnect" },
  { re: /connect timeout|read timeout/i, key: "err.proxyTimeout" },
  { re: /browser not installed yet/i, key: "err.engineMissing" },
  { re: /every geo source failed/i, key: "err.geoFailed" },
  { re: /proxies\.json could not be read/i, key: "err.proxyFileBroken" },
];

/** A readable message for `raw`, plus the original text when it was rewritten. */
export function friendlyError(raw: string): { text: string; detail?: string } {
  const clean = raw.replace(/^Error:\s*/, "").trim();
  for (const r of RULES) {
    const m = clean.match(r.re);
    if (m) return { text: t(r.key, r.vars?.(m)), detail: clean };
  }
  return { text: clean };
}
