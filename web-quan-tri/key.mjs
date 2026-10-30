// Quản lý key trên web: chỉ đụng tới key HLG- của Hir-Login. Kho key dùng chung với HIR-IMG, HirS và các tool khác,
// nên mọi lệnh đều kiểm tra tiền tố trước khi chuyển sang Worker `hir-key`.
const CHU = 'ABCDEFGHJKLMNPQRSTUVWXYZ23456789';

export function sinhKey(tien = 'HLG') {
  const b = new Uint8Array(12);
  crypto.getRandomValues(b);
  const s = [...b].map((x) => CHU[x % CHU.length]).join('');
  return `${tien}-${s.slice(0, 4)}-${s.slice(4, 8)}-${s.slice(8, 12)}`;
}

// Các tool dùng chung kho key (giống SP trong bot-key.js); tool thêm bằng /themsp ở bot nằm trong bảng san_pham của kho.
export const SAN_PHAM_MAC_DINH = [['HLG', 'Hir-Login'], ['HIR', 'Hir-F'], ['HRS', 'HirS'], ['TPT', 'Shoptik+'], ['HSP', 'HIR-Shopee'], ['HFS', 'HirFS'], ['NWS', 'NowSreen'], ['STO', 'Story']];
// Mỗi đợt tối đa bấy nhiêu key: một yêu cầu của Worker chỉ được gọi ra ngoài khoảng 50 lần (gói miễn phí).
export const LO_TOI_DA = 25;
const SONG_SONG = 5;
// Kho key chỉ nhận tối đa 100 tham số mỗi câu truy vấn, nên danh sách lấy từng trang nhỏ.
export const TRANG_KEY = 90;

async function dsSanPham(goi) {
  const ds = SAN_PHAM_MAC_DINH.map(([tien, ten]) => ({ tien, ten }));
  let them = [];
  try { them = (await goi('sanPham', {})) || []; } catch (_) { /* kho không trả lời: dùng danh sách mặc định */ }
  for (const s of them) if (s && /^[A-Z0-9]{2,6}$/.test(String(s.tien)) && !ds.some((d) => d.tien === s.tien)) ds.push({ tien: s.tien, ten: String(s.ten || s.tien).slice(0, 40) });
  return ds;
}

// Tạo một đợt key, mỗi key cho đúng 1 máy. Lỗi giữa chừng thì trả những key đã tạo kèm lời báo lỗi, để không mất key nào.
async function taoLo(b, goi, kho) {
  const tien = String(b.tien || '').trim().toUpperCase();
  if (!(await dsSanPham(goi)).some((s) => s.tien === tien)) throw new Error('Tool này chưa có trong kho key');
  const soLuong = Math.max(1, Math.min(LO_TOI_DA, Math.floor(Number(b.so_luong)) || 1));
  const het_han = hanTuNgay(b.so_ngay);
  const goc = String(b.ghi_chu || '').trim().slice(0, 100);
  const dau = b.danh_so_tu == null ? null : Math.max(1, Math.floor(Number(b.danh_so_tu)) || 1);
  const rong = Math.max(1, Math.min(5, Math.floor(Number(b.rong)) || 3));
  const ghiChu = (i) => (dau == null ? goc : `${goc ? goc + ' ' : ''}${String(dau + i).padStart(rong, '0')}`).slice(0, 120);
  const tao1 = async (i) => {
    for (let lan = 0; ; lan++) {
      try { return await goi('taoKey', { key: sinhKey(tien), ghi_chu: ghiChu(i), het_han, so_may: 1 }); }
      catch (e) { if (lan >= 2 || !/đã tồn tại/.test(String(e && e.message))) throw e; }
    }
  };
  const xong = [];
  let loi = '';
  for (let i = 0; i < soLuong && !loi; i += SONG_SONG) {
    const kq = await Promise.allSettled(Array.from({ length: Math.min(SONG_SONG, soLuong - i) }, (_, j) => tao1(i + j)));
    for (const r of kq) {
      if (r.status === 'fulfilled') xong.push(r.value);
      else loi = loi || String((r.reason && r.reason.message) || r.reason);
    }
  }
  if (xong.length) {
    const chiTiet = `${xong.length} key ${tien}, mỗi key 1 máy${goc ? ', ' + goc : ''}`;
    try { await goi('nhatKy', { ai: 'web', viec: 'taoLo', key: xong[0].key, chi_tiet: chiTiet }); } catch (_) { /* nhật ký là phần phụ */ }
    await kho.ghi('ghiKey', { viec: 'Tạo key số lượng lớn', key: `${xong.length} key ${tien}`, chi_tiet: chiTiet });
  }
  return { hang: xong.map((x) => ({ key: x.key, ghi_chu: x.ghi_chu, het_han: x.het_han })), loi };
}

const chuan = (k) => String(k == null ? '' : k).trim().toUpperCase();
function canHlg(k) {
  const key = chuan(k);
  if (!/^HLG-[A-Z0-9]{4}-[A-Z0-9]{4}-[A-Z0-9]{4}$/.test(key)) throw new Error('Chỉ quản lý được key Hir-Login (HLG-XXXX-XXXX-XXXX)');
  return key;
}

function hanTuNgay(ngay) {
  if (ngay == null || ngay === '' || Number(ngay) === 0) return null;
  const n = Number(ngay);
  if (!Number.isFinite(n) || n < 0 || n > 3650) throw new Error('Số ngày không hợp lệ');
  return new Date(Date.now() + n * 86400000).toISOString().replace('Z', '+00:00');
}

const THAY_DUOC = ['ghi_chu', 'het_han', 'so_may', 'khoa', 'ho_ten', 'gmail', 'zalo'];

// goi(tên hàm trên hir-key, tham số); kho: Durable Object của web (ghi lịch sử, quên xác thực đã nhớ).
export async function hanhDongKey(b, goi, kho) {
  const viec = String(b.viec || '');
  if (viec === 'danhSach') {
    const limit = Math.max(1, Math.min(TRANG_KEY, Math.floor(Number(b.limit)) || TRANG_KEY)), off = Math.max(0, Math.floor(Number(b.off)) || 0);
    const hang = (await goi('danhSach', { tien: 'HLG', limit, off })) || [];
    return { hang: hang.filter((k) => /^HLG-/.test(k.key)), het: hang.length < limit };
  }
  if (viec === 'sanPham') return { hang: await dsSanPham(goi) };
  if (viec === 'taoLo') return taoLo(b, goi, kho);
  if (viec === 'tao') {
    const soLuong = Math.max(1, Math.min(50, Number(b.so_luong) || 1));
    const het_han = hanTuNgay(b.so_ngay);
    const taoRa = [];
    for (let i = 0; i < soLuong; i++) {
      const the = await goi('taoKey', { key: sinhKey(), ghi_chu: String(b.ghi_chu || '').slice(0, 120), het_han, so_may: Number(b.so_may) || 1 });
      taoRa.push(the);
      await goi('nhatKy', { ai: 'web', viec: 'tao', key: the.key, chi_tiet: the.ghi_chu });
    }
    await kho.ghi('ghiKey', { viec: 'Tạo key', key: soLuong === 1 ? taoRa[0].key : `${soLuong} key`, chi_tiet: String(b.ghi_chu || '') });
    return { hang: taoRa };
  }

  const key = canHlg(b.key);
  let the;
  if (viec === 'sua') {
    const thay = {};
    for (const c of THAY_DUOC) if (b.thay && Object.prototype.hasOwnProperty.call(b.thay, c)) thay[c] = b.thay[c];
    if (b.so_ngay !== undefined) thay.het_han = hanTuNgay(b.so_ngay);
    the = await goi('suaKey', { key, thay });
    await goi('nhatKy', { ai: 'web', viec: 'sua', key, chi_tiet: Object.keys(thay).join(',') });
    await kho.ghi('ghiKey', { viec: 'Sửa key', key, chi_tiet: Object.keys(thay).join(', ') });
  } else if (viec === 'khoa' || viec === 'mo') {
    the = await goi('suaKey', { key, thay: { khoa: viec === 'khoa' } });
    await goi('nhatKy', { ai: 'web', viec, key });
    await kho.ghi('ghiKey', { viec: viec === 'khoa' ? 'Khoá key' : 'Mở khoá key', key });
  } else if (viec === 'goMay') {
    await goi('goMay', { key });
    await goi('nhatKy', { ai: 'web', viec: 'goMay', key });
    await kho.ghi('ghiKey', { viec: 'Gỡ máy khỏi key', key });
    the = await goi('layKey', { key });
  } else if (viec === 'xoa') {
    await goi('nhatKy', { ai: 'web', viec: 'xoa', key });
    await goi('xoaKey', { key });
    await kho.ghi('ghiKey', { viec: 'Xoá key', key });
    the = null;
  } else {
    throw new Error('Không có việc này');
  }
  await kho.ghi('quenKey', { key });
  return { the };
}
