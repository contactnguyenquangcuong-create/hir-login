// Chạy thử web quản trị trên máy, không cần Cloudflare: SQLite trong bộ nhớ, kho key giả, dữ liệu mẫu.
//   node web-quan-tri/chay-thu.mjs [cổng]        Mật khẩu thử: thu-tren-may
// Cũng là khung dùng chung cho test.mjs (hàm dungMoiTruong).
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { DatabaseSync } from 'node:sqlite';
import { dung, baoCao, donDep, biChan, ghiSai, xoaSai, DOC, GHI } from './core.mjs';
import { xuLy, bam } from './dinh-tuyen.mjs';

const QUI = path.dirname(fileURLToPath(import.meta.url));

// Thẻ key giả trả lời như Worker `hir-key` (POST /admin/{hàm}).
export function khoKeyGia() {
  const the = new Map();
  const goi = {
    layKey: ({ key }) => the.get(key) || null,
    danhSach: ({ tien }) => [...the.values()].filter((k) => k.key.startsWith(tien + '-')),
    taoKey: (a) => { const k = { key: a.key, ghi_chu: a.ghi_chu || '', het_han: a.het_han || null, so_may: a.so_may || 1, khoa: false, tao_luc: new Date().toISOString(), ho_ten: '', gmail: '', zalo: '', kich_hoat: [] }; the.set(a.key, k); return k; },
    suaKey: ({ key, thay }) => { const k = the.get(key); if (!k) throw new Error('Không có key ' + key); Object.assign(k, thay); return k; },
    xoaKey: ({ key }) => { the.delete(key); return { ok: true }; },
    goMay: ({ key }) => { the.get(key).kich_hoat = []; return { ok: true }; },
    nhatKy: () => ({ ok: true }),
  };
  return {
    the,
    goiCuoi: [],
    async fetch(url, init) {
      const ten = new URL(url).pathname.split('/').pop();
      this.goiCuoi.push(ten);
      if ((init.headers.authorization || '') !== 'Bearer token-key-thu') return new Response(JSON.stringify({ loi: 'Không có quyền' }), { status: 401 });
      try { return new Response(JSON.stringify(goi[ten](JSON.parse(init.body))), { status: 200 }); }
      catch (e) { return new Response(JSON.stringify({ loi: e.message }), { status: 400 }); }
    },
  };
}

export async function dungMoiTruong() {
  const db = new DatabaseSync(':memory:');
  const sql = (q, ...p) => {
    const st = db.prepare(q);
    return /^\s*(select|with)|returning/i.test(q) ? st.all(...p).map((r) => ({ ...r })) : (st.run(...p), []);
  };
  dung(sql);
  const HIR_KEY = khoKeyGia();
  const env = { MAT_KHAU_BAM: await bam('thu-tren-may'), PHIEN_BI_MAT: 'bi-mat-thu', KEY_ADMIN_TOKEN: 'token-key-thu', HIR_KEY, TEN_MIEN: 'hirlogin.nguyenquangcuong.vn' };
  const hoiKey = async (key) => { const r = await HIR_KEY.fetch('https://hir-key/admin/layKey', { method: 'POST', headers: { authorization: 'Bearer token-key-thu' }, body: JSON.stringify({ key }) }); return r.json(); };
  env.KHO = () => ({
    nhanBaoCao: (b, ip) => baoCao(sql, b, ip, hoiKey, Date.now()),
    doc: (t, a) => (Object.prototype.hasOwnProperty.call(DOC, t) ? DOC[t](sql, a || {}, Date.now()) : { loi: 'Không có hàm' }),
    ghi: (t, a) => (Object.prototype.hasOwnProperty.call(GHI, t) ? GHI[t](sql, a || {}, Date.now()) : { loi: 'Không có hàm' }),
    coBiChan: (ip) => biChan(sql, ip, Date.now()),
    saiMatKhau: (ip) => ghiSai(sql, ip, Date.now()),
    dungMatKhau: (ip) => xoaSai(sql, ip),
  });
  const taiSan = {
    trang: fs.readFileSync(path.join(QUI, 'index.html'), 'utf8'),
    app: fs.readFileSync(path.join(QUI, 'app.js'), 'utf8'),
    kieu: fs.readFileSync(path.join(QUI, 'style.css'), 'utf8'),
    logo: fs.readFileSync(path.join(QUI, 'logo.png')),
  };
  return { env, sql, taiSan, HIR_KEY };
}

async function mau({ env, sql, HIR_KEY }) {
  const NGUOI = [['Trần Minh Khoa', 'DESKTOP-03'], ['Lê Thu Hà', 'DESKTOP-11'], ['Phạm Quốc Bảo', 'LAPTOP-02'], ['Ngô Mai Anh', 'DESKTOP-07'], ['Đỗ Hoàng Long', 'LAPTOP-05'], ['Vũ Thanh Tâm', 'DESKTOP-14'], ['Bùi Gia Huy', 'DESKTOP-21'], ['Hoàng Yến Nhi', 'LAPTOP-09']];
  const ten = ['AFF-SHOPEE', 'FB-ADS', 'TIKTOK-CONTENT', 'GOOGLE-ADS', 'CSKH-ZALO'];
  const thuMuc = ['Affiliate A', 'Marketing', 'Content', 'Marketing', 'CSKH'];
  const profiles = [];
  for (let i = 0; i < 64; i++) profiles.push({ id: `p${i}`, ten: `${ten[i % 5]}-${String(i + 1).padStart(3, '0')}`, thu_muc: thuMuc[i % 5] });
  const k = (i) => `HLG-MAU${i}-ABCD-EFG${i}`;
  NGUOI.forEach(([ho, may], i) => {
    HIR_KEY.the.set(k(i), { key: k(i), ghi_chu: `Team ${thuMuc[i % 5]}`, het_han: i === 2 ? new Date(Date.now() + 3 * 86400000).toISOString() : i === 6 ? new Date(Date.now() - 86400000).toISOString() : null, so_may: 2, khoa: i === 7, tao_luc: new Date().toISOString(), ho_ten: ho, gmail: '', zalo: `09${i}1234567`, kich_hoat: [{ ma_may: `may-may-may-may-${i}`, ten_may: may, lan_dau: new Date().toISOString(), lan_cuoi: new Date().toISOString() }] });
  });
  HIR_KEY.the.set('HLG-MOIT-ABCD-0000', { key: 'HLG-MOIT-ABCD-0000', ghi_chu: 'Chưa phát', het_han: null, so_may: 3, khoa: false, tao_luc: new Date().toISOString(), ho_ten: '', gmail: '', zalo: '', kich_hoat: ['A', 'B', 'C'].map((x) => ({ ma_may: `chua-bao-cao-may-${x}`, ten_may: `MAY-CU-${x}`, lan_dau: new Date().toISOString(), lan_cuoi: new Date().toISOString() })) });
  const kho = env.KHO();
  for (let i = 0; i < NGUOI.length; i++) {
    const mo = i < 5 ? profiles.slice(i * 3, i * 3 + (i % 3) + 1).map((p, j) => ({ ...p, giay: 600 + i * 1700 + j * 900 })) : [];
    const b = { key: k(i), ma_may: `may-may-may-may-${i}`, ten_may: NGUOI[i][1], phien_ban: i === 3 ? '0.2.40' : '0.2.43', he_dieu_hanh: i % 3 === 2 ? 'macos' : 'windows', may_chu: i < 4 ? '100.71.20.8:8787' : i < 6 ? '100.92.4.11:8787' : '', tong_profile: 30 + i * 3, cho_tai_len: i === 4 ? 2 : 0, dang_mo: mo, tu_dong: i === 1 ? [{ du_an: 'Đăng bài hàng loạt', luong: 3, dong_ok: 42, dong_loi: 3, tu_luc: Date.now() }] : [] };
    await kho.nhanBaoCao(b, `203.0.113.${i + 5}`);
  }
  // Lịch sử giả cho báo cáo.
  const bay = Date.now();
  for (let n = 0; n < 14; n++) for (let i = 0; i < 6; i++) {
    const bd = bay - n * 86400000 - (i + 1) * 3600000 * 2;
    const dai = (20 + ((n * 7 + i * 13) % 120)) * 60000;
    const p = profiles[(n + i * 5) % 64];
    if (bd + dai > bay) continue;
    sql('insert into phien(ma_may, key, nhan_su, ten_may, profile_id, ten, thu_muc, nen_tang, bat_dau, ket_thuc) values (?,?,?,?,?,?,?,?,?,?)', `may-may-may-may-${i}`, k(i), NGUOI[i][0], NGUOI[i][1], p.id, p.ten, p.thu_muc, i % 5 === 0 ? 'shopee' : i % 5 === 1 ? 'facebook' : i % 5 === 2 ? 'tiktok' : i % 5 === 3 ? 'google' : 'khac', bd, bd + dai);
    sql('insert into nhat_ky(luc, viec, nhan_su, ten_may, ma_may, doi_tuong, chi_tiet) values (?,?,?,?,?,?,?)', bd, 'Mở profile', NGUOI[i][0], NGUOI[i][1], `may-may-may-may-${i}`, p.ten, p.thu_muc);
    sql('insert into nhat_ky(luc, viec, nhan_su, ten_may, ma_may, doi_tuong, chi_tiet) values (?,?,?,?,?,?,?)', bd + dai, 'Đóng profile', NGUOI[i][0], NGUOI[i][1], `may-may-may-may-${i}`, p.ten, p.thu_muc);
  }
  // Tháng trước, để thấy phần so sánh.
  sql('insert or replace into so_lieu_ngay(ngay, tong_profile) values (?, ?)', bay - 40 * 86400000, 280);
  for (let n = 1; n <= 14; n++) sql('insert or replace into dinh_ngay(ngay, may_bat, dang_mo) values (?, ?, ?)', Math.floor(bay / 86400000) * 86400000 - 7 * 3600000 - n * 86400000, 4 + ((n * 3) % 5), 6 + ((n * 7) % 9));
  // Hai máy đã tắt.
  sql('update may set lan_cuoi = ? where ma_may in (?,?)', bay - 3600000 * 5, 'may-may-may-may-6', 'may-may-may-may-7');
  donDep(sql, bay);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const m = await dungMoiTruong();
  if (!process.argv.includes('--trong')) await mau(m);
  const cong = Number(process.argv.find((a) => /^\d+$/.test(a))) || 8799;
  http.createServer(async (q, s) => {
    const than = [];
    for await (const c of q) than.push(c);
    const req = new Request(`http://${q.headers.host}${q.url}`, { method: q.method, headers: q.headers, body: q.method === 'GET' || q.method === 'HEAD' ? undefined : Buffer.concat(than) });
    const r = await xuLy(req, m.env, m.taiSan);
    s.writeHead(r.status, Object.fromEntries(r.headers));
    s.end(Buffer.from(await r.arrayBuffer()));
  }).listen(cong, () => console.log(`http://localhost:${cong}  (mật khẩu thử: thu-tren-may)`));
}
