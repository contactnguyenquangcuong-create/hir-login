// Lõi của web quản trị Hir-Login: nhận báo cáo từ mọi máy, lưu SQLite, trả số liệu cho giao diện.
// Chạy trong Durable Object của Worker `hirlogin-web`, và chạy y hệt trên máy (node:sqlite) để test.
// `sql(câu, ...tham số)` luôn trả về MẢNG hàng. Mọi giờ đều là mili-giây UTC.
//
// Chỉ để quản lý, không đồng bộ gì: máy gửi số lượng profile, profile nào đang mở (tên, folder, đã mở bao lâu),
// automation nào đang chạy, phiên bản, máy chủ team đang nối, và số lượt vào từng website (CHỈ tên miền). KHÔNG có đường dẫn,
// từ khoá tìm kiếm, nội dung trang, danh sách profile khác, cookie, mật khẩu hay thông tin proxy.

export const SCHEMA = [
  // Bản đầu có lưu danh sách profile của mỗi máy; không dùng nữa.
  'drop table if exists profile',
  `create table if not exists may (
     ma_may text primary key, key text not null, nhan_su text not null default '', ten_may text not null default '',
     phien_ban text not null default '', he_dieu_hanh text not null default '', ip text not null default '', may_chu text not null default '',
     lan_dau integer not null, lan_cuoi integer not null, tong_profile integer not null default 0, cho_tai_len integer not null default 0,
     tu_dong text not null default '[]', key_loi text, truc_tuyen integer not null default 1)`,
  `create table if not exists dang_mo (
     ma_may text not null, id text not null, ten text not null default '', thu_muc text not null default '', nen_tang text not null default 'khac',
     tu_luc integer not null, phien integer not null, primary key (ma_may, id))`,
  `create table if not exists phien (
     id integer primary key autoincrement, ma_may text not null, key text not null, nhan_su text not null default '', ten_may text not null default '',
     profile_id text not null, ten text not null default '', thu_muc text not null default '', nen_tang text not null default 'khac',
     bat_dau integer not null, ket_thuc integer)`,
  `create index if not exists phien_bd on phien(bat_dau)`,
  `create index if not exists phien_pid on phien(profile_id)`,
  `create index if not exists phien_may on phien(ma_may, profile_id, bat_dau)`,
  // Máy từ Hir-Login 0.2.44: báo cáo theo đợt 9h và 17h, tự ghi giờ mở và đóng profile.
  `create table if not exists may_dot (ma_may text primary key, dot integer not null, lan_gui integer not null, gui_bu integer not null default 0)`,
  `create table if not exists lan_chay (ma_may text not null, du_an text not null, bat_dau integer not null, ket_thuc integer, primary key (ma_may, du_an, bat_dau))`,
  `create table if not exists so_lieu_ngay (ngay integer primary key, tong_profile integer not null)`,
  `create table if not exists dinh_ngay (ngay integer primary key, may_bat integer not null, dang_mo integer not null)`,
  `create table if not exists nhat_ky (
     id integer primary key autoincrement, luc integer not null, viec text not null, nhan_su text not null default '',
     ten_may text not null default '', ma_may text not null default '', doi_tuong text not null default '', chi_tiet text not null default '')`,
  `create index if not exists nhat_ky_luc on nhat_ky(luc)`,
  `create table if not exists xac_thuc (
     key text not null, ma_may text not null, ok integer not null, nhan_su text not null default '', key_loi text, luc integer not null,
     primary key (key, ma_may))`,
  `create table if not exists trang_web (ngay integer not null, ten_mien text not null, ma_may text not null, key text not null, nhan_su text not null default '', ten_may text not null default '', luot integer not null, primary key (ngay, ten_mien, ma_may))`,
  `create index if not exists trang_web_ngay on trang_web(ngay)`,
  `create table if not exists dang_nhap_sai (ip text primary key, dem integer not null, tu_luc integer not null)`,
  `create table if not exists cau_hinh (k text primary key, v text not null)`,
];

export const MAC_DINH = { sau_giay: 180, giu_ngay: 90, phien_ban_moi: '' };
const XAC_THUC_GIAY = 30 * 60;
const KEY_CU_TOI_DA = 300;
const SAI_TOI_DA = 8;
const SAI_KHOA_MS = 15 * 60 * 1000;
const NGAY = 86400000;

export function dung(sql) {
  for (const s of SCHEMA) sql(s);
}

export const cauHinh = (sql) => {
  const c = { ...MAC_DINH };
  for (const r of sql('select k, v from cau_hinh')) c[r.k] = r.k === 'phien_ban_moi' ? r.v : Number(r.v);
  return c;
};

const chu = (v, n) => String(v == null ? '' : v).slice(0, n);

// Nền tảng đoán từ tên và folder: Hir-Login không lưu "nền tảng" của profile.
export function nenTang(ten, thuMuc) {
  const s = `${ten} ${thuMuc}`.toLowerCase();
  if (/shopee|\bshp\b|\bspe\b/.test(s)) return 'shopee';
  if (/facebook|\bfb\b|fanpage/.test(s)) return 'facebook';
  if (/tiktok|tik[ -]?tok|\btt\b/.test(s)) return 'tiktok';
  if (/google|gmail|youtube|\byt\b/.test(s)) return 'google';
  return 'khac';
}

// Từ thẻ key của máy chủ key: tên người dùng và tình trạng. Key Hir-Login cũ: "Tên · SĐT · máy" nằm trong tên máy.
export function docThongTinKey(k, maMay) {
  if (!k || !/^HLG-/.test(k.key || '')) return { ok: false, nhan_su: '', key_loi: 'KHONG_CO' };
  const dong = (k.kich_hoat || []).find((m) => m.ma_may === maMay);
  if (!dong) return { ok: false, nhan_su: '', key_loi: 'CHUA_KICH_HOAT' };
  const tuMay = String(dong.ten_may || '').includes(' · ') ? String(dong.ten_may).split(' · ')[0].trim() : '';
  const nhan_su = String(k.ho_ten || '').trim() || tuMay || String(k.ghi_chu || '').trim() || k.key;
  let key_loi = null;
  if (k.khoa) key_loi = 'BI_KHOA';
  else if (k.het_han && Date.parse(k.het_han) < Date.now()) key_loi = 'HET_HAN';
  return { ok: true, nhan_su, key_loi };
}

// Xác thực máy: cặp key + mã máy phải có thật trên máy chủ key. Nhớ 30 phút, để không hỏi mỗi lần báo cáo.
export async function xacThuc(sql, key, maMay, hoiKey, bay) {
  const nho = sql('select * from xac_thuc where key = ? and ma_may = ?', key, maMay)[0];
  if (nho && bay - nho.luc < XAC_THUC_GIAY * 1000) return { ok: !!nho.ok, nhan_su: nho.nhan_su, key_loi: nho.key_loi };
  let the;
  try {
    the = await hoiKey(key);
  } catch (_) {
    // Máy chủ key đang lỗi: một máy đã biết thì vẫn nhận, máy lạ thì chưa.
    if (nho) return { ok: !!nho.ok, nhan_su: nho.nhan_su, key_loi: nho.key_loi };
    return { ok: false, tam_loi: true };
  }
  const v = docThongTinKey(the, maMay);
  sql('insert or replace into xac_thuc(key, ma_may, ok, nhan_su, key_loi, luc) values (?,?,?,?,?,?)', key, maMay, v.ok ? 1 : 0, v.nhan_su, v.key_loi, bay);
  return v;
}

const ghiNhatKy = (sql, luc, viec, nhanSu, tenMay, maMay, doiTuong, chiTiet) =>
  sql('insert into nhat_ky(luc, viec, nhan_su, ten_may, ma_may, doi_tuong, chi_tiet) values (?,?,?,?,?,?,?)', luc, viec, nhanSu || '', tenMay || '', maMay || '', chu(doiTuong, 200), chu(chiTiet, 300));

function dongPhien(sql, m, r, luc, viec) {
  sql('update phien set ket_thuc = ? where id = ? and ket_thuc is null', luc, r.phien);
  sql('delete from dang_mo where ma_may = ? and id = ?', m.ma_may, r.id);
  ghiNhatKy(sql, luc, viec, m.nhan_su, m.ten_may, m.ma_may, r.ten, r.thu_muc);
}

// Lượt vào website của một máy. Chỉ nhận đúng dạng tên miền: có đường dẫn hay thứ lạ thì bỏ, để dữ liệu chi tiết không lọt vào đây.
// Mỗi số là TỔNG của ngày đó trên máy đó, nên gửi lại bao nhiêu lần cũng không đếm trùng.
function ghiTrangWeb(sql, ds, m, bay) {
  if (!Array.isArray(ds)) return;
  const homNay = dauNgay(bay), xaNhat = homNay - cauHinh(sql).giu_ngay * NGAY;
  for (const r of ds.slice(0, 6000)) {
    if (!r || typeof r.ten_mien !== 'string') continue;
    const ten = r.ten_mien.toLowerCase();
    if (ten.length > 120 || !/[.:]/.test(ten) || !/^[a-z0-9:]([a-z0-9.:-]*[a-z0-9])?$/.test(ten)) continue;
    const luot = Math.floor(Number(r.luot));
    if (!(luot >= 1 && luot <= 100000)) continue;
    let ngay = ngayVN(r.ngay);
    if (ngay != null && ngay < xaNhat) continue; // gửi bù sau rất lâu: đã quá hạn giữ
    if (ngay == null || ngay > homNay) ngay = homNay; // máy lệch giờ
    sql(`insert into trang_web(ngay, ten_mien, ma_may, key, nhan_su, ten_may, luot) values (?,?,?,?,?,?,?)
         on conflict(ngay, ten_mien, ma_may) do update set luot = max(luot, excluded.luot), nhan_su = excluded.nhan_su, ten_may = excluded.ten_may`, ngay, ten, m.ma_may, m.key, m.nhan_su, m.ten_may, luot);
  }
}

// Đợt báo cáo gần nhất (9h và 17h giờ Việt Nam) tính tới một thời điểm.
export function dotGanNhat(ms) {
  const dau = dauNgay(ms);
  for (const gio of [17, 9]) if (ms >= dau + gio * 3600000) return dau + gio * 3600000;
  return dau - 7 * 3600000; // 17h hôm trước
}

const thoiDiem = (v, bay) => { const n = Math.floor(Number(v)); return n > 0 && n <= bay + 3600000 ? Math.min(n, bay) : null; };

// Phiên mở profile do máy tự ghi giờ thật, kể cả khi tới muộn sau một lần mất điện. Gửi lại không tạo trùng: nhận biết theo máy,
// profile và giờ mở. Danh sách đang mở dựng lại từ báo cáo này.
function ghiPhien(sql, ds, m, bay) {
  sql('delete from dang_mo where ma_may = ?', m.ma_may);
  for (const p of ds.slice(0, 2000)) {
    const id = chu(p && p.id, 128), bd = thoiDiem(p && p.bat_dau, bay);
    if (!id || bd == null) continue;
    const kt = p.ket_thuc == null ? null : thoiDiem(p.ket_thuc, bay);
    if (p.ket_thuc != null && (kt == null || kt < bd)) continue;
    const ten = chu(p.ten, 200), thu_muc = chu(p.thu_muc, 200), nt = nenTang(ten, thu_muc);
    const cu = sql('select id, ket_thuc from phien where ma_may = ? and profile_id = ? and bat_dau = ? limit 1', m.ma_may, id, bd)[0];
    const ph = cu ? cu.id : sql('insert into phien(ma_may, key, nhan_su, ten_may, profile_id, ten, thu_muc, nen_tang, bat_dau) values (?,?,?,?,?,?,?,?,?) returning id', m.ma_may, m.key, m.nhan_su, m.ten_may, id, ten, thu_muc, nt, bd)[0].id;
    if (!cu) ghiNhatKy(sql, bd, 'Mở profile', m.nhan_su, m.ten_may, m.ma_may, ten, thu_muc);
    if (kt != null && (!cu || cu.ket_thuc == null)) {
      sql('update phien set ket_thuc = ? where id = ?', kt, ph);
      ghiNhatKy(sql, kt, p.dot_ngot ? 'Đóng profile (máy hoặc app tắt)' : 'Đóng profile', m.nhan_su, m.ten_may, m.ma_may, ten, thu_muc);
    }
    if (kt == null) sql('insert or replace into dang_mo(ma_may, id, ten, thu_muc, nen_tang, tu_luc, phien) values (?,?,?,?,?,?,?)', m.ma_may, id, ten, thu_muc, nt, bd, ph);
  }
}

function ghiLanChay(sql, ds, m, bay) {
  for (const a of ds.slice(0, 500)) {
    const du_an = chu(a && a.du_an, 120), bd = thoiDiem(a && a.bat_dau, bay);
    if (!du_an || bd == null) continue;
    const kt = a.ket_thuc == null ? null : thoiDiem(a.ket_thuc, bay);
    const cu = sql('select ket_thuc from lan_chay where ma_may = ? and du_an = ? and bat_dau = ?', m.ma_may, du_an, bd)[0];
    if (!cu) {
      sql('insert into lan_chay(ma_may, du_an, bat_dau, ket_thuc) values (?,?,?,?)', m.ma_may, du_an, bd, kt);
      ghiNhatKy(sql, bd, 'Chạy automation', m.nhan_su, m.ten_may, m.ma_may, du_an, '');
    }
    if (kt != null && (!cu || cu.ket_thuc == null)) {
      if (cu) sql('update lan_chay set ket_thuc = ? where ma_may = ? and du_an = ? and bat_dau = ?', kt, m.ma_may, du_an, bd);
      ghiNhatKy(sql, kt, 'Kết thúc automation', m.nhan_su, m.ten_may, m.ma_may, du_an, `${Number(a.dong_ok) || 0} dòng xong, ${Number(a.dong_loi) || 0} dòng lỗi`);
    }
  }
}

// Một báo cáo từ máy. Trả { ok, sau_giay, key_loi }. Có "phien" là máy bản 0.2.44 trở lên (gửi theo đợt, tự ghi giờ);
// không có là bản 0.2.43 (gửi mỗi 3 phút, server tự suy ra phiên mở và đóng).
export async function baoCao(sql, b, ip, hoiKey, bay) {
  const key = chu(b.key, 40).trim().toUpperCase();
  const ma_may = chu(b.ma_may, 128);
  if (!/^(HLG|HIR)-[A-Z0-9]{4}-[A-Z0-9]{4}-[A-Z0-9]{4}$/.test(key) || ma_may.length < 16) return { ok: false, ma: 400, loi: 'SAI_DU_LIEU' };
  // Key cũ (trước Hir-Login 0.2.2) không có trong kho key nên không xác thực được: vẫn nhận để thấy đủ máy,
  // giao diện gắn nhãn "Key cũ", và giới hạn số máy để không ai bơm rác được vô hạn.
  const keyCu = key.startsWith('HIR-');
  if (keyCu && !sql('select 1 from may where ma_may = ?', ma_may)[0] && sql("select count(*) n from may where key like 'HIR-%'")[0].n >= KEY_CU_TOI_DA) return { ok: false, ma: 403, loi: 'QUA_NHIEU_KEY_CU' };
  const v = keyCu ? { ok: true, nhan_su: chu(b.ten_nguoi, 80).trim() || chu(b.ten_may, 80), key_loi: null } : await xacThuc(sql, key, ma_may, hoiKey, bay);
  if (v.tam_loi) return { ok: false, ma: 503, loi: 'TAM_LOI' };
  if (!v.ok) return { ok: false, ma: 403, loi: 'KHONG_XAC_THUC', key_loi: v.key_loi };

  const c = cauHinh(sql);
  const cu = sql('select * from may where ma_may = ?', ma_may)[0];
  const theoDot = Array.isArray(b.phien);
  const tu_dong = (Array.isArray(b.tu_dong) ? b.tu_dong : []).filter((a) => a && a.ket_thuc == null).slice(0, 50).map((a) => ({
    du_an: chu(a.du_an, 120), luong: Number(a.luong) || 0, dong_ok: Number(a.dong_ok) || 0, dong_loi: Number(a.dong_loi) || 0, tu_luc: Number(a.tu_luc || a.bat_dau) || 0,
  }));
  const m = {
    ma_may, key, nhan_su: v.nhan_su, ten_may: chu(b.ten_may, 80), phien_ban: chu(b.phien_ban, 20), he_dieu_hanh: chu(b.he_dieu_hanh, 20),
    ip: chu(ip, 64), may_chu: chu(b.may_chu, 120), tong_profile: Math.max(0, Number(b.tong_profile) || 0), cho_tai_len: Math.max(0, Number(b.cho_tai_len) || 0),
  };

  if (!cu) {
    sql(`insert into may(ma_may, key, nhan_su, ten_may, phien_ban, he_dieu_hanh, ip, may_chu, lan_dau, lan_cuoi, tong_profile, cho_tai_len, tu_dong, key_loi, truc_tuyen)
         values (?,?,?,?,?,?,?,?,?,?,?,?,?,?,1)`, ma_may, key, m.nhan_su, m.ten_may, m.phien_ban, m.he_dieu_hanh, m.ip, m.may_chu, bay, bay, m.tong_profile, m.cho_tai_len, JSON.stringify(tu_dong), v.key_loi);
    ghiNhatKy(sql, bay, 'Máy mới', m.nhan_su, m.ten_may, ma_may, m.phien_ban, '');
  } else {
    if (!cu.truc_tuyen && !theoDot) ghiNhatKy(sql, bay, 'Bật phần mềm', m.nhan_su, m.ten_may, ma_may, m.phien_ban, '');
    if (cu.phien_ban && cu.phien_ban !== m.phien_ban) ghiNhatKy(sql, bay, 'Cập nhật phần mềm', m.nhan_su, m.ten_may, ma_may, `${cu.phien_ban} → ${m.phien_ban}`, '');
    sql(`update may set key=?, nhan_su=?, ten_may=?, phien_ban=?, he_dieu_hanh=?, ip=?, may_chu=?, lan_cuoi=?, tong_profile=?, cho_tai_len=?, tu_dong=?, key_loi=?, truc_tuyen=1 where ma_may=?`,
      key, m.nhan_su, m.ten_may, m.phien_ban, m.he_dieu_hanh, m.ip, m.may_chu, bay, m.tong_profile, m.cho_tai_len, JSON.stringify(tu_dong), v.key_loi, ma_may);
    if (!theoDot) {
      const truoc = new Set(JSON.parse(cu.tu_dong || '[]').map((a) => a.du_an));
      const nay = new Set(tu_dong.map((a) => a.du_an));
      for (const a of tu_dong) if (!truoc.has(a.du_an)) ghiNhatKy(sql, bay, 'Chạy automation', m.nhan_su, m.ten_may, ma_may, a.du_an, '');
      for (const t of truoc) if (!nay.has(t)) ghiNhatKy(sql, bay, 'Kết thúc automation', m.nhan_su, m.ten_may, ma_may, t, '');
    }
  }

  if (theoDot) {
    ghiPhien(sql, b.phien, m, bay);
    ghiLanChay(sql, Array.isArray(b.tu_dong) ? b.tu_dong : [], m, bay);
    ghiTrangWeb(sql, b.trang_web, m, bay);
    const dot = thoiDiem(b.dot, bay) || bay;
    const daCo = sql('select dot from may_dot where ma_may = ?', ma_may)[0];
    // Một đợt có thể gửi nhiều phần: chỉ ghi lịch sử một lần.
    if (b.gui_bu && (!daCo || daCo.dot !== dot)) ghiNhatKy(sql, bay, 'Gửi bù báo cáo', m.nhan_su, m.ten_may, ma_may, 'Đợt ' + new Date(dot + 7 * 3600000).toISOString().slice(11, 13) + 'h ngày ' + new Date(dot + 7 * 3600000).toISOString().slice(8, 10) + '/' + new Date(dot + 7 * 3600000).toISOString().slice(5, 7), '');
    sql('insert or replace into may_dot(ma_may, dot, lan_gui, gui_bu) values (?,?,?,?)', ma_may, dot, bay, b.gui_bu ? 1 : 0);
    return { ok: true, key_loi: v.key_loi };
  }

  // Profile đang mở: so với lần trước để ra phiên mở / đóng.
  const moBay = new Map();
  for (const p of (Array.isArray(b.dang_mo) ? b.dang_mo : []).slice(0, 500)) {
    const id = chu(p.id, 128);
    if (id) moBay.set(id, { id, ten: chu(p.ten, 200), thu_muc: chu(p.thu_muc, 200), giay: Math.max(0, Number(p.giay) || 0) });
  }
  const moCu = sql('select * from dang_mo where ma_may = ?', ma_may);
  for (const r of moCu) if (!moBay.has(r.id)) dongPhien(sql, m, r, bay, 'Đóng profile');
  const daMo = new Set(moCu.map((r) => r.id));
  for (const p of moBay.values()) {
    if (daMo.has(p.id)) continue;
    const tu = Math.min(bay, bay - p.giay * 1000);
    const nt = nenTang(p.ten, p.thu_muc);
    const ph = sql('insert into phien(ma_may, key, nhan_su, ten_may, profile_id, ten, thu_muc, nen_tang, bat_dau) values (?,?,?,?,?,?,?,?,?) returning id', ma_may, key, m.nhan_su, m.ten_may, p.id, p.ten, p.thu_muc, nt, tu)[0].id;
    sql('insert into dang_mo(ma_may, id, ten, thu_muc, nen_tang, tu_luc, phien) values (?,?,?,?,?,?,?)', ma_may, p.id, p.ten, p.thu_muc, nt, tu, ph);
    ghiNhatKy(sql, tu, 'Mở profile', m.nhan_su, m.ten_may, ma_may, p.ten, p.thu_muc);
  }

  ghiTrangWeb(sql, b.trang_web, m, bay);
  return { ok: true, sau_giay: c.sau_giay, key_loi: v.key_loi };
}

// Xoá dữ liệu quá hạn và cập nhật tình trạng máy. Máy theo đợt: "đã báo cáo" nghĩa là có số liệu từ đợt gần nhất (cho 15 phút để
// các máy kịp gửi); đóng nó chỉ khi im quá lâu, vì lần báo cáo sau sẽ tự mang giờ đóng thật về.
// Máy bản 0.2.43: im quá 3 nhịp là mất kết nối, đóng các phiên của nó tại lần báo cáo cuối.
export function donDep(sql, bay) {
  const c = cauHinh(sql);
  const dot = dotGanNhat(bay - 15 * 60000);
  sql('update may set truc_tuyen = 0 where truc_tuyen = 1 and lan_cuoi < ? and ma_may in (select ma_may from may_dot)', dot);
  for (const m of sql('select m.* from may m where m.lan_cuoi < ? and m.ma_may in (select ma_may from dang_mo) and m.ma_may in (select ma_may from may_dot)', bay - 3 * NGAY)) {
    for (const r of sql('select * from dang_mo where ma_may = ?', m.ma_may)) dongPhien(sql, m, r, m.lan_cuoi, 'Đóng profile (máy không báo cáo)');
  }
  const nguong = bay - Math.max(c.sau_giay * 3, 300) * 1000;
  for (const m of sql('select * from may where truc_tuyen = 1 and lan_cuoi < ? and ma_may not in (select ma_may from may_dot)', nguong)) {
    for (const r of sql('select * from dang_mo where ma_may = ?', m.ma_may)) dongPhien(sql, m, r, m.lan_cuoi, 'Đóng profile (mất kết nối)');
    sql('update may set truc_tuyen = 0 where ma_may = ?', m.ma_may);
    ghiNhatKy(sql, m.lan_cuoi, 'Tắt phần mềm / mất kết nối', m.nhan_su, m.ten_may, m.ma_may, '', '');
  }
  // Số tổng profile mỗi ngày, để so với tháng trước (máy chỉ báo số hiện tại).
  sql('insert or replace into so_lieu_ngay(ngay, tong_profile) values (?, (select coalesce(sum(tong_profile), 0) from may))', dauNgay(bay));
  const han = bay - c.giu_ngay * NGAY;
  sql('delete from phien where bat_dau < ? and ket_thuc is not null', han);
  sql('delete from nhat_ky where luc < ?', han);
  sql('delete from so_lieu_ngay where ngay < ?', bay - 400 * NGAY);
  sql('delete from dinh_ngay');
  sql('delete from lan_chay where bat_dau < ?', han);
  sql('delete from trang_web where ngay < ?', han);
  sql('delete from dang_nhap_sai where tu_luc < ?', bay - SAI_KHOA_MS);
}

// ---- Đăng nhập: chặn dò mật khẩu theo địa chỉ IP ----
export function biChan(sql, ip, bay) {
  const r = sql('select * from dang_nhap_sai where ip = ?', ip)[0];
  return !!r && r.dem >= SAI_TOI_DA && bay - r.tu_luc < SAI_KHOA_MS;
}
export function ghiSai(sql, ip, bay) {
  const r = sql('select * from dang_nhap_sai where ip = ?', ip)[0];
  if (!r || bay - r.tu_luc >= SAI_KHOA_MS) sql('insert or replace into dang_nhap_sai(ip, dem, tu_luc) values (?,1,?)', ip, bay);
  else sql('update dang_nhap_sai set dem = dem + 1 where ip = ?', ip);
}
export function xoaSai(sql, ip) {
  sql('delete from dang_nhap_sai where ip = ?', ip);
}

// ---- Số liệu cho giao diện ----
const dauThang = (bay, lui) => {
  const d = new Date(bay + 7 * 3600000);
  return Date.UTC(d.getUTCFullYear(), d.getUTCMonth() - lui, 1) - 7 * 3600000;
};
const dauNgay = (bay) => {
  const d = new Date(bay + 7 * 3600000);
  return Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate()) - 7 * 3600000;
};
const soSanhPhienBan = (a, b) => {
  const x = String(a).split('.').map(Number), y = String(b).split('.').map(Number);
  for (let i = 0; i < 3; i++) if ((x[i] || 0) !== (y[i] || 0)) return (x[i] || 0) - (y[i] || 0);
  return 0;
};
const mauMay = (m) => ({ ...m, tu_dong: JSON.parse(m.tu_dong || '[]') });

// Phiên bản coi là mới nhất: đặt trong Cài đặt, không đặt thì lấy bản cao nhất đang có máy dùng.
function banMoi(sql) {
  const dat = cauHinh(sql).phien_ban_moi;
  if (dat) return dat;
  let cao = '';
  for (const r of sql('select distinct phien_ban from may')) if (r.phien_ban && (!cao || soSanhPhienBan(r.phien_ban, cao) > 0)) cao = r.phien_ban;
  return cao;
}

// ---- Số liệu cho biểu đồ ----
const MS_PHIEN = 'sum(min(coalesce(ket_thuc, ?), ?) - max(bat_dau, ?))';
const TRONG_KHOANG = 'bat_dau < ? and coalesce(ket_thuc, ?) > ?';
const VN_MS = 7 * 3600000;
const KHOANG_TOI_DA = 366;

const ngayVN = (s) => {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(String(s || ''));
  return m ? Date.UTC(+m[1], +m[2] - 1, +m[3]) - VN_MS : null;
};

// Khoảng ngày (giờ Việt Nam) trình duyệt gửi lên: tu, den dạng YYYY-MM-DD, tính cả ngày den. Trả { tu, den, so_ngay },
// den là mốc NGAY SAU ngày cuối. Sai dạng thì lấy 7 ngày gần nhất; không vượt quá hôm nay và quá một năm.
export function khoang(a, bay) {
  const homNay = dauNgay(bay);
  let tu = ngayVN(a.tu), den = ngayVN(a.den);
  if (tu == null || den == null || den < tu) {
    den = homNay;
    tu = homNay - (Math.min(Math.max(1, Number(a.ngay) || 7), KHOANG_TOI_DA) - 1) * NGAY;
  }
  den = Math.min(den + NGAY, homNay + NGAY);
  tu = Math.max(tu, den - KHOANG_TOI_DA * NGAY);
  if (tu >= den) tu = den - NGAY;
  return { tu, den, so_ngay: Math.round((den - tu) / NGAY) };
}

function theoNgay(sql, tu, den, bay) {
  const ds = [];
  for (let d = tu; d < den; d += NGAY) {
    const c = Math.min(d + NGAY, bay);
    const r = sql(`select count(distinct key) nguoi, count(distinct profile_id) profile, ${MS_PHIEN} ms from phien where bat_dau < ? and coalesce(ket_thuc, ?) > ?`, c, c, d, c, bay, d)[0];
    ds.push({ ngay: d, nguoi: r.nguoi, profile: r.profile, ms: r.ms || 0, luot: sql('select count(*) n from phien where bat_dau >= ? and bat_dau < ?', d, d + NGAY)[0].n });
  }
  return ds;
}

const theoNen = (sql, tu, den, bay) => sql(`select nen_tang, count(*) luot, ${MS_PHIEN} ms from phien where ${TRONG_KHOANG} group by nen_tang`, bay, Math.min(den, bay), tu, den, bay, tu).map((r) => ({ ...r, ms: r.ms || 0 }));

// Mỗi ngày: số máy có dùng, và số profile mở cùng lúc nhiều nhất. Tính từ giờ mở và đóng thật, không phụ thuộc lúc máy báo cáo.
function dinhTheoNgay(sql, tu, den, bay) {
  const cuoi = Math.min(den, bay);
  const ngay = new Map();
  for (const r of sql(`select ma_may, bat_dau, coalesce(ket_thuc, ?) kt from phien where ${TRONG_KHOANG} limit 50000`, bay, den, bay, tu)) {
    const a = Math.max(r.bat_dau, tu), b = Math.min(r.kt, cuoi);
    for (let d = tu + Math.floor((a - tu) / NGAY) * NGAY; d <= b && d < den; d += NGAY) {
      if (!ngay.has(d)) ngay.set(d, { may: new Set(), moc: [] });
      const o = ngay.get(d);
      o.may.add(r.ma_may);
      o.moc.push([Math.max(a, d), 1], [Math.min(b, d + NGAY), -1]);
    }
  }
  return [...ngay.keys()].sort((x, y) => x - y).map((d) => {
    const o = ngay.get(d);
    o.moc.sort((x, y) => x[0] - y[0] || x[1] - y[1]);
    let dang = 0, cao = 0;
    for (const [, buoc] of o.moc) { dang += buoc; if (dang > cao) cao = dang; }
    return { ngay: d, may_bat: o.may.size, dang_mo: cao };
  });
}

// Ma trận thứ × giờ (giờ Việt Nam, hàng 0 là thứ 2): mili-giây dùng profile, cộng dồn trong khoảng đã chọn.
function oTheoGio(sql, tu, den, bay) {
  const o = Array.from({ length: 7 }, () => new Array(24).fill(0));
  const cuoi = Math.min(den, bay);
  for (const r of sql(`select bat_dau, coalesce(ket_thuc, ?) kt from phien where ${TRONG_KHOANG} limit 50000`, bay, den, bay, tu)) {
    let t = Math.max(r.bat_dau, tu);
    const het = Math.min(r.kt, cuoi);
    while (t < het) {
      const vn = t + VN_MS, d = Math.floor(vn / NGAY), trongNgay = vn - d * NGAY;
      const sau = Math.min(het, t - (trongNgay % 3600000) + 3600000);
      o[(d + 3) % 7][Math.floor(trongNgay / 3600000)] += sau - t; // 1/1/1970 là thứ 5
      t = sau;
    }
  }
  return { o };
}

export function canhBao(sql, bay) {
  const ds = [];
  const moi = banMoi(sql);
  for (const m of sql('select * from may where truc_tuyen = 1 and cho_tai_len > 0')) ds.push({ loai: 'dong_bo', muc: 'loi', tieu_de: `${m.ten_may}: ${m.cho_tai_len} profile chưa tải lên máy chủ team`, ai: m.nhan_su });
  for (const m of sql('select * from may where key_loi is not null')) ds.push({ loai: 'key', muc: 'loi', tieu_de: `${m.ten_may}: key ${m.key_loi === 'BI_KHOA' ? 'đã bị khoá' : 'đã hết hạn'}`, ai: m.nhan_su });
  if (moi) for (const m of sql('select * from may where truc_tuyen = 1')) if (m.phien_ban && soSanhPhienBan(m.phien_ban, moi) < 0) ds.push({ loai: 'phien_ban', muc: 'chu_y', tieu_de: `${m.ten_may} đang dùng bản cũ ${m.phien_ban} (mới nhất ${moi})`, ai: m.nhan_su });
  return ds;
}

export const DOC = {
  // nhe: chỉ những số đổi từng lúc, đọc rất ít dòng (trang tự làm mới gọi cái này). Biểu đồ và tổng cả tháng phải quét nhiều dòng,
  // trình duyệt lấy vài phút một lần: Cloudflare giới hạn số dòng đọc mỗi ngày.
  tongQuan(sql, a, bay) {
    const nhe = !!(a && a.nhe);
    const dem = (q, ...p) => sql(q, ...p)[0].n;
    const thang = dauThang(bay, 0), thangTruoc = dauThang(bay, 1);
    const luot = nhe ? null : dem('select count(*) n from phien where bat_dau >= ?', thang);
    const luotTruoc = nhe ? null : dem('select count(*) n from phien where bat_dau >= ? and bat_dau < ?', thangTruoc, thang);
    const dangDung = sql(`select d.id, d.ten, d.thu_muc, d.nen_tang, d.tu_luc, m.nhan_su, m.ten_may, m.ma_may from dang_mo d join may m on m.ma_may = d.ma_may where m.truc_tuyen = 1 order by d.tu_luc desc limit 50`);
    const tuDong = sql('select tu_dong from may where truc_tuyen = 1').reduce((n, r) => n + JSON.parse(r.tu_dong || '[]').length, 0);
    const cb = canhBao(sql, bay);
    return {
      bay,
      so: {
        tong_profile: dem('select coalesce(sum(tong_profile), 0) n from may'),
        tong_profile_truoc: (sql('select tong_profile n from so_lieu_ngay where ngay < ? order by ngay desc limit 1', thang)[0] || { n: null }).n,
        dang_su_dung: dem('select count(distinct d.id) n from dang_mo d join may m on m.ma_may = d.ma_may where m.truc_tuyen = 1'),
        nhan_su: dem('select count(distinct key) n from may'),
        nhan_su_moi: dem('select count(distinct key) n from may where lan_dau >= ?', thang),
        thiet_bi: dem('select count(*) n from may'),
        truc_tuyen: dem('select count(*) n from may where truc_tuyen = 1'),
        luot, luot_truoc: luotTruoc,
        tu_dong: tuDong,
        canh_bao: cb.length,
      },
      dang_dung: dangDung,
      hoat_dong: sql('select * from nhat_ky order by luc desc, id desc limit 12'),
      ...(nhe ? {} : {
        xu_huong: theoNgay(sql, dauNgay(bay) - 13 * NGAY, dauNgay(bay) + NGAY, bay),
        top_web: sql('select ten_mien, sum(luot) luot, count(distinct key) so_nguoi from trang_web where ngay >= ? group by ten_mien order by luot desc limit 8', thang),
        theo_nen: theoNen(sql, thang, dauNgay(bay) + NGAY, bay),
      }),
      may_tt: {
        dang_mo: dem('select count(distinct d.ma_may) n from dang_mo d join may m on m.ma_may = d.ma_may where m.truc_tuyen = 1'),
        bat: dem('select count(*) n from may where truc_tuyen = 1'),
        tat: dem('select count(*) n from may where truc_tuyen = 0'),
      },
      canh_bao: cb,
    };
  },

  chiTietProfile(sql, a) {
    const id = String(a.id || '');
    return { id, lich_su: sql('select nhan_su, ten_may, ten, bat_dau, ket_thuc from phien where profile_id = ? order by bat_dau desc limit 30', id) };
  },

  nhanSu(sql, _a, bay) {
    const homNay = dauNgay(bay);
    const nhom = new Map();
    for (const m of sql('select * from may order by lan_cuoi desc')) {
      const n = nhom.get(m.key) || { key: m.key, nhan_su: m.nhan_su, may: [], truc_tuyen: 0, dang_mo: 0, tong_profile: 0, lan_cuoi: 0, phien_ban: m.phien_ban };
      n.may.push({ ma_may: m.ma_may, ten_may: m.ten_may, truc_tuyen: !!m.truc_tuyen });
      n.truc_tuyen += m.truc_tuyen ? 1 : 0;
      n.tong_profile += m.tong_profile;
      n.lan_cuoi = Math.max(n.lan_cuoi, m.lan_cuoi);
      nhom.set(m.key, n);
    }
    for (const r of sql('select m.key, count(*) n from dang_mo d join may m on m.ma_may = d.ma_may group by m.key')) if (nhom.has(r.key)) nhom.get(r.key).dang_mo = r.n;
    for (const r of sql('select key, sum(min(coalesce(ket_thuc, ?), ?) - max(bat_dau, ?)) ms, count(*) luot from phien where coalesce(ket_thuc, ?) > ? group by key', bay, bay, homNay, bay, homNay)) {
      if (nhom.has(r.key)) Object.assign(nhom.get(r.key), { ms_hom_nay: r.ms || 0, luot_hom_nay: r.luot });
    }
    return { hang: [...nhom.values()] };
  },

  thietBi(sql) {
    const moi = banMoi(sql);
    const mo = new Map(sql('select ma_may, count(*) n from dang_mo group by ma_may').map((r) => [r.ma_may, r.n]));
    return {
      ban_moi: moi,
      hang: sql('select * from may order by truc_tuyen desc, lan_cuoi desc').map((m) => ({ ...mauMay(m), dang_mo: mo.get(m.ma_may) || 0, ban_cu: !!(moi && m.phien_ban && soSanhPhienBan(m.phien_ban, moi) < 0) })),
    };
  },

  chiTietMay(sql, a, bay) {
    const m = sql('select * from may where ma_may = ?', String(a.ma_may || ''))[0];
    if (!m) return null;
    return {
      may: mauMay(m),
      dang_mo: sql('select * from dang_mo where ma_may = ? order by tu_luc desc', m.ma_may),
      hoat_dong: sql('select * from nhat_ky where ma_may = ? order by luc desc, id desc limit 40', m.ma_may),
      top_web: sql('select ten_mien, sum(luot) luot from trang_web where ma_may = ? and ngay >= ? group by ten_mien order by luot desc limit 10', m.ma_may, dauNgay(bay) - 29 * NGAY),
    };
  },

  lichSu(sql, a, bay) {
    const k = khoang(a, bay);
    const dk = ['luc >= ?', 'luc < ?'], ts = [k.tu, k.den];
    if (a.viec) { dk.push('viec = ?'); ts.push(a.viec); }
    if (a.tim) { dk.push("(nhan_su like ? escape '\\' or doi_tuong like ? escape '\\' or ten_may like ? escape '\\')"); const s = '%' + String(a.tim).replace(/[\\%_]/g, (c) => '\\' + c) + '%'; ts.push(s, s, s); }
    const trang = Math.max(1, Number(a.trang) || 1), moi = Math.min(Number(a.moi) || 50, 5000);
    const where = dk.join(' and ');
    return {
      tong: sql(`select count(*) n from nhat_ky where ${where}`, ...ts)[0].n, trang, moi,
      hang: sql(`select * from nhat_ky where ${where} order by luc desc, id desc limit ? offset ?`, ...ts, moi, (trang - 1) * moi),
      viec: sql('select distinct viec from nhat_ky order by viec').map((r) => r.viec),
    };
  },

  // Website vào nhiều nhất trong khoảng ngày, lọc được theo cộng sự (key), máy và chữ trong tên miền.
  website(sql, a, bay) {
    const { tu, den, so_ngay } = khoang(a, bay);
    const dk = ['ngay >= ?', 'ngay < ?'], ts = [tu, den];
    if (a.key) { dk.push('key = ?'); ts.push(String(a.key)); }
    if (a.ma_may) { dk.push('ma_may = ?'); ts.push(String(a.ma_may)); }
    if (a.tim) { dk.push("ten_mien like ? escape '\\'"); ts.push('%' + String(a.tim).toLowerCase().replace(/[\\%_]/g, (x) => '\\' + x) + '%'); }
    const w = dk.join(' and ');
    const tong = sql(`select coalesce(sum(luot), 0) luot, count(distinct ten_mien) so_web, count(distinct key) so_nguoi, count(distinct ma_may) so_may from trang_web where ${w}`, ...ts)[0];
    const theoNgayMap = new Map(sql(`select ngay, sum(luot) luot from trang_web where ${w} group by ngay`, ...ts).map((r) => [r.ngay, r.luot]));
    const theo_ngay = [];
    for (let d = tu; d < den; d += NGAY) theo_ngay.push({ ngay: d, luot: theoNgayMap.get(d) || 0 });
    const trang = Math.max(1, Number(a.trang) || 1), moi = Math.min(Number(a.moi) || 50, 5000);
    return {
      tu, den, so_ngay, giu_ngay: cauHinh(sql).giu_ngay, ...tong, theo_ngay, trang, moi,
      hang: sql(`select ten_mien, sum(luot) luot, count(distinct key) so_nguoi, count(distinct ma_may) so_may, max(ngay) cuoi from trang_web where ${w} group by ten_mien order by luot desc, ten_mien limit ? offset ?`, ...ts, moi, (trang - 1) * moi),
      nguoi: sql('select key, max(nhan_su) nhan_su from trang_web where ngay >= ? and ngay < ? group by key order by nhan_su', tu, den),
      may: sql('select ma_may, max(ten_may) ten_may, max(nhan_su) nhan_su from trang_web where ngay >= ? and ngay < ? group by ma_may order by ten_may', tu, den),
    };
  },

  // Một website: ai, trên máy nào, bao nhiêu lượt, trong khoảng ngày đã chọn.
  chiTietWebsite(sql, a, bay) {
    const { tu, den } = khoang(a, bay);
    const ten = String(a.ten_mien || '');
    return {
      ten_mien: ten,
      may: sql('select ma_may, max(ten_may) ten_may, max(nhan_su) nhan_su, sum(luot) luot, max(ngay) cuoi from trang_web where ten_mien = ? and ngay >= ? and ngay < ? group by ma_may order by luot desc limit 200', ten, tu, den),
      theo_ngay: sql('select ngay, sum(luot) luot from trang_web where ten_mien = ? and ngay >= ? and ngay < ? group by ngay order by ngay desc limit 366', ten, tu, den),
    };
  },

  baoCaoSo(sql, a, bay) {
    const { tu, den, so_ngay } = khoang(a, bay);
    const ms = [bay, Math.min(den, bay), tu], dk = [den, bay, tu];
    return {
      tu, den, so_ngay, giu_ngay: cauHinh(sql).giu_ngay,
      theo_ngay: theoNgay(sql, tu, den, bay),
      so_profile: sql(`select count(distinct profile_id) n from phien where ${TRONG_KHOANG}`, ...dk)[0].n,
      theo_nen: theoNen(sql, tu, den, bay),
      theo_gio: oTheoGio(sql, tu, den, bay),
      dinh: dinhTheoNgay(sql, tu, den, bay),
      theo_nguoi: sql(`select nhan_su, key, count(*) luot, count(distinct profile_id) so_profile, ${MS_PHIEN} ms from phien where ${TRONG_KHOANG} group by key order by ms desc limit 200`, ...ms, ...dk),
      theo_profile: sql(`select max(ten) ten, max(thu_muc) thu_muc, max(nen_tang) nen_tang, count(*) luot, count(distinct key) so_nguoi, ${MS_PHIEN} ms from phien where ${TRONG_KHOANG} group by profile_id order by ms desc limit 20`, ...ms, ...dk),
    };
  },

  cauHinh(sql) { return cauHinh(sql); },
};

export const GHI = {
  luuCauHinh(sql, a) {
    if (a.sau_giay != null) sql("insert or replace into cau_hinh(k, v) values ('sau_giay', ?)", String(Math.max(60, Math.min(3600, Number(a.sau_giay) || MAC_DINH.sau_giay))));
    if (a.giu_ngay != null) sql("insert or replace into cau_hinh(k, v) values ('giu_ngay', ?)", String(Math.max(7, Math.min(365, Number(a.giu_ngay) || MAC_DINH.giu_ngay))));
    if (a.phien_ban_moi != null) sql("insert or replace into cau_hinh(k, v) values ('phien_ban_moi', ?)", chu(a.phien_ban_moi, 20).trim());
    return cauHinh(sql);
  },
  // Xoá một máy khỏi danh sách (máy đã thanh lý). Lịch sử của nó vẫn giữ.
  xoaMay(sql, a, bay) {
    const m = sql('select * from may where ma_may = ?', String(a.ma_may || ''))[0];
    if (!m) return { ok: false };
    for (const r of sql('select * from dang_mo where ma_may = ?', m.ma_may)) dongPhien(sql, m, r, m.lan_cuoi, 'Đóng profile (mất kết nối)');
    sql('delete from may where ma_may = ?', m.ma_may);
    sql('delete from xac_thuc where ma_may = ?', m.ma_may);
    ghiNhatKy(sql, bay, 'Admin xoá máy', m.nhan_su, m.ten_may, m.ma_may, '', '');
    return { ok: true };
  },
  // Key vừa đổi trên web: quên kết quả đã nhớ, để báo cáo sau của máy thấy ngay.
  quenKey(sql, a) {
    sql('delete from xac_thuc where key = ?', String(a.key || ''));
    return { ok: true };
  },
  ghiKey(sql, a, bay) {
    ghiNhatKy(sql, bay, a.viec, 'Admin', '', '', a.key, a.chi_tiet);
    return { ok: true };
  },
};
