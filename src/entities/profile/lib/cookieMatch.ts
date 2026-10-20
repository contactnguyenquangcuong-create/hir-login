import type { CookieFile } from "../model/api";

/** How a file came to be paired with a profile. */
export type MatchHow = "id" | "name" | "order";
export type CookieMatch = { path: string; how: MatchHow };

const stem = (s: string) =>
  s.normalize("NFC").toLowerCase().replace(/\.(json|txt)$/i, "").replace(/[-_ .]*cookies?$/i, "");
const squash = (s: string) => s.replace(/[^\p{L}\p{N}]+/gu, "");

/**
 * Pairs cookie files with profiles. A file is used once, and the surest signal wins:
 *  1. the profile's id (or its first 8 characters) is in the file name — what "Export cookies"
 *     writes, so an export imports straight back;
 *  2. the file name is the profile's name;
 *  3. what is left goes in order — the first unpaired file to the first unpaired profile — which is
 *     what someone with fifty account files and fifty profiles expects.
 */
export function matchCookieFiles(
  profiles: { id: string; name: string }[],
  files: CookieFile[],
): Record<string, CookieMatch | null> {
  const out: Record<string, CookieMatch | null> = {};
  const free = [...files].sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true }));
  const take = (i: number) => free.splice(i, 1)[0];

  for (const p of profiles) {
    out[p.id] = null;
    const full = p.id.toLowerCase();
    const short = full.slice(0, 8);
    let i = free.findIndex((f) => f.name.toLowerCase().includes(full));
    if (i < 0) i = free.findIndex((f) => f.name.toLowerCase().includes(short));
    if (i >= 0) out[p.id] = { path: take(i).path, how: "id" };
  }
  for (const p of profiles) {
    if (out[p.id]) continue;
    const want = squash(p.name.normalize("NFC").toLowerCase());
    if (!want) continue;
    const i = free.findIndex((f) => squash(stem(f.name)) === want);
    if (i >= 0) out[p.id] = { path: take(i).path, how: "name" };
  }
  for (const p of profiles) {
    if (out[p.id] || free.length === 0) continue;
    out[p.id] = { path: take(0).path, how: "order" };
  }
  return out;
}
