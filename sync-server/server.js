// Hir-Login profile sync server — zero-dependency, single file.
// Stores profile bundles (zip: <id>.json + user-data/) on disk and hands out
// per-profile locks so two machines never open the same profile at once.
//
// Run:
//   SYNC_TOKEN=some-long-secret node server.js
// Env vars:
//   PORT              default 8787
//   SYNC_TOKEN        required — shared bearer token for the whole team
//   DATA_DIR          default ./data
//   LOCK_TTL_SECONDS  default 21600 (6h) — auto-expires a lock if a client
//                     crashes without releasing it
//
// Put this behind a reverse proxy (Caddy/nginx/Cloudflare Tunnel) for HTTPS.
// Caddy example:
//   sync.yourcompany.com {
//     reverse_proxy 127.0.0.1:8787
//   }

const http = require("http");
const fs = require("fs");
const path = require("path");
const crypto = require("crypto");

const PORT = parseInt(process.env.PORT || "8787", 10);
const TOKEN = process.env.SYNC_TOKEN;
const DATA_DIR = path.resolve(process.env.DATA_DIR || path.join(__dirname, "data"));
const BUNDLES_DIR = path.join(DATA_DIR, "bundles");
const LOCKS_FILE = path.join(DATA_DIR, "locks.json");
const META_FILE = path.join(DATA_DIR, "meta.json");
const LOCK_TTL_MS = parseInt(process.env.LOCK_TTL_SECONDS || "21600", 10) * 1000;

if (!TOKEN) {
  console.error("SYNC_TOKEN env var is required — refusing to start with no auth.");
  process.exit(1);
}

fs.mkdirSync(BUNDLES_DIR, { recursive: true });

function loadJson(file, fallback) {
  try {
    return JSON.parse(fs.readFileSync(file, "utf8"));
  } catch {
    return fallback;
  }
}
function saveJson(file, obj) {
  fs.writeFileSync(file, JSON.stringify(obj, null, 2));
}

let locks = loadJson(LOCKS_FILE, {}); // { [id]: { holder, acquiredAt, expiresAt } }
let meta = loadJson(META_FILE, {}); // { [id]: { updatedAt, updatedBy, sizeBytes } }

function persistLocks() { saveJson(LOCKS_FILE, locks); }
function persistMeta() { saveJson(META_FILE, meta); }

function isExpired(lock) {
  return !lock || Date.now() > lock.expiresAt;
}

function safeId(id) {
  // profile ids are uuids/slugs in this app — reject anything path-traversal-shaped
  return /^[A-Za-z0-9_-]{1,128}$/.test(id);
}

function timingSafeEqual(a, b) {
  const ab = Buffer.from(a);
  const bb = Buffer.from(b);
  if (ab.length !== bb.length) return false;
  return crypto.timingSafeEqual(ab, bb);
}

function checkAuth(req) {
  const h = req.headers["authorization"] || "";
  const m = /^Bearer\s+(.+)$/.exec(h);
  if (!m) return false;
  return timingSafeEqual(m[1], TOKEN);
}

function sendJson(res, status, obj) {
  const body = JSON.stringify(obj);
  res.writeHead(status, { "Content-Type": "application/json", "Content-Length": Buffer.byteLength(body) });
  res.end(body);
}

function readBody(req, maxBytes) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    let size = 0;
    req.on("data", (c) => {
      size += c.length;
      if (size > maxBytes) {
        reject(new Error("payload too large"));
        req.destroy();
        return;
      }
      chunks.push(c);
    });
    req.on("end", () => resolve(Buffer.concat(chunks)));
    req.on("error", reject);
  });
}

const MAX_BUNDLE_BYTES = 2 * 1024 * 1024 * 1024; // 2GB ceiling per profile bundle

const server = http.createServer(async (req, res) => {
  try {
    const url = new URL(req.url, "http://localhost");
    const parts = url.pathname.split("/").filter(Boolean);

    if (req.method === "GET" && parts.length === 1 && parts[0] === "health") {
      return sendJson(res, 200, { ok: true });
    }

    if (!checkAuth(req)) {
      return sendJson(res, 401, { ok: false, error: "unauthorized" });
    }

    // GET /profiles
    if (req.method === "GET" && parts.length === 1 && parts[0] === "profiles") {
      const ids = new Set([...Object.keys(meta), ...Object.keys(locks)]);
      const list = [...ids].map((id) => {
        const lock = locks[id];
        const held = lock && !isExpired(lock);
        return {
          id,
          updatedAt: meta[id]?.updatedAt ?? null,
          updatedBy: meta[id]?.updatedBy ?? null,
          sizeBytes: meta[id]?.sizeBytes ?? null,
          locked: !!held,
          holder: held ? lock.holder : null,
          lockExpiresAt: held ? lock.expiresAt : null,
        };
      });
      return sendJson(res, 200, { ok: true, profiles: list });
    }

    // /profiles/:id/...
    if (parts.length === 3 && parts[0] === "profiles") {
      const id = parts[1];
      const action = parts[2];
      if (!safeId(id)) return sendJson(res, 400, { ok: false, error: "bad id" });

      if (req.method === "POST" && action === "lock") {
        const body = JSON.parse((await readBody(req, 1024)).toString("utf8") || "{}");
        const holder = String(body.holder || "").slice(0, 200);
        if (!holder) return sendJson(res, 400, { ok: false, error: "holder required" });
        const existing = locks[id];
        if (existing && !isExpired(existing) && existing.holder !== holder) {
          return sendJson(res, 409, { ok: false, holder: existing.holder, expiresAt: existing.expiresAt });
        }
        const expiresAt = Date.now() + LOCK_TTL_MS;
        locks[id] = { holder, acquiredAt: Date.now(), expiresAt };
        persistLocks();
        return sendJson(res, 200, { ok: true, expiresAt });
      }

      if (req.method === "POST" && action === "unlock") {
        const body = JSON.parse((await readBody(req, 1024)).toString("utf8") || "{}");
        const holder = String(body.holder || "");
        const force = body.force === true;
        const existing = locks[id];
        if (existing && !isExpired(existing) && existing.holder !== holder && !force) {
          return sendJson(res, 403, { ok: false, error: "held by another holder", holder: existing.holder });
        }
        delete locks[id];
        persistLocks();
        return sendJson(res, 200, { ok: true });
      }

      if (action === "bundle") {
        const filePath = path.join(BUNDLES_DIR, `${id}.zip`);

        if (req.method === "GET") {
          if (!fs.existsSync(filePath)) return sendJson(res, 404, { ok: false, error: "no bundle yet" });
          const stat = fs.statSync(filePath);
          res.writeHead(200, { "Content-Type": "application/zip", "Content-Length": stat.size });
          fs.createReadStream(filePath).pipe(res);
          return;
        }

        if (req.method === "PUT") {
          const holder = String(req.headers["x-sync-holder"] || "");
          const existing = locks[id];
          if (!existing || isExpired(existing) || existing.holder !== holder) {
            return sendJson(res, 403, { ok: false, error: "you do not hold the lock for this profile" });
          }
          const body = await readBody(req, MAX_BUNDLE_BYTES);
          fs.writeFileSync(filePath, body);
          meta[id] = { updatedAt: new Date().toISOString(), updatedBy: holder, sizeBytes: body.length };
          persistMeta();
          // Refresh the lock TTL on every successful push (extends the session).
          locks[id] = { ...existing, expiresAt: Date.now() + LOCK_TTL_MS };
          persistLocks();
          return sendJson(res, 200, { ok: true, sizeBytes: body.length });
        }
      }
    }

    sendJson(res, 404, { ok: false, error: "not found" });
  } catch (e) {
    sendJson(res, 500, { ok: false, error: String(e && e.message ? e.message : e) });
  }
});

server.listen(PORT, () => {
  console.log(`Hir-Login sync server listening on :${PORT}, data dir: ${DATA_DIR}`);
});
