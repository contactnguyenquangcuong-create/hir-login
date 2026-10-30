// Đưa web quản trị (Worker `hirlogin-web`) lên Cloudflare, cùng tài khoản với `hir-key`.
//   node web-quan-tri/trien-khai.mjs                 Cập nhật (giữ nguyên mật khẩu).
//   node web-quan-tri/trien-khai.mjs --mat-khau-moi  Đổi mật khẩu quản trị và đăng xuất mọi nơi.
//
// Bí mật nằm ngoài repo:
//   D:\AI\HIR-IMG-goi\cloudflare.json   { accountId, apiToken }
//   D:\AI\HIR-IMG-goi\hirkey.json       { adminToken }  của Worker hir-key
//   D:\AI\HirLogin-goi\quan-tri.json    { matKhau, phienBiMat, migTag, url }  (tự sinh lần đầu; mật khẩu nằm ở đây)
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { fileURLToPath } from 'node:url';

const QUI = path.dirname(fileURLToPath(import.meta.url));
const TEN = 'hirlogin-web';
const TEN_MIEN = 'hirlogin.nguyenquangcuong.vn';
const API = 'https://api.cloudflare.com/client/v4';
const FILE = 'D:\\AI\\HirLogin-goi\\quan-tri.json';
const doc = (f) => JSON.parse(fs.readFileSync(f, 'utf8'));

async function cf(cfg, pt, duong, body, loai) {
  const headers = { Authorization: 'Bearer ' + cfg.apiToken };
  if (loai) headers['Content-Type'] = loai;
  const r = await fetch(API + duong, { method: pt, headers, body });
  let j = null;
  try { j = await r.json(); } catch (_) { /* không phải JSON */ }
  if (!r.ok || (j && j.success === false)) throw new Error(pt + ' ' + duong.split('?')[0] + ': ' + (j && j.errors && j.errors.length ? j.errors.map((e) => e.code + ' ' + e.message).join('; ') : 'HTTP ' + r.status));
  return j ? j.result : null;
}

const cfg = doc('D:\\AI\\HIR-IMG-goi\\cloudflare.json');
const hk = doc('D:\\AI\\HIR-IMG-goi\\hirkey.json');
let qt = {};
try { qt = doc(FILE); } catch (_) { /* lần đầu */ }
const moi = process.argv.includes('--mat-khau-moi');
if (!qt.matKhau || moi) qt.matKhau = crypto.randomBytes(15).toString('base64url');
if (!qt.phienBiMat || moi) qt.phienBiMat = crypto.randomBytes(32).toString('hex');
if (!qt.chuyenTiep) qt.chuyenTiep = crypto.randomBytes(32).toString('hex');
const bam = crypto.createHash('sha256').update(qt.matKhau).digest('base64url');

const meta = {
  main_module: 'worker.mjs',
  compatibility_date: '2024-09-01',
  bindings: [
    { type: 'durable_object_namespace', name: 'KHO', class_name: 'KhoQuanTri' },
    { type: 'service', name: 'HIR_KEY', service: 'hir-key', environment: 'production' },
    { type: 'secret_text', name: 'KEY_ADMIN_TOKEN', text: hk.adminToken },
    { type: 'secret_text', name: 'MAT_KHAU_BAM', text: bam },
    { type: 'secret_text', name: 'PHIEN_BI_MAT', text: qt.phienBiMat },
    { type: 'secret_text', name: 'CHUYEN_TIEP_BI_MAT', text: qt.chuyenTiep },
    { type: 'plain_text', name: 'TEN_MIEN', text: TEN_MIEN },
  ],
};
if (!qt.migTag) meta.migrations = { new_tag: 'v1', new_sqlite_classes: ['KhoQuanTri'] };

const fd = new FormData();
fd.append('metadata', new Blob([JSON.stringify(meta)], { type: 'application/json' }));
for (const f of ['worker.mjs', 'core.mjs', 'dinh-tuyen.mjs', 'key.mjs']) fd.append(f, new Blob([fs.readFileSync(path.join(QUI, f))], { type: 'application/javascript+module' }), f);
for (const [ten, nguon] of [['index.html', 'index.html'], ['app.js.txt', 'app.js'], ['style.css', 'style.css']]) fd.append(ten, new Blob([fs.readFileSync(path.join(QUI, nguon))], { type: 'text/plain' }), ten);
fd.append('logo.png', new Blob([fs.readFileSync(path.join(QUI, 'logo.png'))], { type: 'application/octet-stream' }), 'logo.png');

await cf(cfg, 'PUT', `/accounts/${cfg.accountId}/workers/scripts/${TEN}`, fd);
await cf(cfg, 'POST', `/accounts/${cfg.accountId}/workers/scripts/${TEN}/subdomain`, JSON.stringify({ enabled: true }), 'application/json');
const sub = (await cf(cfg, 'GET', `/accounts/${cfg.accountId}/workers/subdomain`)).subdomain;
qt.url = `https://${TEN}.${sub}.workers.dev`;
qt.migTag = 'v1';
fs.mkdirSync(path.dirname(FILE), { recursive: true });
fs.writeFileSync(FILE, JSON.stringify(qt, null, 2) + '\n');

let ok = null;
for (let i = 0; i < 20 && !ok; i++) {
  const r = await fetch(qt.url + '/health').catch(() => null);
  if (r && r.ok) ok = await r.json();
  else await new Promise((res) => setTimeout(res, 1500));
}
console.log('ĐÃ TRIỂN KHAI ' + qt.url + (ok ? ' (đang chạy)' : ' (chưa trả lời, thử lại sau ít phút)'));
console.log('Mật khẩu quản trị nằm trong ' + FILE);
