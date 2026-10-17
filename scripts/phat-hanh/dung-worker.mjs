// Deploys (or updates) the Hir-Login update channel: Worker `hirlogin-cap-nhat` + its KV store,
// on the same Cloudflare account as HIR-IMG / HirS.
//
// Secrets stay outside the repo:
//   D:\AI\HIR-IMG-goi\cloudflare.json   { accountId, apiToken }        (shared Cloudflare account)
//   D:\AI\HirLogin-goi\cap-nhat.json    { url, uploadKey }             (written here on first run)
// The upload key is what this PC and GitHub (secret HIRLOGIN_UPLOAD_KEY) send to PUT files.
//
// Usage: node scripts/phat-hanh/dung-worker.mjs
import fs from 'fs';
import path from 'path';
import crypto from 'crypto';
import { fileURLToPath } from 'url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const TEN = 'hirlogin-cap-nhat';
const API = 'https://api.cloudflare.com/client/v4';
const CF_FILE = 'D:\\AI\\HIR-IMG-goi\\cloudflare.json';
export const GOI = 'D:\\AI\\HirLogin-goi';
const CAP_NHAT = path.join(GOI, 'cap-nhat.json');

async function cf(cfg, method, route, body, type) {
  const headers = { Authorization: 'Bearer ' + cfg.apiToken };
  if (type) headers['Content-Type'] = type;
  const r = await fetch(API + route, { method, headers, body });
  let j = null;
  try { j = await r.json(); } catch { /* not JSON */ }
  if (!r.ok || (j && j.success === false)) {
    const why = j && j.errors && j.errors.length ? j.errors.map((e) => e.code + ' ' + e.message).join('; ') : 'HTTP ' + r.status;
    throw new Error(method + ' ' + route.split('?')[0] + ': ' + why);
  }
  return j ? j.result : null;
}

async function kvNamespace(cfg) {
  const all = await cf(cfg, 'GET', `/accounts/${cfg.accountId}/storage/kv/namespaces?per_page=100`);
  const found = (all || []).find((n) => n.title === TEN);
  if (found) return found.id;
  return (await cf(cfg, 'POST', `/accounts/${cfg.accountId}/storage/kv/namespaces`, JSON.stringify({ title: TEN }), 'application/json')).id;
}

export async function dungWorker() {
  const cfg = JSON.parse(fs.readFileSync(CF_FILE, 'utf8'));
  fs.mkdirSync(GOI, { recursive: true });
  let local = {};
  try { local = JSON.parse(fs.readFileSync(CAP_NHAT, 'utf8')); } catch { /* first run */ }
  if (!local.uploadKey) local.uploadKey = crypto.randomBytes(32).toString('base64url');

  const nsId = await kvNamespace(cfg);
  const code = fs.readFileSync(path.join(HERE, 'worker.js'), 'utf8');
  const fd = new FormData();
  fd.append('metadata', new Blob([JSON.stringify({
    main_module: 'worker.js',
    compatibility_date: '2024-09-01',
    bindings: [
      { type: 'kv_namespace', name: 'KHO', namespace_id: nsId },
      { type: 'secret_text', name: 'UPLOAD_KEY', text: local.uploadKey },
    ],
  })], { type: 'application/json' }));
  fd.append('worker.js', new Blob([code], { type: 'application/javascript+module' }), 'worker.js');
  await cf(cfg, 'PUT', `/accounts/${cfg.accountId}/workers/scripts/${TEN}`, fd);
  await cf(cfg, 'POST', `/accounts/${cfg.accountId}/workers/scripts/${TEN}/subdomain`, JSON.stringify({ enabled: true }), 'application/json');
  const sub = (await cf(cfg, 'GET', `/accounts/${cfg.accountId}/workers/subdomain`)).subdomain;
  local.url = `https://${TEN}.${sub}.workers.dev`;
  fs.writeFileSync(CAP_NHAT, JSON.stringify(local, null, 2) + '\n');
  return local;
}

export function capNhat() {
  return JSON.parse(fs.readFileSync(CAP_NHAT, 'utf8'));
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const { url } = await dungWorker();
  // A fresh workers.dev name can take a moment to answer.
  for (let i = 0; i < 20; i++) {
    const r = await fetch(url + '/').catch(() => null);
    if (r && r.ok) break;
    await new Promise((res) => setTimeout(res, 1500));
  }
  const probe = await fetch(url + '/windows/x86_64/latest.json');
  console.log('Worker: ' + url + ' (windows latest.json -> HTTP ' + probe.status + ')');
}
