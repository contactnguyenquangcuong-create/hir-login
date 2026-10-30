// Định tuyến của Worker `hirlogin-web`, tách khỏi worker.mjs để chạy được cả trên máy khi test.
//  - POST /api/bao-cao   Mọi máy cài Hir-Login gửi tình trạng về đây (xác thực bằng key + mã máy).
//  - /api/qt/*           Trang quản trị, cần đăng nhập: số liệu, và quản lý key HLG- qua Worker `hir-key`.
//  - Còn lại             Giao diện web.
// env.KHO(): Durable Object; env.HIR_KEY: Worker `hir-key` (service binding). taiSan: các file giao diện.
import { hanhDongKey } from './key.mjs';

const PHIEN_MS = 12 * 3600 * 1000;
const COOKIE = 'hir_qt';
const BODY_TOI_DA = 1024 * 1024;

const BAO_MAT = {
  'x-content-type-options': 'nosniff',
  'x-frame-options': 'DENY',
  'referrer-policy': 'same-origin',
  'content-security-policy': "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; font-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
};
const json = (o, status, them) => new Response(JSON.stringify(o), { status: status || 200, headers: { 'content-type': 'application/json; charset=utf-8', 'cache-control': 'no-store', ...BAO_MAT, ...(them || {}) } });
const tinh = (noi, loai, luuLau) => new Response(noi, { headers: { 'content-type': loai, 'cache-control': luuLau ? 'public, max-age=86400' : 'no-cache', ...BAO_MAT } });

export async function goiKey(env, ten, body) {
  const r = await env.HIR_KEY.fetch('https://hir-key/admin/' + ten, {
    method: 'POST',
    headers: { 'content-type': 'application/json', authorization: 'Bearer ' + env.KEY_ADMIN_TOKEN },
    body: JSON.stringify(body || {}),
  });
  const j = await r.json().catch(() => null);
  if (!r.ok) {
    const e = new Error((j && j.loi) || 'Máy chủ key báo lỗi ' + r.status);
    e.ma = r.status;
    throw e;
  }
  return j;
}

// ---- Phiên đăng nhập: cookie có chữ ký, không lưu gì trên server ----
const b64 = (buf) => btoa(String.fromCharCode(...new Uint8Array(buf))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
async function kyTen(bimat, s) {
  const k = await crypto.subtle.importKey('raw', new TextEncoder().encode(bimat), { name: 'HMAC', hash: 'SHA-256' }, false, ['sign']);
  return b64(await crypto.subtle.sign('HMAC', k, new TextEncoder().encode(s)));
}
export async function bam(s) {
  return b64(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(s)));
}
function giongNhau(a, b) {
  if (typeof a !== 'string' || typeof b !== 'string' || a.length !== b.length) return false;
  let r = 0;
  for (let i = 0; i < a.length; i++) r |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return r === 0;
}
function docCookie(req) {
  for (const c of (req.headers.get('cookie') || '').split(/;\s*/)) if (c.startsWith(COOKIE + '=')) return c.slice(COOKIE.length + 1);
  return '';
}
async function daDangNhap(req, env) {
  const [han, chuKy] = docCookie(req).split('.');
  if (!han || !chuKy || Number(han) < Date.now()) return false;
  return giongNhau(chuKy, await kyTen(env.PHIEN_BI_MAT, han));
}
const phanCookie = (u, giaTri, tuoi) => `${COOKIE}=${giaTri}; Path=/; HttpOnly; SameSite=Strict; Max-Age=${tuoi}` + (u.protocol === 'https:' ? '; Secure' : '');

// Lệnh ghi chỉ nhận từ chính trang này. (Tên miền có thể đi qua một Worker chuyển tiếp, nên so cả tên miền đã khai.)
function dungNguon(req, u, env) {
  const nguon = req.headers.get('origin');
  if (!nguon) return false;
  try {
    const h = new URL(nguon).host;
    return h === u.host || (!!env.TEN_MIEN && h === env.TEN_MIEN);
  } catch (_) { return false; }
}

async function docBody(req) {
  const t = await req.text();
  if (t.length > BODY_TOI_DA) throw Object.assign(new Error('Dữ liệu quá lớn'), { ma: 413 });
  try { return t ? JSON.parse(t) : {}; } catch (_) { throw Object.assign(new Error('Sai JSON'), { ma: 400 }); }
}

export async function xuLy(req, env, taiSan) {
  const u = new URL(req.url);
  const p = u.pathname.replace(/\/+$/, '') || '/';
  // Đi qua Worker chuyển tiếp của tên miền thì địa chỉ thật nằm ở x-hir-ip, chỉ tin khi có đúng mã bí mật.
  const quaChuyenTiep = !!env.CHUYEN_TIEP_BI_MAT && giongNhau(req.headers.get('x-hir-chuyen') || '', env.CHUYEN_TIEP_BI_MAT);
  const ip = (quaChuyenTiep ? req.headers.get('x-hir-ip') : req.headers.get('cf-connecting-ip')) || '';
  const kho = env.KHO();

  if (req.method === 'GET') {
    if (p === '/') return tinh(taiSan.trang, 'text/html; charset=utf-8');
    if (p === '/app.js') return tinh(taiSan.app, 'application/javascript; charset=utf-8');
    if (p === '/style.css') return tinh(taiSan.kieu, 'text/css; charset=utf-8');
    if (p === '/logo.png') return tinh(taiSan.logo, 'image/png', true);
    if (p === '/health') return json({ ok: true, dich_vu: 'hirlogin-web' });
  }

  if (p === '/api/bao-cao' && req.method === 'POST') {
    let b;
    try { b = await docBody(req); } catch (e) { return json({ ok: false, loi: e.message }, e.ma || 400); }
    const { ma, ...con } = await kho.nhanBaoCao(b, ip);
    return json(con, ma || 200);
  }

  if (p === '/api/dang-nhap' && req.method === 'POST') {
    if (!dungNguon(req, u, env)) return json({ loi: 'Sai nguồn' }, 403);
    if (await kho.coBiChan(ip)) return json({ loi: 'Sai quá nhiều lần. Thử lại sau 15 phút.' }, 429);
    const b = await docBody(req).catch(() => ({}));
    if (!giongNhau(await bam(String(b.mat_khau || '')), env.MAT_KHAU_BAM)) {
      await kho.saiMatKhau(ip);
      return json({ loi: 'Sai mật khẩu' }, 401);
    }
    await kho.dungMatKhau(ip);
    const han = String(Date.now() + PHIEN_MS);
    return json({ ok: true }, 200, { 'set-cookie': phanCookie(u, han + '.' + (await kyTen(env.PHIEN_BI_MAT, han)), PHIEN_MS / 1000) });
  }
  if (p === '/api/dang-xuat' && req.method === 'POST') {
    if (!dungNguon(req, u, env)) return json({ loi: 'Sai nguồn' }, 403);
    return json({ ok: true }, 200, { 'set-cookie': phanCookie(u, '', 0) });
  }

  const m = /^\/api\/qt\/([a-zA-Z]+)$/.exec(p);
  if (m) {
    if (!(await daDangNhap(req, env))) return json({ loi: 'Chưa đăng nhập' }, 401);
    if (req.method === 'GET') return json(await kho.doc(m[1], Object.fromEntries(u.searchParams)));
    if (req.method !== 'POST' || !dungNguon(req, u, env)) return json({ loi: 'Không được phép' }, 403);
    try {
      const b = await docBody(req);
      if (m[1] === 'key') return json(await hanhDongKey(b, (ten, a) => goiKey(env, ten, a), kho));
      return json(await kho.ghi(m[1], b));
    } catch (e) {
      return json({ loi: String((e && e.message) || e) }, e && e.ma ? e.ma : 400);
    }
  }
  return json({ loi: 'Không có đường này' }, 404);
}
