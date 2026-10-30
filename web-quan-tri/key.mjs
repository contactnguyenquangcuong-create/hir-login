// Quản lý key trên web: chỉ đụng tới key HLG- của Hir-Login. Kho key dùng chung với HIR-IMG, HirS và các tool khác,
// nên mọi lệnh đều kiểm tra tiền tố trước khi chuyển sang Worker `hir-key`.
const CHU = 'ABCDEFGHJKLMNPQRSTUVWXYZ23456789';

export function sinhKey() {
  const b = new Uint8Array(12);
  crypto.getRandomValues(b);
  const s = [...b].map((x) => CHU[x % CHU.length]).join('');
  return `HLG-${s.slice(0, 4)}-${s.slice(4, 8)}-${s.slice(8, 12)}`;
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
    const hang = (await goi('danhSach', { tien: 'HLG', limit: 5000, off: 0 })) || [];
    return { hang: hang.filter((k) => /^HLG-/.test(k.key)) };
  }
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
