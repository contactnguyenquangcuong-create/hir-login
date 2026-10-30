// node --test web-quan-tri/test.mjs
import test from 'node:test';
import assert from 'node:assert/strict';
import { dungMoiTruong } from './chay-thu.mjs';
import { xuLy } from './dinh-tuyen.mjs';
import { donDep, nenTang, khoang } from './core.mjs';

const GOC = 'https://hirlogin-web.example.workers.dev';
const KEY = 'HLG-AAAA-BBBB-CCCC', MAY = 'may-may-may-may-01';

async function dung() {
  const m = await dungMoiTruong();
  m.HIR_KEY.the.set(KEY, { key: KEY, ghi_chu: 'Team A', het_han: null, so_may: 2, khoa: false, tao_luc: '', ho_ten: 'Trần Khoa', gmail: '', zalo: '', kich_hoat: [{ ma_may: MAY, ten_may: 'PC-01', lan_dau: '', lan_cuoi: '' }] });
  m.goi = async (duong, body, them) => {
    const init = body === undefined ? {} : { method: 'POST', body: JSON.stringify(body), headers: { 'content-type': 'application/json', origin: GOC, ...(them || {}) } };
    if (body === undefined && them) init.headers = them;
    const r = await xuLy(new Request(GOC + duong, init), m.env, m.taiSan);
    return { ma: r.status, j: await r.clone().json().catch(() => null), r };
  };
  m.baoCao = (b) => m.goi('/api/bao-cao', { key: KEY, ma_may: MAY, ten_may: 'PC-01', phien_ban: '0.2.43', he_dieu_hanh: 'windows', tong_profile: 2, dang_mo: [], tu_dong: [], ...b });
  m.dangNhap = async () => {
    const r = await m.goi('/api/dang-nhap', { mat_khau: 'thu-tren-may' });
    assert.equal(r.ma, 200);
    return { cookie: r.r.headers.get('set-cookie').split(';')[0] };
  };
  return m;
}

test('máy lạ, key sai, mã máy không khớp đều không báo cáo được', async () => {
  const m = await dung();
  assert.equal((await m.baoCao({ key: 'HRS-AAAA-BBBB-CCCC' })).ma, 400, 'key của tool khác');
  assert.equal((await m.baoCao({ key: 'HLG-ZZZZ-ZZZZ-ZZZZ' })).ma, 403, 'key không có thật');
  assert.equal((await m.baoCao({ ma_may: 'may-khac-may-khac-99' })).ma, 403, 'máy chưa kích hoạt key này');
  assert.equal((await m.baoCao({})).ma, 200);
  const nguoi = m.env.KHO().doc('thietBi').hang;
  assert.equal(nguoi.length, 1);
  assert.equal(nguoi[0].nhan_su, 'Trần Khoa', 'tên lấy từ key, không tin máy gửi');
});

test('chỉ quản lý: không nhận danh sách profile', async () => {
  const m = await dung();
  m.HIR_KEY.the.get(KEY).kich_hoat.push({ ma_may: 'may-may-may-may-02', ten_may: 'PC-02', lan_dau: '', lan_cuoi: '' });
  const r = await m.baoCao({ tong_profile: 40, profiles: [{ id: 'la', ten: 'KHONG-DUOC-LUU', thu_muc: 'x' }], dang_mo: [{ id: 'p1', ten: 'FB-001', thu_muc: 'Ads', giay: 60 }] });
  assert.equal(r.ma, 200);
  assert.equal(r.j.can_ds, undefined, 'máy không còn bị đòi danh sách');
  assert.equal(m.sql('select count(*) n from phien')[0].n, 1, 'chỉ profile đang mở mới có phiên');
  assert.equal(JSON.stringify(m.env.KHO().doc('tongQuan')).includes('KHONG-DUOC-LUU'), false, 'có gửi cũng không lưu');
  const bangs = m.sql("select name from sqlite_master where type = 'table'").map((x) => x.name);
  assert.equal(bangs.includes('profile'), false, 'không có bảng danh sách profile');
  await m.baoCao({ ma_may: 'may-may-may-may-02', ten_may: 'PC-02', tong_profile: 2, dang_mo: [{ id: 'p1', ten: 'FB-001', thu_muc: 'Ads', giay: 5 }] });
  const q = m.env.KHO().doc('tongQuan');
  assert.equal(q.so.tong_profile, 42, 'tổng là số máy tự báo');
  assert.equal(q.so.dang_su_dung, 1, 'một profile mở trên hai máy chỉ tính một');
  assert.equal(m.env.KHO().doc('chiTietProfile', { id: 'p1' }).lich_su.length, 2, 'bấm vào profile đang mở vẫn xem được lịch sử');
  assert.equal(m.env.KHO().doc('profiles', {}).loi, 'Không có hàm', 'trang Profile đã bỏ');
  assert.equal(m.env.KHO().doc('nhom', {}).loi, 'Không có hàm', 'trang Nhóm đã bỏ');
});

test('5 chỉ số tổng quan: có số tháng trước để so sánh', async () => {
  const m = await dung();
  await m.baoCao({ tong_profile: 120, dang_mo: [{ id: 'p1', ten: 'A', thu_muc: '', giay: 1 }] });
  let s = m.env.KHO().doc('tongQuan').so;
  assert.equal(s.tong_profile, 120);
  assert.equal(s.tong_profile_truoc, null, 'chưa có số cũ thì không so');
  assert.equal(s.dang_su_dung, 1);
  assert.equal(s.nhan_su, 1);
  assert.equal(s.nhan_su_moi, 1);
  assert.equal(s.thiet_bi, 1);
  assert.equal(s.truc_tuyen, 1);
  assert.equal(s.luot, 1);
  donDep(m.sql, Date.now());
  assert.equal(m.sql('select tong_profile from so_lieu_ngay')[0].tong_profile, 120, 'chốt số mỗi ngày');
  m.sql('update so_lieu_ngay set ngay = ?', Date.now() - 45 * 86400000);
  m.sql("insert into phien(ma_may, key, profile_id, bat_dau, ket_thuc) values ('x', 'k', 'q', ?, ?)", Date.now() - 45 * 86400000, Date.now() - 45 * 86400000 + 1000);
  s = m.env.KHO().doc('tongQuan').so;
  assert.equal(s.tong_profile_truoc, 120);
  assert.equal(s.luot_truoc + s.luot >= 1, true);
});

test('mở, đóng profile thành phiên và lịch sử; máy im thì tự đóng ở lần báo cáo cuối', async () => {
  const m = await dung();
  await m.baoCao({ dang_mo: [{ id: 'p1', ten: 'FB-001', thu_muc: 'Ads', giay: 120 }, { id: 'p2', ten: 'S-2', thu_muc: 'Aff', giay: 10 }] });
  let q = m.env.KHO().doc('tongQuan');
  assert.equal(q.so.dang_su_dung, 2);
  assert.equal(q.so.luot, 2);
  const tu = m.sql("select tu_luc from dang_mo where id = 'p1'")[0].tu_luc;
  assert.ok(Math.abs(Date.now() - 120000 - tu) < 5000, 'giờ mở tính ngược từ thời gian đã chạy');
  await m.baoCao({ dang_mo: [{ id: 'p1', ten: 'FB-001', thu_muc: 'Ads', giay: 300 }] });
  q = m.env.KHO().doc('tongQuan');
  assert.equal(q.so.dang_su_dung, 1);
  assert.equal(q.so.luot, 2, 'báo cáo tiếp theo không tính thêm lượt');
  assert.ok(q.hoat_dong.some((n) => n.viec === 'Đóng profile' && n.doi_tuong === 'S-2'));
  // Im quá lâu.
  const cuoi = Date.now() - 3600000;
  m.sql('update may set lan_cuoi = ?', cuoi);
  donDep(m.sql, Date.now());
  q = m.env.KHO().doc('tongQuan');
  assert.equal(q.so.dang_su_dung, 0);
  assert.equal(q.so.truc_tuyen, 0);
  assert.equal(m.sql("select ket_thuc from phien where profile_id = 'p1'")[0].ket_thuc, cuoi);
  // Bật lại.
  await m.baoCao({ dang_mo: [{ id: 'p1', ten: 'FB-001', thu_muc: 'Ads', giay: 5 }] });
  q = m.env.KHO().doc('tongQuan');
  assert.equal(q.so.truc_tuyen, 1);
  assert.equal(q.so.luot, 3);
  assert.ok(q.hoat_dong.some((n) => n.viec === 'Bật phần mềm'));
});

test('key bị khoá trên web: máy biết ngay ở lần báo cáo sau', async () => {
  const m = await dung();
  const a = await m.dangNhap();
  assert.equal((await m.baoCao({})).j.key_loi, null);
  assert.equal((await m.goi('/api/qt/key', { viec: 'khoa', key: KEY }, a)).ma, 200);
  assert.equal((await m.baoCao({})).j.key_loi, 'BI_KHOA', 'không chờ hết 30 phút đã nhớ');
  await m.goi('/api/qt/key', { viec: 'mo', key: KEY }, a);
  assert.equal((await m.baoCao({})).j.key_loi, null);
  await m.goi('/api/qt/key', { viec: 'goMay', key: KEY }, a);
  assert.equal((await m.baoCao({})).ma, 403);
});

test('máy chủ key lỗi: máy đã biết vẫn báo cáo được, máy lạ thì chờ', async () => {
  const m = await dung();
  await m.baoCao({});
  m.sql('update xac_thuc set luc = 0');
  const f = m.HIR_KEY.fetch;
  m.HIR_KEY.fetch = async () => { throw new Error('mạng'); };
  assert.equal((await m.baoCao({})).ma, 200);
  assert.equal((await m.baoCao({ ma_may: 'may-khac-may-khac-99' })).ma, 503);
  m.HIR_KEY.fetch = f;
});

test('không đăng nhập thì không đọc, không sửa được gì', async () => {
  const m = await dung();
  for (const d of ['tongQuan', 'thietBi', 'lichSu', 'cauHinh']) assert.equal((await m.goi('/api/qt/' + d)).ma, 401);
  assert.equal((await m.goi('/api/qt/key', { viec: 'danhSach' })).ma, 401);
  assert.equal((await m.goi('/api/qt/tongQuan', undefined, { cookie: 'hir_qt=9999999999999.gia' })).ma, 401, 'cookie tự chế');
  const a = await m.dangNhap();
  const het = a.cookie.replace(/=\d+\./, '=1.');
  assert.equal((await m.goi('/api/qt/tongQuan', undefined, { cookie: het })).ma, 401, 'đổi hạn thì sai chữ ký');
  assert.equal((await m.goi('/api/qt/tongQuan', undefined, a)).ma, 200);
  assert.equal((await m.goi('/api/qt/key', { viec: 'danhSach' }, { ...a, origin: 'https://evil.example' })).ma, 403, 'trang lạ không gọi lệnh ghi được');
  assert.equal((await m.goi('/api/qt/key', { viec: 'danhSach' }, { ...a, origin: 'https://hirlogin.nguyenquangcuong.vn' })).ma, 200, 'tên miền đã khai thì được');
  assert.equal((await m.goi('/api/qt/hamLa', {}, a)).j.loi, 'Không có hàm');
  assert.equal((await m.goi('/api/qt/__proto__')).ma, 404);
});

test('dò mật khẩu bị chặn sau nhiều lần sai', async () => {
  const m = await dung();
  const ip = { 'cf-connecting-ip': '198.51.100.7' };
  for (let i = 0; i < 8; i++) assert.equal((await m.goi('/api/dang-nhap', { mat_khau: 'sai' + i }, ip)).ma, 401);
  assert.equal((await m.goi('/api/dang-nhap', { mat_khau: 'thu-tren-may' }, ip)).ma, 429, 'đúng mật khẩu cũng phải chờ');
  assert.equal((await m.goi('/api/dang-nhap', { mat_khau: 'thu-tren-may' }, { 'cf-connecting-ip': '198.51.100.8' })).ma, 200, 'IP khác không bị vạ lây');
});

test('địa chỉ IP qua Worker chuyển tiếp: chỉ tin khi có đúng mã bí mật', async () => {
  const m = await dung();
  m.env.CHUYEN_TIEP_BI_MAT = 'ma-chuyen-tiep';
  const qua = { 'x-hir-chuyen': 'ma-chuyen-tiep', 'x-hir-ip': '198.51.100.20', 'cf-connecting-ip': '2a06:98c0::1' };
  for (let i = 0; i < 8; i++) await m.goi('/api/dang-nhap', { mat_khau: 'sai' }, qua);
  assert.equal((await m.goi('/api/dang-nhap', { mat_khau: 'thu-tren-may' }, qua)).ma, 429);
  assert.equal((await m.goi('/api/dang-nhap', { mat_khau: 'thu-tren-may' }, { ...qua, 'x-hir-ip': '198.51.100.21' })).ma, 200, 'người khác qua cùng Worker không bị chặn lây');
  assert.equal((await m.goi('/api/dang-nhap', { mat_khau: 'thu-tren-may' }, { 'x-hir-chuyen': 'gia', 'x-hir-ip': '198.51.100.20', 'cf-connecting-ip': '198.51.100.30' })).ma, 200, 'mã sai thì x-hir-ip bị bỏ qua');
  const gia = { 'x-hir-chuyen': 'gia', 'x-hir-ip': '198.51.100.40', 'cf-connecting-ip': '198.51.100.20' };
  assert.equal((await m.goi('/api/dang-nhap', { mat_khau: 'thu-tren-may' }, gia)).ma, 429, 'không tự đổi IP để thoát chặn được');
});

test('web chỉ đụng tới key HLG-, không đụng key của tool khác', async () => {
  const m = await dung();
  const a = await m.dangNhap();
  m.HIR_KEY.the.set('HIR-XXXX-YYYY-ZZZZ', { key: 'HIR-XXXX-YYYY-ZZZZ', khoa: false, kich_hoat: [] });
  m.HIR_KEY.the.set('HRS-XXXX-YYYY-ZZZZ', { key: 'HRS-XXXX-YYYY-ZZZZ', khoa: false, kich_hoat: [] });
  const ds = (await m.goi('/api/qt/key', { viec: 'danhSach' }, a)).j.hang;
  assert.deepEqual(ds.map((k) => k.key), [KEY]);
  for (const viec of ['khoa', 'mo', 'goMay', 'xoa', 'sua']) {
    const r = await m.goi('/api/qt/key', { viec, key: 'HIR-XXXX-YYYY-ZZZZ', thay: { khoa: true } }, a);
    assert.equal(r.ma, 400, viec);
  }
  assert.ok(m.HIR_KEY.the.has('HIR-XXXX-YYYY-ZZZZ') && !m.HIR_KEY.the.get('HIR-XXXX-YYYY-ZZZZ').khoa);
});

test('tạo, sửa, xoá key; cột không được phép bị bỏ qua', async () => {
  const m = await dung();
  const a = await m.dangNhap();
  const r = await m.goi('/api/qt/key', { viec: 'tao', ghi_chu: 'Nhân sự mới', so_ngay: 30, so_may: 3, so_luong: 2 }, a);
  assert.equal(r.j.hang.length, 2);
  const k = r.j.hang[0];
  assert.match(k.key, /^HLG-[A-Z2-9]{4}-[A-Z2-9]{4}-[A-Z2-9]{4}$/);
  assert.equal(k.so_may, 3);
  assert.ok(Math.abs(Date.parse(k.het_han) - Date.now() - 30 * 86400000) < 60000);
  const vinh = await m.goi('/api/qt/key', { viec: 'tao', so_ngay: 0 }, a);
  assert.equal(vinh.j.hang[0].het_han, null);
  const s = await m.goi('/api/qt/key', { viec: 'sua', key: k.key, so_ngay: 0, thay: { ho_ten: 'An', key: 'HLG-HACK-HACK-HACK', tao_luc: 'x' } }, a);
  assert.equal(s.j.the.ho_ten, 'An');
  assert.equal(s.j.the.het_han, null);
  assert.equal(s.j.the.key, k.key, 'không đổi được mã key');
  assert.equal(s.j.the.tao_luc !== 'x', true);
  assert.equal((await m.goi('/api/qt/key', { viec: 'xoa', key: k.key }, a)).ma, 200);
  assert.equal(m.HIR_KEY.the.has(k.key), false);
  const ls = m.env.KHO().doc('lichSu', { ngay: 1 }).hang.map((n) => n.viec);
  for (const v of ['Tạo key', 'Sửa key', 'Xoá key']) assert.ok(ls.includes(v), v);
});

test('chữ độc hại từ máy gửi lên không chui vào mã trang: có CSP, và giao diện chỉ dùng textContent', async () => {
  const m = await dung();
  await m.baoCao({ ten_may: '<img src=x onerror=alert(1)>', dang_mo: [{ id: 'p', ten: '<script>alert(1)</script>', thu_muc: '', giay: 1 }] });
  const r = await m.goi('/');
  const csp = r.r.headers.get('content-security-policy');
  assert.ok(csp.includes("script-src 'self'") && !csp.includes('unsafe-inline'));
  assert.equal(r.r.headers.get('x-frame-options'), 'DENY');
  const inner = m.taiSan.app.match(/\.innerHTML\s*=/g) || [];
  assert.equal(inner.length, 1, 'chỉ một chỗ innerHTML: biểu tượng hằng số');
  assert.equal(m.env.KHO().doc('thietBi').hang[0].ten_may, '<img src=x onerror=alert(1)>', 'giữ nguyên chữ, không tự ý bỏ');
});

test('máy dùng key cũ HIR- vẫn thấy được, có giới hạn', async () => {
  const m = await dung();
  const b = { key: 'HIR-AAAA-BBBB-CCCC', ma_may: 'may-key-cu-may-01', ten_may: 'PC-CU', ten_nguoi: 'Nguyễn An' };
  const r = await m.baoCao(b);
  assert.equal(r.ma, 200);
  assert.equal(r.j.key_loi, null);
  assert.equal(m.HIR_KEY.goiCuoi.includes('layKey'), false, 'không hỏi kho key về key cũ');
  const may = m.env.KHO().doc('thietBi').hang[0];
  assert.equal(may.nhan_su, 'Nguyễn An');
  assert.equal(may.key, b.key);
  assert.equal((await m.baoCao({ ...b, ten_nguoi: '' })).ma, 200);
  assert.equal(m.env.KHO().doc('thietBi').hang[0].nhan_su, 'PC-CU', 'không khai tên thì lấy tên máy');
  for (let i = 1; i < 300; i++) m.sql("insert into may(ma_may, key, lan_dau, lan_cuoi) values (?, 'HIR-XXXX-XXXX-XXXX', 1, 1)", 'gia-' + i);
  assert.equal((await m.baoCao({ ...b, ma_may: 'may-key-cu-may-02' })).ma, 403, 'quá giới hạn thì máy mới bị từ chối');
  assert.equal((await m.baoCao(b)).ma, 200, 'máy đã có vẫn báo cáo được');
  assert.equal((await m.baoCao({})).ma, 200, 'key HLG- không bị ảnh hưởng');
});

test('số liệu biểu đồ: theo ngày, nền tảng, ô giờ, đỉnh', async () => {
  const m = await dung();
  const bay = Date.now();
  // Một phiên 3 giờ, kết thúc 1 giờ trước: tổng trong ma trận phải đúng 3 giờ dù cắt qua nhiều ô.
  const ket = bay - 3600000, bd = ket - 3 * 3600000;
  m.sql("insert into phien(ma_may, key, nhan_su, profile_id, ten, nen_tang, bat_dau, ket_thuc) values ('x', 'k', 'An', 'p', 'FB', 'facebook', ?, ?)", bd, ket);
  const r = m.env.KHO().doc('baoCaoSo', { ngay: 7 });
  assert.equal(r.theo_ngay.length, 7);
  assert.equal(r.theo_ngay.reduce((a, d) => a + d.ms, 0), 3 * 3600000, 'tổng theo ngày');
  assert.equal(r.theo_gio.o.length, 7);
  assert.ok(r.theo_gio.o.every((h) => h.length === 24));
  assert.equal(r.theo_gio.o.flat().reduce((a, b) => a + b, 0), 3 * 3600000, 'tổng các ô giờ');
  assert.equal(r.theo_nen[0].nen_tang, 'facebook');
  assert.equal(r.theo_nen[0].ms, 3 * 3600000);
  const q = m.env.KHO().doc('tongQuan');
  assert.equal(q.xu_huong.length, 14);
  assert.equal(q.xu_huong.reduce((a, d) => a + d.ms, 0), 3 * 3600000);
  // Máy và đỉnh trong ngày.
  await m.baoCao({ dang_mo: [{ id: 'a', ten: 'A', thu_muc: '', giay: 1 }, { id: 'b', ten: 'B', thu_muc: '', giay: 1 }] });
  donDep(m.sql, Date.now());
  await m.baoCao({ dang_mo: [{ id: 'a', ten: 'A', thu_muc: '', giay: 1 }] });
  donDep(m.sql, Date.now());
  const dinh = m.env.KHO().doc('baoCaoSo', { ngay: 7 }).dinh;
  assert.equal(dinh.length, 1);
  assert.equal(dinh[0].dang_mo, 2, 'giữ số cao nhất trong ngày, không bị số sau đè xuống');
  assert.equal(dinh[0].may_bat, 1);
  assert.deepEqual(m.env.KHO().doc('tongQuan').may_tt, { dang_mo: 1, bat: 1, tat: 0 });
});

test('khoảng ngày: tính theo giờ Việt Nam, sai dạng hay quá hạn đều về chỗ hợp lý', () => {
  const bay = Date.UTC(2026, 9, 30, 5); // 12:00 ngày 30/10 giờ Việt Nam
  const vn = (y, m, d) => Date.UTC(y, m - 1, d) - 7 * 3600000;
  assert.deepEqual(khoang({ tu: '2026-10-26', den: '2026-10-28' }, bay), { tu: vn(2026, 10, 26), den: vn(2026, 10, 29), so_ngay: 3 });
  assert.deepEqual(khoang({ tu: '2026-10-30', den: '2026-10-30' }, bay), { tu: vn(2026, 10, 30), den: vn(2026, 10, 31), so_ngay: 1 });
  assert.equal(khoang({}, bay).so_ngay, 7, 'không chọn gì: 7 ngày');
  assert.equal(khoang({ tu: 'abc', den: '2026-10-28' }, bay).so_ngay, 7, 'sai dạng: 7 ngày');
  assert.equal(khoang({ tu: '2026-10-28', den: '2026-10-26' }, bay).so_ngay, 7, 'ngược: 7 ngày');
  assert.equal(khoang({ tu: '2026-10-20', den: '2030-01-01' }, bay).den, vn(2026, 10, 31), 'không vượt hôm nay');
  assert.deepEqual(khoang({ tu: '2030-01-01', den: '2030-01-02' }, bay), { tu: vn(2026, 10, 30), den: vn(2026, 10, 31), so_ngay: 1 }, 'toàn tương lai: chỉ hôm nay');
  assert.equal(khoang({ tu: '2020-01-01', den: '2026-10-30' }, bay).so_ngay, 366, 'tối đa một năm');
  assert.equal(khoang({ ngay: 30 }, bay).so_ngay, 30);
});

test('báo cáo và lịch sử chỉ tính đúng trong khoảng đã chọn', async () => {
  const m = await dung();
  const vn = (d, g, p) => Date.UTC(2026, 9, d, g, p || 0) - 7 * 3600000;
  const phien = (id, nen, bd, kt) => m.sql("insert into phien(ma_may, key, nhan_su, profile_id, ten, nen_tang, bat_dau, ket_thuc) values ('x', 'k', 'An', ?, ?, ?, ?, ?)", id, id, nen, bd, kt);
  phien('trong', 'facebook', vn(26, 9, 30), vn(26, 11)); // thứ 2, 9:30-11:00
  phien('vat-qua', 'shopee', vn(25, 23), vn(26, 1)); // chủ nhật 23:00 sang thứ 2 01:00: chỉ tính 1 giờ
  phien('ngoai', 'tiktok', vn(20, 9), vn(20, 10));
  const r = m.env.KHO().doc('baoCaoSo', { tu: '2026-10-26', den: '2026-10-26' });
  assert.equal(r.so_ngay, 1);
  assert.equal(r.theo_ngay.length, 1);
  assert.equal(r.theo_ngay[0].ms, 2.5 * 3600000, 'cắt phần trước nửa đêm');
  assert.equal(r.so_profile, 2);
  assert.deepEqual(r.theo_nen.map((x) => x.nen_tang).sort(), ['facebook', 'shopee'], 'phiên ngoài khoảng không tính');
  assert.equal(r.theo_nen.find((x) => x.nen_tang === 'shopee').ms, 3600000);
  assert.equal(r.theo_nguoi[0].ms, 2.5 * 3600000);
  assert.equal(r.theo_gio.o[0][9], 30 * 60000, 'thứ 2 lúc 9 giờ: 30 phút');
  assert.equal(r.theo_gio.o[0][10], 3600000, 'thứ 2 lúc 10 giờ');
  assert.equal(r.theo_gio.o[0][0], 3600000, 'thứ 2 lúc 0 giờ');
  assert.equal(r.theo_gio.o[6][23], 0, 'chủ nhật nằm ngoài khoảng');
  assert.equal(r.theo_gio.o.flat().reduce((a, b) => a + b, 0), 2.5 * 3600000);
  const ca = m.env.KHO().doc('baoCaoSo', { tu: '2026-10-20', den: '2026-10-26' });
  assert.equal(ca.theo_gio.o[6][23], 3600000, 'mở rộng khoảng thì có chủ nhật');
  // Ngày trong tương lai không tính thêm thời gian của phiên đang mở.
  const luc = (d, g) => m.sql("insert into nhat_ky(luc, viec) values (?, 'Mở profile')", vn(d, g));
  luc(26, 0); luc(26, 23); luc(27, 0); luc(25, 23);
  const ls = m.env.KHO().doc('lichSu', { tu: '2026-10-26', den: '2026-10-26' });
  assert.equal(ls.tong, 2, 'gồm cả đầu ngày và cuối ngày, không lấn sang ngày bên cạnh');
});

test('đoán nền tảng', () => {
  assert.equal(nenTang('GOOGLE-ADS-001', 'Marketing'), 'google');
  assert.equal(nenTang('FB-ADS-1', ''), 'facebook');
  assert.equal(nenTang('abc', 'Shopee VN'), 'shopee');
  assert.equal(nenTang('Tik Tok 1', ''), 'tiktok');
  assert.equal(nenTang('acc 1', ''), 'khac');
});
