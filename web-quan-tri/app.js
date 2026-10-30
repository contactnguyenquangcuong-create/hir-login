// Giao diện web quản trị Hir-Login. Không framework, không build: một file, chạy thẳng.
// Tên profile, tên máy... do máy cộng sự gửi lên nên KHÔNG được tin: mọi chữ đều đi qua textContent (hàm h),
// chỉ biểu tượng hằng số trong BIEU mới dùng innerHTML.
(function () {
  'use strict';

  // ---- Tiện ích DOM ----
  function h(the, thuocTinh) {
    const el = document.createElement(the);
    for (const [k, v] of Object.entries(thuocTinh || {})) {
      if (v == null || v === false) continue;
      if (k === 'lop') el.className = v;
      else if (k === 'bam') el.addEventListener('click', v);
      else if (k.startsWith('on')) el.addEventListener(k.slice(2), v);
      else if (k === 'giaTri') el.value = v;
      else el.setAttribute(k, v === true ? '' : v);
    }
    for (let i = 2; i < arguments.length; i++) them(el, arguments[i]);
    return el;
  }
  function them(el, c) {
    if (c == null || c === false) return;
    if (Array.isArray(c)) c.forEach((x) => them(el, x));
    else el.append(c.nodeType ? c : document.createTextNode(String(c)));
  }
  const xoa = (el) => { el.textContent = ''; return el; };

  const NET = 'fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"';
  const BIEU = {
    tongQuan: '<rect x="3" y="3" width="7" height="8" rx="1.5"/><rect x="14" y="3" width="7" height="5" rx="1.5"/><rect x="14" y="12" width="7" height="9" rx="1.5"/><rect x="3" y="15" width="7" height="6" rx="1.5"/>',
    thietBi: '<rect x="3" y="4" width="18" height="12" rx="2"/><path d="M8 20h8M12 16v4"/>',
    nhanSu: '<circle cx="9" cy="8" r="3.5"/><path d="M3 20c0-3.3 2.7-6 6-6s6 2.7 6 6M16 4.6a3.5 3.5 0 0 1 0 6.8M18 14.4c1.8.8 3 2.6 3 4.6"/>',
    lichSu: '<path d="M3.500 12a8.500 8.500 0 1 0 2.600-6.100L3.500 8.500M3.500 4v4.500H8M12 7.500V12l3 2"/>',
    baoCao: '<path d="M4 20V10M10 20V4M16 20v-7M21 20H3"/>',
    key: '<circle cx="8" cy="15" r="4.500"/><path d="M11.500 11.500 20 3M16 7l3 3M13.500 9.500l2 2"/>',
    caiDat: '<circle cx="12" cy="12" r="3"/><path d="M12 2.500v3M12 18.500v3M2.500 12h3M18.500 12h3M5.300 5.300l2.100 2.100M16.600 16.600l2.100 2.100M5.300 18.700l2.100-2.100M16.600 7.400l2.100-2.100"/>',
    tim: '<circle cx="11" cy="11" r="6.500"/><path d="m20 20-4-4"/>',
    cong: '<path d="M12 5v14M5 12h14"/>',
    dong: '<path d="M6 6l12 12M18 6 6 18"/>',
    sao: '<rect x="9" y="9" width="11" height="11" rx="2"/><path d="M5 15a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2"/>',
    tai: '<path d="M12 3v12M7 10l5 5 5-5M4 20h16"/>',
    trang: '<circle cx="12" cy="12" r="4"/><path d="M12 2.500v2M12 19.500v2M2.500 12h2M19.500 12h2M5.300 5.300l1.400 1.400M17.300 17.300l1.400 1.400M5.300 18.700l1.400-1.400M17.300 6.700l1.400-1.400"/>',
    toi: '<path d="M20 14.500A8.500 8.500 0 0 1 9.500 4a8.500 8.500 0 1 0 10.500 10.500z"/>',
    ra: '<path d="M15 4h3a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2h-3M10 8l-4 4 4 4M6 12h10"/>',
  };
  function bieu(ten) {
    const s = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    s.setAttribute('viewBox', '0 0 24 24');
    s.setAttribute('aria-hidden', 'true');
    s.innerHTML = `<g ${NET}>${BIEU[ten] || ''}</g>`;
    return s;
  }

  // ---- Định dạng ----
  const VN = 'Asia/Ho_Chi_Minh';
  const gio = (ms) => new Date(ms).toLocaleTimeString('vi-VN', { hour: '2-digit', minute: '2-digit', timeZone: VN });
  const ngayGio = (ms) => new Date(ms).toLocaleString('vi-VN', { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit', timeZone: VN });
  const ngayNgan = (ms) => new Date(ms).toLocaleDateString('vi-VN', { day: '2-digit', month: '2-digit', timeZone: VN });
  const ngayDu = (ms) => new Date(ms).toLocaleDateString('vi-VN', { day: '2-digit', month: '2-digit', year: 'numeric', timeZone: VN });
  const cungNgay = (a, b) => ngayDu(a) === ngayDu(b);
  const luc = (ms) => (cungNgay(ms, Date.now()) ? gio(ms) : ngayGio(ms));
  function truoc(ms) {
    const g = Math.max(0, Math.round((Date.now() - ms) / 1000));
    if (g < 60) return 'vừa xong';
    if (g < 3600) return `${Math.floor(g / 60)} phút trước`;
    if (g < 86400) return `${Math.floor(g / 3600)} giờ trước`;
    return `${Math.floor(g / 86400)} ngày trước`;
  }
  function thoiLuong(ms) {
    if (!(ms > 0)) return '0 giờ';
    const p = Math.max(0, Math.round(ms / 60000));
    if (p < 1) return '< 1 phút';
    if (p < 60) return `${p} phút`;
    const g = Math.floor(p / 60);
    return p % 60 ? `${g}g ${p % 60}p` : `${g} giờ`;
  }
  const so = (n) => Number(n || 0).toLocaleString('vi-VN');
  const TEN_NEN = { shopee: 'Shopee', facebook: 'Facebook', tiktok: 'TikTok', google: 'Google', khac: 'Khác' };
  const CHU_NEN = { shopee: 'S', facebook: 'F', tiktok: 'T', google: 'G', khac: '·' };
  const kyHieu = (n) => h('span', { lop: 'ky-hieu ' + n, title: TEN_NEN[n] || '' }, CHU_NEN[n] || '·');
  const daoTen = (t) => { const p = String(t || '?').trim().split(/\s+/); return (p.length > 1 ? p[0][0] + p[p.length - 1][0] : p[0].slice(0, 2)).toUpperCase(); };
  const nhanTrang = (t, loai) => h('span', { lop: 'nhan ' + (loai || '') }, t);
  const trong = (tieuDe, ghi) => h('div', { lop: 'trong' }, h('b', null, tieuDe), ghi);

  // ---- Gọi server ----
  async function api(duong, body) {
    const r = await fetch(duong, body === undefined ? {} : { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) });
    const j = await r.json().catch(() => null);
    if (r.status === 401 && duong.startsWith('/api/qt/')) { veDangNhap(); throw new Error('Chưa đăng nhập'); }
    if (!r.ok) throw new Error((j && j.loi) || 'Lỗi ' + r.status);
    return j;
  }
  const doc = (ten, a) => api(`/api/qt/${ten}?` + new URLSearchParams(Object.entries(a || {}).filter(([, v]) => v != null && v !== '')));
  const ghi = (ten, b) => api('/api/qt/' + ten, b);
  const key = (b) => ghi('key', b);

  function taiFile(ten, noiDung) {
    const a = h('a', { href: URL.createObjectURL(new Blob(['\ufeff' + noiDung], { type: 'text/csv;charset=utf-8' })), download: ten });
    a.click();
    setTimeout(() => URL.revokeObjectURL(a.href), 5000);
  }
  const oCsv = (s) => { const t = String(s == null ? '' : s); return `"${(/^[=+\-@]/.test(t) ? "'" + t : t).replace(/"/g, '""')}"`; };
  function bao(chu, loi) {
    const t = h('div', { lop: 'tb' + (loi ? ' loi' : '') }, chu);
    document.getElementById('thong-bao').append(t);
    setTimeout(() => t.remove(), loi ? 5000 : 2600);
  }
  async function chep(s) {
    try { await navigator.clipboard.writeText(s); bao('Đã chép'); } catch (_) { bao('Không chép được', true); }
  }

  // ---- Lớp nổi: ngăn kéo và hộp thoại ----
  const lop = () => document.getElementById('lop-noi');
  let dongLop = null;
  let lopKhoa = false;
  function moLop(the) {
    const l = xoa(lop());
    const dong = () => { xoa(l); dongLop = null; lopKhoa = false; document.removeEventListener('keydown', phim); };
    function phim(e) { if (e.key === 'Escape' && !lopKhoa) dong(); }
    document.addEventListener('keydown', phim);
    dongLop = dong;
    l.append(h('div', { lop: 'man', bam: () => { if (!lopKhoa) dong(); } }), the);
    return dong;
  }
  function moNgan(tieuDe, phuDe, than) {
    const n = h('aside', { lop: 'ngan', role: 'dialog', 'aria-label': tieuDe },
      h('div', { lop: 'dau' }, h('div', { lop: '' }, h('h3', null, tieuDe), phuDe && h('div', { lop: 'dong-phu' }, phuDe)),
        h('button', { lop: 'nut icon', 'aria-label': 'Đóng', bam: () => dongLop && dongLop() }, bieu('dong'))),
      h('div', { lop: 'than' }, than));
    moLop(n);
  }
  function moHop(the) {
    const hop = h('div', { lop: 'hop', role: 'dialog', 'aria-modal': 'true' }, the);
    moLop(hop);
    const dau = hop.querySelector('input,select,button');
    if (dau) dau.focus();
  }
  function xacNhan(tieuDe, noiDung, nhanNut, nguy) {
    return new Promise((tra) => {
      const xong = (v) => { if (dongLop) dongLop(); tra(v); };
      moHop([h('h3', null, tieuDe), h('p', null, noiDung),
        h('div', { lop: 'chan-hop' },
          h('button', { lop: 'nut', bam: () => xong(false) }, 'Huỷ'),
          h('button', { lop: 'nut ' + (nguy ? 'nguy-dac' : 'chinh-nut'), bam: () => xong(true) }, nhanNut))]);
    });
  }
  const truong = (nhan, o, ghiChu) => h('div', { lop: 'truong' }, h('label', null, nhan), o, ghiChu && h('small', null, ghiChu));

  // ---- Trang: mỗi trang dựng khung một lần, `nap()` chỉ thay phần dữ liệu ----
  const dauTrang = (tieuDe, moTa, hd) => h('header', { lop: 'dau-trang' }, h('div', null, h('h1', null, tieuDe), moTa && h('p', null, moTa)), hd && h('div', { lop: 'hanh-dong' }, hd));
  function oTim(goi, goiY) {
    let hen;
    return h('span', { lop: 'o-tim' }, bieu('tim'), h('input', { lop: 'o', type: 'search', placeholder: goiY, 'aria-label': goiY, oninput: (e) => { clearTimeout(hen); hen = setTimeout(() => goi(e.target.value.trim()), 220); } }));
  }
  function tabNho(muc, chon, doi) {
    const nut = muc.map(([v, t]) => h('button', { lop: v === chon ? 'bat' : '', type: 'button', 'data-v': v, bam: () => { nut.forEach((x) => x.classList.toggle('bat', x.dataset.v === v)); doi(v); } }, t));
    return h('div', { lop: 'tab-nho', role: 'tablist' }, nut);
  }
  // Bộ lọc ngày dùng chung cho Báo cáo và Lịch sử: đổi ở trang này, sang trang kia vẫn giữ.
  // Mọi ngày tính theo giờ Việt Nam, gửi lên server dạng YYYY-MM-DD.
  const KHOANG_NHANH = [['hom_nay', 'Hôm nay'], ['hom_qua', 'Hôm qua'], ['7n', '7 ngày'], ['30n', '30 ngày'], ['90n', '90 ngày'], ['thang_nay', 'Tháng này'], ['thang_truoc', 'Tháng trước'], ['tuy_chon', 'Tuỳ chọn']];
  const khoangChon = { loai: '7n', tu: '', den: '' };
  const isoNgay = (ms) => new Date(ms).toISOString().slice(0, 10);
  const homNayVN = () => Math.floor((Date.now() + 7 * 3600000) / 86400000) * 86400000;
  function tinhKhoang(loai) {
    const nay = homNayVN(), d = new Date(nay), y = d.getUTCFullYear(), m = d.getUTCMonth();
    const lui = (n) => [isoNgay(nay - (n - 1) * 86400000), isoNgay(nay)];
    if (loai === 'hom_nay') return lui(1);
    if (loai === 'hom_qua') return [isoNgay(nay - 86400000), isoNgay(nay - 86400000)];
    if (loai === '30n') return lui(30);
    if (loai === '90n') return lui(90);
    if (loai === 'thang_nay') return [isoNgay(Date.UTC(y, m, 1)), isoNgay(nay)];
    if (loai === 'thang_truoc') return [isoNgay(Date.UTC(y, m - 1, 1)), isoNgay(Date.UTC(y, m, 0))];
    return lui(7);
  }
  function khoangHien() {
    const [tu, den] = khoangChon.loai === 'tuy_chon' ? [khoangChon.tu, khoangChon.den] : tinhKhoang(khoangChon.loai);
    return { tu, den };
  }
  const ngayDoc = (iso) => iso.split('-').reverse().join('/');
  function boLocNgay(doi) {
    const dau = khoangHien();
    const chu = h('span', { lop: 'khoang-chu' });
    function veChu() {
      const k = khoangHien();
      const n = Math.round((Date.parse(k.den) - Date.parse(k.tu)) / 86400000) + 1;
      chu.textContent = k.tu === k.den ? ngayDoc(k.tu) : `${ngayDoc(k.tu)} – ${ngayDoc(k.den)} · ${so(n)} ngày`;
    }
    function doiTuy() {
      if (!oTu.value || !oDen.value) return;
      const n = (Date.parse(oDen.value) - Date.parse(oTu.value)) / 86400000;
      if (n < 0) return bao('Ngày bắt đầu phải trước ngày kết thúc', true);
      if (n >= 366) return bao('Chọn tối đa một năm', true);
      Object.assign(khoangChon, { loai: 'tuy_chon', tu: oTu.value, den: oDen.value });
      veChu();
      doi();
    }
    const oTu = h('input', { lop: 'o o-ngay', type: 'date', max: isoNgay(homNayVN()), 'aria-label': 'Từ ngày', giaTri: dau.tu, onchange: doiTuy });
    const oDen = h('input', { lop: 'o o-ngay', type: 'date', max: isoNgay(homNayVN()), 'aria-label': 'Đến ngày', giaTri: dau.den, onchange: doiTuy });
    const tuy = h('span', { lop: 'chon-ngay' + (khoangChon.loai === 'tuy_chon' ? '' : ' hidden') }, oTu, h('span', { lop: 'mo' }, 'đến'), oDen);
    veChu();
    const tab = tabNho(KHOANG_NHANH, khoangChon.loai, (v) => {
      tuy.classList.toggle('hidden', v !== 'tuy_chon');
      if (v === 'tuy_chon') { const k = khoangHien(); oTu.value = k.tu; oDen.value = k.den; Object.assign(khoangChon, { loai: v, tu: k.tu, den: k.den }); veChu(); return; }
      khoangChon.loai = v;
      const k = khoangHien(); oTu.value = k.tu; oDen.value = k.den;
      veChu();
      doi();
    });
    return h('div', { lop: 'loc' }, tab, tuy, chu);
  }
  function phanTrang(trang, moi, tong, doi) {
    const het = Math.max(1, Math.ceil(tong / moi));
    return h('div', { lop: 'chan-bang' }, h('span', null, `${so(tong)} dòng`),
      h('span', { lop: 'hanh-dong' },
        h('button', { lop: 'nut nho', disabled: trang <= 1, bam: () => doi(trang - 1) }, 'Trước'),
        h('span', { lop: 'so-mono' }, `${trang} / ${het}`),
        h('button', { lop: 'nut nho', disabled: trang >= het, bam: () => doi(trang + 1) }, 'Sau')));
  }
  function bang(cot, hang, dong, bamHang) {
    return h('div', { lop: 'cuon' }, h('table', null,
      h('thead', null, h('tr', null, cot.map((c) => (typeof c === 'string' ? h('th', null, c) : h('th', { lop: c.lop }, c.t))))),
      h('tbody', null, hang.map((r) => h('tr', { lop: bamHang ? 'bam' : '', bam: bamHang ? () => bamHang(r) : null }, dong(r).map((o) => (o && o.nodeName === 'TD' ? o : h('td', null, o))))))));
  }
  const nguoiChip = (ten, phu) => h('div', { lop: 'chip-profile' }, h('span', { lop: 'dan-nguoi' }, daoTen(ten)), h('div', { lop: '' }, h('div', { lop: 'dong-chinh cat' }, ten || '—'), phu && h('div', { lop: 'dong-phu cat' }, phu)));
  const profileChip = (ten, thuMuc, nen) => h('div', { lop: 'chip-profile' }, kyHieu(nen), h('div', { lop: '' }, h('div', { lop: 'dong-chinh cat' }, ten || '—'), h('div', { lop: 'dong-phu cat' }, thuMuc || 'Không folder')));

  function dongThoiGian(ds) {
    if (!ds.length) return trong('Chưa có hoạt động', 'Lịch sử xuất hiện khi các máy bắt đầu báo cáo.');
    return h('ul', { lop: 'dong-tg' }, ds.map((n) => h('li', { lop: /^Mở|^Chạy/.test(n.viec) ? 'mo-p' : /mất kết nối|xoá|Xoá|Khoá/.test(n.viec) ? 'xau' : '' },
      h('span', { lop: 'dau-cham' }),
      h('span', { lop: 'noi', title: `${n.viec} ${n.doi_tuong}` }, h('b', null, n.nhan_su || 'Hệ thống'), ` ${n.viec.toLowerCase()}`, n.doi_tuong ? ` · ${n.doi_tuong}` : ''),
      h('time', null, luc(n.luc)))));
  }

  // ---- Biểu đồ: SVG vẽ theo đúng bề rộng khung (nên chữ không bị kéo giãn), không thư viện ----
  // Một chuỗi số thì một màu; nhiều thực thể thì màu cố định theo thực thể (mau-1..5), không theo thứ hạng.
  const MAU_NEN = { shopee: 'mau-1', facebook: 'mau-2', tiktok: 'mau-3', google: 'mau-4', khac: 'mau-5' };
  const SVG = 'http://www.w3.org/2000/svg';
  function v(the, a) {
    const e = document.createElementNS(SVG, the);
    for (const k in a || {}) if (a[k] != null) e.setAttribute(k, a[k]);
    for (let i = 2; i < arguments.length; i++) if (arguments[i] != null) e.append(arguments[i]);
    return e;
  }
  let meoEl = null;
  function hienMeo(x, y, tieuDe, dong) {
    if (!meoEl) { meoEl = h('div', { lop: 'meo-bd', role: 'status' }); document.body.append(meoEl); }
    xoa(meoEl).append(h('div', { lop: 'meo-dau' }, tieuDe), ...dong.map((d) => h('div', { lop: 'meo-dong' }, d.mau ? h('i', { lop: 'meo-khoa ' + d.mau }) : null, h('b', null, d.gia), h('span', null, d.nhan))));
    meoEl.classList.add('hien');
    const r = meoEl.getBoundingClientRect();
    meoEl.style.left = Math.max(8, Math.min(window.innerWidth - r.width - 8, x + 14)) + 'px';
    meoEl.style.top = Math.max(8, y - r.height - 12 < 8 ? y + 16 : y - r.height - 12) + 'px';
  }
  function anMeo() { if (meoEl) meoEl.classList.remove('hien'); }
  function khungVe(cao, ve) {
    const el = h('div', { lop: 'bd' });
    el.style.height = cao + 'px';
    let cu = 0;
    new ResizeObserver((ds) => {
      const w = Math.floor(ds[0].contentRect.width);
      if (w > 0 && w !== cu) { cu = w; xoa(el).append(ve(w, cao)); }
    }).observe(el);
    return el;
  }
  // Mốc trục tròn số: 0, 1, 2 / 0, 5, 10...
  function mocTruc(lon) {
    const dinh = Math.max(lon, 1);
    const mu = Math.pow(10, Math.floor(Math.log10(dinh / 4)));
    const buoc = [1, 2, 5, 10].map((b) => b * mu).find((b) => dinh / b <= 4) || 10 * mu;
    const ds = [];
    for (let i = 0; i * buoc < dinh + buoc * 0.999; i++) ds.push(i * buoc);
    return ds;
  }
  const soGio = (ms) => ms / 3600000;
  const nhanGio = (g) => (g === 0 ? '0' : `${Math.round(g * 10) / 10}g`);
  const nhanSoGon = (n) => (n >= 1000 ? `${Math.round(n / 100) / 10}K` : String(Math.round(n * 10) / 10));
  function veLuoi(s, W, H, le, moc, y, nhan) {
    moc.forEach((m) => {
      s.append(v('line', { x1: le.t, x2: W - le.p, y1: y(m), y2: y(m), class: m === 0 ? 'truc' : 'luoi' }));
      const t = v('text', { x: le.t - 8, y: y(m) + 4, 'text-anchor': 'end' }); t.textContent = nhan(m); s.append(t);
    });
  }
  function veNhanNgay(s, ds, viTri, H, W) {
    const buoc = Math.max(1, Math.ceil(ds.length / Math.max(2, Math.floor(W / 66))));
    ds.forEach((d, i) => {
      if ((ds.length - 1 - i) % buoc) return;
      const t = v('text', { x: viTri(i), y: H - 6, 'text-anchor': 'middle' }); t.textContent = ngayNgan(d.ngay); s.append(t);
    });
  }
  // Cột theo ngày, một chuỗi số: giá trị nằm trên cột cao nhất, còn lại ở trục và ô rê chuột.
  function cotTheoNgay(ds, moTa, tuGia, nhan, dongMeo) {
    return khungVe(220, (W, H) => {
      const le = { t: 42, p: 8, tr: 20, d: 24 };
      const s = v('svg', { width: W, height: H, class: 'bieu-do', role: 'img', 'aria-label': moTa });
      const gia = ds.map(tuGia), lon = Math.max(...gia, 0.0001), moc = mocTruc(lon), dinh = moc[moc.length - 1];
      const y = (g) => H - le.d - (g / dinh) * (H - le.d - le.tr);
      veLuoi(s, W, H, le, moc, y, nhan);
      const band = (W - le.t - le.p) / ds.length, rong = Math.min(24, Math.max(3, band * 0.62));
      const viTri = (i) => le.t + band * i + band / 2;
      let daNhan = false;
      ds.forEach((d, i) => {
        const x = viTri(i) - rong / 2, y0 = y(0), yg = Math.min(y0 - (gia[i] ? 2 : 0), y(gia[i])), r = Math.min(4, rong / 2, y0 - yg);
        const cot = v('path', { d: `M${x} ${y0}V${yg + r}Q${x} ${yg} ${x + r} ${yg}H${x + rong - r}Q${x + rong} ${yg} ${x + rong} ${yg + r}V${y0}Z`, class: 'cot-so' });
        s.append(cot);
        if (!daNhan && gia[i] === lon && lon > 0.0001) {
          daNhan = true;
          const t = v('text', { x: viTri(i), y: yg - 6, 'text-anchor': 'middle', class: 'nhan-dinh' }); t.textContent = dongMeo(d)[0].gia; s.append(t);
        }
        const vung = v('rect', { x: le.t + band * i, y: le.tr, width: band, height: H - le.d - le.tr, class: 'vung', tabindex: ds.length <= 31 ? 0 : null, 'aria-label': `${ngayNgan(d.ngay)}: ${dongMeo(d).map((m) => m.gia + ' ' + m.nhan).join(', ')}` });
        const hien = (px, py) => { cot.classList.add('sang'); hienMeo(px, py, ngayDu(d.ngay), dongMeo(d)); };
        vung.addEventListener('pointermove', (e) => hien(e.clientX, e.clientY));
        vung.addEventListener('focus', () => { const b = vung.getBoundingClientRect(); hien(b.left + b.width / 2, b.top + 30); });
        const an = () => { cot.classList.remove('sang'); anMeo(); };
        vung.addEventListener('pointerleave', an);
        vung.addEventListener('blur', an);
        s.append(vung);
      });
      veNhanNgay(s, ds, viTri, H, W);
      return s;
    });
  }
  // Đường theo ngày. Một chuỗi: có lớp nền mờ và số ở cuối. Nhiều chuỗi: chung một trục, cùng đơn vị.
  function duongTheoNgay(ds, chuoi, moTa, nhan, cao) {
    return khungVe(cao || 220, (W, H) => {
      const le = { t: 42, p: chuoi.length === 1 ? 44 : 14, tr: 14, d: 24 };
      const s = v('svg', { width: W, height: H, class: 'bieu-do', role: 'img', 'aria-label': moTa });
      const lon = Math.max(0.0001, ...chuoi.flatMap((c) => ds.map(c.gia))), moc = mocTruc(lon), dinh = moc[moc.length - 1];
      const y = (g) => H - le.d - (g / dinh) * (H - le.d - le.tr);
      const x = (i) => le.t + (ds.length > 1 ? (i / (ds.length - 1)) * (W - le.t - le.p) : 0);
      veLuoi(s, W, H, le, moc, y, nhan);
      chuoi.forEach((c) => {
        const net = ds.map((d, i) => `${i ? 'L' : 'M'}${x(i)} ${y(c.gia(d))}`).join('');
        if (chuoi.length === 1) s.append(v('path', { d: `${net}L${x(ds.length - 1)} ${y(0)}L${x(0)} ${y(0)}Z`, class: 'vung-mo ' + c.mau }));
        s.append(v('path', { d: net, class: 'duong ' + c.mau }));
        const cuoi = ds.length - 1;
        s.append(v('circle', { cx: x(cuoi), cy: y(c.gia(ds[cuoi])), r: 4, class: 'cham-cuoi ' + c.mau }));
        if (chuoi.length === 1) { const t = v('text', { x: x(cuoi) + 9, y: y(c.gia(ds[cuoi])) + 4, class: 'nhan-dinh' }); t.textContent = nhan(c.gia(ds[cuoi])); s.append(t); }
      });
      veNhanNgay(s, ds, x, H, W);
      const dau = v('line', { y1: le.tr, y2: H - le.d, class: 'truc-doc' });
      dau.classList.add('hidden');
      const diem = chuoi.map((c) => v('circle', { r: 4, class: 'cham-cuoi hidden ' + c.mau }));
      s.append(dau, ...diem);
      const chon = (i, px, py) => {
        dau.setAttribute('x1', x(i)); dau.setAttribute('x2', x(i)); dau.classList.remove('hidden');
        chuoi.forEach((c, k) => { diem[k].setAttribute('cx', x(i)); diem[k].setAttribute('cy', y(c.gia(ds[i]))); diem[k].classList.remove('hidden'); });
        hienMeo(px, py, ngayDu(ds[i].ngay), chuoi.map((c) => ({ gia: c.dinhDang(c.gia(ds[i])), nhan: c.ten, mau: c.mau })));
      };
      const pham = v('rect', { x: le.t, y: le.tr, width: W - le.t - le.p, height: H - le.d - le.tr, class: 'vung', tabindex: 0, 'aria-label': `${moTa}. Dùng phím mũi tên để xem từng ngày.` });
      let dangChon = ds.length - 1;
      pham.addEventListener('pointermove', (e) => {
        const b = pham.getBoundingClientRect();
        dangChon = Math.max(0, Math.min(ds.length - 1, Math.round(((e.clientX - b.left) / b.width) * (ds.length - 1))));
        chon(dangChon, e.clientX, e.clientY);
      });
      const an = () => { dau.classList.add('hidden'); diem.forEach((c) => c.classList.add('hidden')); anMeo(); };
      const banPhim = () => { const b = pham.getBoundingClientRect(); chon(dangChon, b.left + (dangChon / Math.max(1, ds.length - 1)) * b.width, b.top + 20); };
      pham.addEventListener('pointerleave', an);
      pham.addEventListener('blur', an);
      pham.addEventListener('focus', banPhim);
      pham.addEventListener('keydown', (e) => {
        if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
        e.preventDefault();
        dangChon = Math.max(0, Math.min(ds.length - 1, dangChon + (e.key === 'ArrowLeft' ? -1 : 1)));
        banPhim();
      });
      s.append(pham);
      return s;
    });
  }
  // Thanh chồng 100%: phần của một tổng. Chú thích luôn có, kèm số và phần trăm (chữ màu thường, ô màu đứng cạnh).
  function thanhChong(muc, moTa) {
    const tong = muc.reduce((a, m) => a + m.gia, 0);
    if (!tong) return trong('Chưa có số liệu', '');
    const phanTram = (g) => { const p = (g / tong) * 100; return `${p > 0 && p < 1 ? '< 1' : Math.round(p)}%`; };
    const thanh = h('div', { lop: 'thanh-chong', role: 'img', 'aria-label': `${moTa}: ${muc.map((m) => `${m.ten} ${phanTram(m.gia)}`).join(', ')}` });
    muc.filter((m) => m.gia > 0).forEach((m) => {
      const o = h('span', { lop: 'doan ' + m.mau, tabindex: 0, 'aria-label': `${m.ten}: ${m.chu}, ${phanTram(m.gia)}` });
      o.style.flexGrow = m.gia;
      const hien = (x, y) => hienMeo(x, y, m.ten, [{ gia: m.chu, nhan: phanTram(m.gia), mau: m.mau }]);
      o.addEventListener('pointermove', (e) => hien(e.clientX, e.clientY));
      o.addEventListener('focus', () => { const b = o.getBoundingClientRect(); hien(b.left + b.width / 2, b.top); });
      o.addEventListener('pointerleave', anMeo);
      o.addEventListener('blur', anMeo);
      thanh.append(o);
    });
    return h('div', { lop: 'khoi-chong' }, thanh, h('ul', { lop: 'chu-thich' }, muc.map((m) => h('li', null, h('i', { lop: 'o-mau ' + m.mau }), h('span', { lop: 'ten-ct' }, m.ten), h('b', null, m.chu), h('span', { lop: 'mo so-mono' }, phanTram(m.gia))))));
  }
  const chongTheoNen = (ds) => thanhChong(['shopee', 'facebook', 'tiktok', 'google', 'khac'].map((n) => { const r = ds.find((x) => x.nen_tang === n) || { ms: 0 }; return { ten: TEN_NEN[n], gia: r.ms, chu: thoiLuong(r.ms), mau: MAU_NEN[n] }; }).filter((m) => m.gia > 0), 'Giờ dùng theo nền tảng');
  // Bản đồ nhiệt thứ × giờ, cộng dồn trong khoảng đã chọn: một màu, đậm dần theo số giờ dùng.
  const TEN_THU = ['Thứ 2', 'Thứ 3', 'Thứ 4', 'Thứ 5', 'Thứ 6', 'Thứ 7', 'CN'];
  function banDoGio(g) {
    const lon = Math.max(0, ...g.o.flat());
    if (!lon) return trong('Chưa có số liệu', 'Không có lượt dùng nào trong khoảng này.');
    const bac = (ms) => (!ms ? 0 : Math.min(5, Math.ceil((ms / lon) * 5)));
    const ve = khungVe(7 * 24 + 26, (W, H) => {
      const le = 44, tren = 20, cW = (W - le) / 24, cH = 24;
      const s = v('svg', { width: W, height: H, class: 'bieu-do', role: 'img', 'aria-label': 'Giờ dùng profile theo thứ trong tuần và giờ trong ngày' });
      for (let i = 0; i < 24; i += 3) { const t = v('text', { x: le + i * cW + cW / 2, y: 12, 'text-anchor': 'middle' }); t.textContent = `${i}h`; s.append(t); }
      g.o.forEach((hang, d) => {
        const t = v('text', { x: 0, y: tren + d * cH + 16 }); t.textContent = TEN_THU[d]; s.append(t);
        hang.forEach((ms, i) => {
          const o = v('rect', { x: le + i * cW + 1, y: tren + d * cH + 1, width: Math.max(1, cW - 2), height: cH - 2, rx: 3, class: 'o-nhiet b' + bac(ms) });
          o.addEventListener('pointermove', (e) => hienMeo(e.clientX, e.clientY, `${TEN_THU[d]}, ${i}h–${i + 1}h`, [{ gia: ms ? thoiLuong(ms) : 'Không dùng', nhan: ms ? 'giờ dùng profile' : '' }]));
          o.addEventListener('pointerleave', anMeo);
          s.append(o);
        });
      });
      return s;
    });
    return h('div', null, ve, h('div', { lop: 'thang-mau' }, 'Ít', [1, 2, 3, 4, 5].map((b) => h('i', { lop: 'o-thang b' + b })), 'Nhiều'));
  }
  // Mọi số trên biểu đồ cũng đọc được không cần rê chuột.
  const xemBang = (cot, hang, dong) => h('details', { lop: 'xem-bang' }, h('summary', null, 'Xem dạng bảng'), bang(cot, hang, dong));

  // ----- Tổng quan -----
  function trangTongQuan(goc) {
    const dai = h('div', { lop: 'dai-so' });
    const dangMo = h('div'), tuong = h('div'), hoatDong = h('div'), chuY = h('div');
    const bdXuHuong = h('div'), bdNen = h('div'), bdMay = h('div'), phuSong = h('div');
    // Số máy đã kích hoạt key, lấy từ kho key: hỏi một lần khi mở trang, không theo nhịp làm mới.
    let daKichHoat = null;
    layHetKey().then((ds) => { daKichHoat = ds.reduce((n, k) => n + (k.kich_hoat || []).length, 0); veMoi(); }).catch(() => {});
    let veMoi = () => {};
    const demMo = h('span', { lop: 'nhan cam' }, '0'), demMay = h('span', { lop: 'mo' });
    const capNhat = h('span', { lop: 'nhan' }, h('span', { lop: 'cham song' }), 'Đang nhận dữ liệu');
    goc.append(dauTrang('Tổng quan', 'Toàn bộ máy cài Hir-Login của công ty, gộp về một chỗ.', [capNhat, h('button', { lop: 'nut chinh-nut', bam: () => moTaoKey(() => { dieuHuong('key'); }) }, bieu('cong'), 'Tạo key')]),
      phuSong,
      dai,
      h('div', { lop: 'luoi-2 co-duoi' },
        h('section', { lop: 'khoi' }, h('div', { lop: 'dau-khoi' }, h('h2', null, 'Giờ dùng profile, 14 ngày qua')), h('div', { lop: 'than-khoi' }, bdXuHuong)),
        h('div', { lop: 'cot' },
          h('section', { lop: 'khoi' }, h('div', { lop: 'dau-khoi' }, h('h2', null, 'Theo nền tảng, tháng này')), h('div', { lop: 'than-khoi' }, bdNen)),
          h('section', { lop: 'khoi' }, h('div', { lop: 'dau-khoi' }, h('h2', null, 'Tình trạng thiết bị')), h('div', { lop: 'than-khoi' }, bdMay)))),
      h('div', { lop: 'luoi-2' },
        h('div', { lop: 'cot' },
          h('section', { lop: 'khoi' }, h('div', { lop: 'dau-khoi' }, h('h2', null, 'Đang sử dụng', demMo)), dangMo),
          h('section', { lop: 'khoi' }, h('div', { lop: 'dau-khoi' }, h('h2', null, 'Thiết bị', demMay), h('button', { lop: 'lien-ket', bam: () => dieuHuong('thietBi') }, 'Chi tiết →')), tuong)),
        h('div', { lop: 'cot' },
          chuY,
          h('section', { lop: 'khoi' }, h('div', { lop: 'dau-khoi' }, h('h2', null, 'Hoạt động gần đây'), h('button', { lop: 'lien-ket', bam: () => dieuHuong('lichSu') }, 'Lịch sử →')), hoatDong))));
    const soSanh = (nay, truoc, khongCo) => {
      if (!truoc) return khongCo;
      const pt = Math.round(((nay - truoc) / truoc) * 100);
      return h('span', null, h('span', { lop: pt > 0 ? 'tang' : pt < 0 ? 'giam' : '' }, `${pt > 0 ? '↑' : pt < 0 ? '↓' : ''} ${Math.abs(pt)}%`), ' so với tháng trước');
    };
    const oSo = (nhan, giaTri, ghiChu, noi) => h('div', { lop: 'o-so' + (noi ? ' noi' : '') }, h('div', { lop: 'nhan-so' }, nhan), h('div', { lop: 'so' }, giaTri), h('div', { lop: 'ghi' }, ghiChu));
    return {
      async nap() {
        const [q, tb] = await Promise.all([doc('tongQuan'), doc('thietBi')]);
        const s = q.so;
        // Máy cài bản cũ chưa gửi báo cáo: nói rõ, để số 0 không bị hiểu là không có ai dùng.
        veMoi = () => {
          const baoCao = tb.hang.filter((m) => m.key.startsWith('HLG-')).length;
          xoa(phuSong);
          if (daKichHoat != null && baoCao < daKichHoat) phuSong.append(h('div', { lop: 'bao-phu' }, h('b', null, `Mới có ${so(baoCao)} trên ${so(daKichHoat)} máy đã kích hoạt key gửi báo cáo.`), ' Các số bên dưới chỉ tính những máy này. Máy khác sẽ xuất hiện sau khi cập nhật Hir-Login lên bản 0.2.43 trở lên.'));
        };
        veMoi();
        xoa(dai).append(
          oSo('Tổng profile', so(s.tong_profile), soSanh(s.tong_profile, s.tong_profile_truoc, 'tổng trên các máy')),
          oSo('Đang sử dụng', so(s.dang_su_dung), s.tong_profile ? `${(Math.round((s.dang_su_dung / s.tong_profile) * 1000) / 10).toLocaleString('vi-VN')}% tổng profile` : 'không có profile', true),
          oSo('Cộng sự', so(s.nhan_su), s.nhan_su_moi ? h('span', { lop: 'tang' }, `${so(s.nhan_su_moi)} mới tháng này`) : 'không có người mới'),
          oSo('Thiết bị', [so(s.truc_tuyen), h('span', { lop: 'tren' }, ` / ${so(s.thiet_bi)}`)], s.thiet_bi ? 'đang hoạt động' : 'chưa có máy'),
          oSo('Tổng lượt sử dụng', so(s.luot), soSanh(s.luot, s.luot_truoc, 'tháng này')));
        xoa(bdXuHuong).append(q.xu_huong.some((d) => d.ms)
          ? duongTheoNgay(q.xu_huong, [{ ten: 'giờ dùng', gia: (d) => soGio(d.ms), mau: 'mau-cam', dinhDang: (g) => thoiLuong(g * 3600000) }], 'Giờ dùng profile 14 ngày qua', nhanGio, 316)
          : trong('Chưa có số liệu', 'Biểu đồ có sau khi cộng sự mở profile.'));
        xoa(bdNen).append(chongTheoNen(q.theo_nen));
        xoa(bdMay).append(thanhChong([
          { ten: 'Đang mở profile', gia: q.may_tt.dang_mo, chu: so(q.may_tt.dang_mo), mau: 'mau-cam' },
          { ten: 'Bật, đang rảnh', gia: Math.max(0, q.may_tt.bat - q.may_tt.dang_mo), chu: so(Math.max(0, q.may_tt.bat - q.may_tt.dang_mo)), mau: 'mau-ok' },
          { ten: 'Đã tắt', gia: q.may_tt.tat, chu: so(q.may_tt.tat), mau: 'mau-xam' },
        ], 'Tình trạng thiết bị'));
        demMo.textContent = so(q.dang_dung.length);
        demMay.textContent = s.tu_dong ? `${so(s.truc_tuyen)} bật · ${so(s.tu_dong)} automation đang chạy` : `${so(s.truc_tuyen)} bật`;
        xoa(dangMo).append(q.dang_dung.length
          ? bang(['Profile', 'Người dùng', 'Máy', { t: 'Đã mở', lop: 'phai' }], q.dang_dung, (d) => [profileChip(d.ten, d.thu_muc, d.nen_tang), nguoiChip(d.nhan_su), h('span', { lop: 'phu' }, d.ten_may), h('td', { lop: 'phai so-mono' }, thoiLuong(Date.now() - d.tu_luc))], (d) => moChiTietProfile(d.id, d.ten))
          : trong('Không có profile nào đang mở', 'Khi cộng sự mở profile, nó hiện ở đây sau ít giây.'));
        xoa(tuong).append(tb.hang.length
          ? h('div', { lop: 'tuong' }, tb.hang.map((m) => h('button', { lop: 'o-may' + (m.dang_mo ? ' dang-lam' : '') + (m.truc_tuyen ? '' : ' tat'), bam: () => moChiTietMay(m.ma_may) },
            h('span', { lop: 'ten' }, h('span', { lop: 'cham ' + (m.truc_tuyen ? (m.dang_mo ? 'mo-profile' : 'song') : '') }), h('span', null, m.ten_may || 'Máy')),
            h('span', { lop: 'ai' }, m.nhan_su),
            h('span', { lop: 'tt' }, m.truc_tuyen ? (m.dang_mo ? `${m.dang_mo} profile đang mở` : 'Đang rảnh') : truoc(m.lan_cuoi)))))
          : trong('Chưa có máy nào báo cáo', 'Máy tự xuất hiện sau khi cập nhật Hir-Login lên bản có báo cáo.'));
        xoa(hoatDong).append(dongThoiGian(q.hoat_dong));
        xoa(chuY);
        if (q.canh_bao.length) chuY.append(h('section', { lop: 'khoi' }, h('div', { lop: 'dau-khoi' }, h('h2', null, 'Cần chú ý', h('span', { lop: 'nhan loi' }, q.canh_bao.length))),
          h('ul', { lop: 'ds-chu-y' }, q.canh_bao.slice(0, 8).map((c) => h('li', null, h('span', { lop: 'cham ' + (c.muc === 'loi' ? 'loi' : 'chu-y') }), h('span', null, c.tieu_de, c.ai ? ` · ${c.ai}` : ''))))));
        capNhatNav(s.canh_bao);
      },
    };
  }

  // ----- Thiết bị -----
  function trangThietBi(goc) {
    const than = h('div', { lop: 'khoi' });
    let tim = '', tt = '';
    goc.append(dauTrang('Thiết bị', 'Mỗi máy đã cài và kích hoạt Hir-Login.'), h('div', { lop: 'loc' }, oTim((v) => { tim = v; nap(); }, 'Tìm máy, người dùng…'), tabNho([['', 'Tất cả'], ['bat', 'Đang bật'], ['tat', 'Đã tắt'], ['cu', 'Bản cũ']], '', (v) => { tt = v; nap(); })), than);
    async function nap() {
      const d = await doc('thietBi');
      const s = tim.toLowerCase();
      const hang = d.hang.filter((m) => (!s || `${m.ten_may} ${m.nhan_su} ${m.may_chu}`.toLowerCase().includes(s)) && (!tt || (tt === 'bat' && m.truc_tuyen) || (tt === 'tat' && !m.truc_tuyen) || (tt === 'cu' && m.ban_cu)));
      xoa(than).append(hang.length
        ? bang(['Máy', 'Người dùng', 'Tình trạng', { t: 'Profile', lop: 'phai' }, 'Automation', 'Phiên bản', 'Máy chủ team', 'Báo cáo cuối'], hang, (m) => [
          h('div', { lop: 'chip-profile' }, h('span', { lop: 'cham ' + (m.truc_tuyen ? (m.dang_mo ? 'mo-profile' : 'song') : '') }), h('div', null, h('div', { lop: 'dong-chinh cat' }, m.ten_may || 'Máy'), h('div', { lop: 'dong-phu' }, m.he_dieu_hanh))),
          h('div', null, h('div', { lop: 'cat' }, m.nhan_su), m.key.startsWith('HIR-') ? nhanTrang('Key cũ', 'chu-y') : null),
          m.key_loi ? nhanTrang(m.key_loi === 'BI_KHOA' ? 'Key bị khoá' : 'Key hết hạn', 'loi') : m.truc_tuyen ? (m.dang_mo ? nhanTrang(`${m.dang_mo} đang mở`, 'cam') : nhanTrang('Đang rảnh', 'ok')) : nhanTrang('Đã tắt'),
          h('td', { lop: 'phai so-mono' }, so(m.tong_profile)),
          m.tu_dong.length ? nhanTrang(`${m.tu_dong.length} đang chạy`, 'tin') : h('span', { lop: 'mo' }, '—'),
          h('span', { lop: 'so-mono' }, m.phien_ban, m.ban_cu ? [' ', nhanTrang('cũ', 'chu-y')] : ''),
          h('span', { lop: 'phu mono cat' }, m.may_chu || '—'),
          h('span', { lop: 'phu' }, m.truc_tuyen ? truoc(m.lan_cuoi) : ngayGio(m.lan_cuoi)),
        ], (m) => moChiTietMay(m.ma_may))
        : trong('Không có máy nào', tim || tt ? 'Thử bỏ bớt điều kiện lọc.' : 'Máy tự xuất hiện sau khi cập nhật Hir-Login.'));
    }
    return { nap };
  }

  async function moChiTietMay(maMay) {
    const d = await doc('chiTietMay', { ma_may: maMay });
    if (!d) return bao('Máy không còn trong danh sách', true);
    const m = d.may;
    const coChon = dongLop;
    if (coChon) coChon();
    moNgan(m.ten_may || 'Máy', m.nhan_su, [
      h('div', { lop: 'muc-ngan' }, h('h4', null, 'Thông tin'), h('dl', { lop: 'cap' },
        h('dt', null, 'Tình trạng'), h('dd', null, m.truc_tuyen ? nhanTrang('Đang bật', 'ok') : nhanTrang('Đã tắt')),
        h('dt', null, 'Báo cáo cuối'), h('dd', null, ngayGio(m.lan_cuoi)),
        h('dt', null, 'Lần đầu'), h('dd', null, ngayGio(m.lan_dau)),
        h('dt', null, 'Phiên bản'), h('dd', null, `${m.phien_ban} · ${m.he_dieu_hanh}`),
        h('dt', null, 'Tổng profile'), h('dd', null, so(m.tong_profile)),
        h('dt', null, 'Chờ tải lên'), h('dd', null, so(m.cho_tai_len)),
        h('dt', null, 'Máy chủ team'), h('dd', { lop: 'mono' }, m.may_chu || 'Không nối'),
        h('dt', null, 'Địa chỉ IP'), h('dd', { lop: 'mono' }, m.ip || '—'),
        h('dt', null, 'Key'), h('dd', { lop: 'mono' }, m.key, m.key.startsWith('HIR-') ? ' (key cũ, chưa xác thực được)' : ''))),
      h('div', { lop: 'muc-ngan' }, h('h4', null, `Đang mở (${d.dang_mo.length})`),
        d.dang_mo.length ? d.dang_mo.map((p) => h('div', { lop: 'dong-gon' }, h('span', { lop: 'cat' }, p.ten), h('span', { lop: 'mo so-mono' }, thoiLuong(Date.now() - p.tu_luc)))) : h('div', { lop: 'mo' }, 'Không mở profile nào')),
      m.tu_dong.length ? h('div', { lop: 'muc-ngan' }, h('h4', null, 'Automation đang chạy'), m.tu_dong.map((a) => h('div', { lop: 'dong-gon' }, h('span', { lop: 'cat' }, a.du_an), h('span', { lop: 'mo so-mono' }, `${a.dong_ok} xong · ${a.dong_loi} lỗi`)))) : null,
      h('div', { lop: 'muc-ngan' }, h('h4', null, 'Hoạt động của máy'), dongThoiGian(d.hoat_dong)),
      h('button', { lop: 'nut nguy', bam: async () => {
        if (!(await xacNhan('Xoá máy khỏi danh sách?', `${m.ten_may}: lịch sử vẫn được giữ. Nếu máy còn dùng Hir-Login, nó sẽ tự xuất hiện lại ở lần báo cáo sau.`, 'Xoá máy', true))) return;
        await ghi('xoaMay', { ma_may: maMay }); bao('Đã xoá máy'); napLai();
      } }, 'Xoá khỏi danh sách'),
    ]);
  }

  // ----- Cộng sự -----
  function trangNhanSu(goc) {
    const than = h('div', { lop: 'khoi' });
    let tim = '';
    goc.append(dauTrang('Cộng sự', 'Tính theo key. Tên lấy từ thông tin key.'), h('div', { lop: 'loc' }, oTim((v) => { tim = v; nap(); }, 'Tìm tên, key…')), than);
    async function nap() {
      const d = await doc('nhanSu');
      const s = tim.toLowerCase();
      const hang = d.hang.filter((n) => !s || `${n.nhan_su} ${n.key}`.toLowerCase().includes(s));
      xoa(than).append(hang.length
        ? bang(['Cộng sự', 'Máy', { t: 'Đang mở', lop: 'phai' }, { t: 'Giờ dùng hôm nay', lop: 'phai' }, { t: 'Lượt hôm nay', lop: 'phai' }, { t: 'Tổng profile', lop: 'phai' }, 'Hoạt động cuối'], hang, (n) => [
          nguoiChip(n.nhan_su, h('span', { lop: 'mono' }, n.key)),
          h('span', null, h('span', { lop: 'so-mono' }, `${n.truc_tuyen}/${n.may.length}`), h('span', { lop: 'mo' }, ' đang bật')),
          h('td', { lop: 'phai so-mono' }, n.dang_mo ? nhanTrang(n.dang_mo, 'cam') : '0'),
          h('td', { lop: 'phai so-mono' }, n.ms_hom_nay ? thoiLuong(n.ms_hom_nay) : '—'),
          h('td', { lop: 'phai so-mono' }, so(n.luot_hom_nay)),
          h('td', { lop: 'phai so-mono' }, so(n.tong_profile)),
          h('span', { lop: 'phu' }, n.truc_tuyen ? 'Đang online' : truoc(n.lan_cuoi)),
        ], (n) => { if (n.may[0]) moChiTietMay(n.may[0].ma_may); })
        : trong('Chưa có cộng sự', tim ? 'Không tìm thấy.' : 'Cộng sự xuất hiện khi máy của họ bắt đầu báo cáo.'));
    }
    return { nap };
  }

  async function moChiTietProfile(id, ten) {
    const d = await doc('chiTietProfile', { id });
    moNgan(ten || 'Profile', id, [
      h('div', { lop: 'muc-ngan' }, h('h4', null, 'Các lần mở gần đây'), d.lich_su.length ? d.lich_su.map((p) => h('div', { lop: 'dong-gon' }, h('span', { lop: 'cat' }, `${p.nhan_su} · ${p.ten_may}`), h('span', { lop: 'mo so-mono' }, `${ngayGio(p.bat_dau)} · ${p.ket_thuc ? thoiLuong(p.ket_thuc - p.bat_dau) : 'đang mở'}`))) : h('div', { lop: 'mo' }, 'Chưa có')),
    ]);
  }

  // ----- Lịch sử -----
  function trangLichSu(goc) {
    const than = h('div', { lop: 'khoi' });
    const loc = { viec: '', tim: '', trang: 1 };
    let luot = 0;
    const chonViec = h('select', { lop: 'o', 'aria-label': 'Hành động', onchange: (e) => { loc.viec = e.target.value; loc.trang = 1; nap(); } }, h('option', { value: '' }, 'Mọi hành động'));
    let biet = new Set();
    goc.append(dauTrang('Lịch sử sử dụng', 'Ai mở profile nào, lúc nào, trên máy nào.', h('button', { lop: 'nut', bam: xuat }, bieu('tai'), 'Xuất Excel')),
      boLocNgay(() => { loc.trang = 1; than.classList.add('dang-tai'); nap().catch((e) => bao(e.message, true)); }),
      h('div', { lop: 'loc' }, oTim((v) => { loc.tim = v; loc.trang = 1; nap(); }, 'Tìm người, profile, máy…'), chonViec), than);
    async function nap() {
      const me = ++luot;
      const d = await doc('lichSu', { ...loc, ...khoangHien() });
      if (me !== luot) return;
      than.classList.remove('dang-tai');
      d.viec.forEach((v) => { if (!biet.has(v)) { biet.add(v); chonViec.append(h('option', { value: v }, v)); } });
      them(xoa(than), d.hang.length
        ? [bang(['Thời gian', 'Cộng sự', 'Hành động', 'Đối tượng', 'Máy'], d.hang, (n) => [
          h('span', { lop: 'so-mono phu' }, ngayGio(n.luc)), h('span', { lop: 'dong-chinh' }, n.nhan_su || 'Hệ thống'), n.viec,
          h('span', { lop: 'cat phu', title: n.doi_tuong }, n.doi_tuong || '—'), h('span', { lop: 'phu' }, n.ten_may || '—'),
        ]), phanTrang(d.trang, d.moi, d.tong, (t) => { loc.trang = t; nap(); })]
        : trong('Không có dòng nào', 'Thử chọn khoảng ngày khác.'));
    }
    async function xuat() {
      const d = await doc('lichSu', { ...loc, ...khoangHien(), trang: 1, moi: 5000 });
      const o = (s) => `"${String(s == null ? '' : s).replace(/"/g, '""')}"`;
      const dong = [['Thời gian', 'Cộng sự', 'Hành động', 'Đối tượng', 'Folder', 'Máy'].map(o).join(',')].concat(d.hang.map((n) => [ngayDu(n.luc) + ' ' + gio(n.luc), n.nhan_su, n.viec, n.doi_tuong, n.chi_tiet, n.ten_may].map((x) => o(/^[=+\-@]/.test(String(x)) ? "'" + x : x)).join(',')));
      const a = h('a', { href: URL.createObjectURL(new Blob(['﻿' + dong.join('\r\n')], { type: 'text/csv;charset=utf-8' })), download: `lich-su-hirlogin-${Date.now()}.csv` });
      a.click();
      setTimeout(() => URL.revokeObjectURL(a.href), 5000);
    }
    return { nap };
  }

  // ----- Báo cáo -----
  function trangBaoCao(goc) {
    const than = h('div', { lop: 'cot' });
    let luot0 = 0;
    goc.append(dauTrang('Báo cáo', 'Thời gian dùng profile, tính từ các phiên mở và đóng.'),
      boLocNgay(() => { than.classList.add('dang-tai'); nap().catch((e) => bao(e.message, true)); }), than);
    async function nap() {
      const me = ++luot0;
      const d = await doc('baoCaoSo', khoangHien());
      if (me !== luot0) return;
      than.classList.remove('dang-tai');
      const tong = d.theo_ngay.reduce((a, x) => a + x.ms, 0), luot = d.theo_ngay.reduce((a, x) => a + x.luot, 0);
      const lon = Math.max(1, ...d.theo_nguoi.map((n) => n.ms));
      xoa(than).append(
        d.tu < Date.now() - d.giu_ngay * 86400000 ? h('div', { lop: 'ghi-khoang' }, `Lịch sử chỉ giữ ${d.giu_ngay} ngày nên phần cũ hơn không có số liệu. Đổi được trong Cài đặt.`) : '',
        h('div', { lop: 'dai-so' }, [['Tổng giờ dùng', thoiLuong(tong), `trong ${so(d.so_ngay)} ngày`], ['Lượt mở', so(luot), 'tổng cộng'], ['Cộng sự có dùng', so(d.theo_nguoi.length), 'có ít nhất 1 lượt'], ['Profile có dùng', so(d.so_profile), 'có ít nhất 1 lượt'], ['TB mỗi ngày', thoiLuong(tong / d.so_ngay), 'giờ dùng']].map(([a, b, c]) => h('div', { lop: 'o-so' }, h('div', { lop: 'nhan-so' }, a), h('div', { lop: 'so' }, b), h('div', { lop: 'ghi' }, c)))),
        h('section', { lop: 'khoi' }, h('div', { lop: 'dau-khoi' }, h('h2', null, 'Giờ dùng profile theo ngày')), h('div', { lop: 'than-khoi' },
          !tong ? trong('Chưa có số liệu', 'Không có lượt dùng nào trong khoảng này.') : cotTheoNgay(d.theo_ngay, 'Giờ dùng profile theo ngày', (x) => soGio(x.ms), nhanGio, (x) => [{ gia: thoiLuong(x.ms), nhan: 'giờ dùng' }, { gia: so(x.luot), nhan: 'lượt mở' }, { gia: so(x.nguoi), nhan: 'cộng sự' }]),
          xemBang(['Ngày', { t: 'Giờ dùng', lop: 'phai' }, { t: 'Lượt', lop: 'phai' }, { t: 'Cộng sự', lop: 'phai' }, { t: 'Profile', lop: 'phai' }], [...d.theo_ngay].reverse(), (x) => [ngayDu(x.ngay), h('td', { lop: 'phai so-mono' }, thoiLuong(x.ms)), h('td', { lop: 'phai so-mono' }, so(x.luot)), h('td', { lop: 'phai so-mono' }, so(x.nguoi)), h('td', { lop: 'phai so-mono' }, so(x.profile))]))),
        h('div', { lop: 'luoi-2' },
          h('section', { lop: 'khoi' }, h('div', { lop: 'dau-khoi' }, h('h2', null, 'Giờ nào dùng nhiều, theo thứ trong tuần')), h('div', { lop: 'than-khoi' }, banDoGio(d.theo_gio))),
          h('section', { lop: 'khoi' }, h('div', { lop: 'dau-khoi' }, h('h2', null, 'Theo nền tảng')), h('div', { lop: 'than-khoi' }, chongTheoNen(d.theo_nen)))),
        h('section', { lop: 'khoi' }, h('div', { lop: 'dau-khoi' }, h('h2', null, 'Cao nhất mỗi ngày: máy bật và profile mở cùng lúc')), h('div', { lop: 'than-khoi' },
          d.dinh.length >= 2
            ? [h('ul', { lop: 'chu-thich ngang' }, h('li', null, h('i', { lop: 'o-mau mau-1' }), h('span', { lop: 'ten-ct' }, 'Profile mở cùng lúc')), h('li', null, h('i', { lop: 'o-mau mau-2' }), h('span', { lop: 'ten-ct' }, 'Máy bật'))),
              duongTheoNgay(d.dinh, [{ ten: 'profile mở cùng lúc', gia: (x) => x.dang_mo, mau: 'mau-1', dinhDang: so }, { ten: 'máy bật', gia: (x) => x.may_bat, mau: 'mau-2', dinhDang: so }], 'Số máy bật và số profile mở cùng lúc cao nhất mỗi ngày', nhanSoGon),
              xemBang(['Ngày', { t: 'Profile mở', lop: 'phai' }, { t: 'Máy bật', lop: 'phai' }], [...d.dinh].reverse(), (x) => [ngayDu(x.ngay), h('td', { lop: 'phai so-mono' }, so(x.dang_mo)), h('td', { lop: 'phai so-mono' }, so(x.may_bat))])]
            : trong('Chưa đủ số liệu', 'Cần ít nhất hai ngày có số liệu trong khoảng này.'))),
        h('div', { lop: 'luoi-2' },
          h('section', { lop: 'khoi' }, h('div', { lop: 'dau-khoi' }, h('h2', null, 'Theo cộng sự')), d.theo_nguoi.length ? bang(['Cộng sự', { t: 'Lượt', lop: 'phai' }, { t: 'Profile', lop: 'phai' }, { t: 'Giờ dùng', lop: 'phai' }], d.theo_nguoi, (n) => {
            const thanh = h('i'); thanh.style.width = `${Math.round((n.ms / lon) * 100)}%`;
            return [h('div', null, h('div', { lop: 'dong-chinh cat' }, n.nhan_su), h('div', { lop: 'thanh' }, thanh)), h('td', { lop: 'phai so-mono' }, so(n.luot)), h('td', { lop: 'phai so-mono' }, so(n.so_profile)), h('td', { lop: 'phai so-mono' }, thoiLuong(n.ms))];
          }) : trong('Chưa có số liệu', '')),
          h('section', { lop: 'khoi' }, h('div', { lop: 'dau-khoi' }, h('h2', null, 'Profile dùng nhiều')), d.theo_profile.length ? bang(['Profile', { t: 'Giờ dùng', lop: 'phai' }], d.theo_profile, (p) => [profileChip(p.ten, `${p.so_nguoi} người · ${p.luot} lượt`, p.nen_tang), h('td', { lop: 'phai so-mono' }, thoiLuong(p.ms))]) : trong('Chưa có số liệu', ''))));
    }
    return { nap, thua: true };
  }

  // ----- Key -----
  function moTaoKey(sauKhiXong) {
    const ghiChu = h('input', { lop: 'o', maxlength: 120, placeholder: 'Ví dụ: Nguyễn An, team Shopee' });
    const soNgay = h('input', { lop: 'o', type: 'number', min: 0, max: 3650, giaTri: 30 });
    const soMay = h('input', { lop: 'o', type: 'number', min: 1, max: 50, giaTri: 1 });
    const soLuong = h('input', { lop: 'o', type: 'number', min: 1, max: 50, giaTri: 1 });
    const loi = h('div', { lop: 'loi-hop' });
    const tao = h('button', { lop: 'nut chinh-nut', bam: async () => {
      tao.disabled = true;
      try {
        const r = await key({ viec: 'tao', ghi_chu: ghiChu.value.trim(), so_ngay: Number(soNgay.value), so_may: Number(soMay.value), so_luong: Number(soLuong.value) });
        const ds = r.hang.map((k) => k.key);
        if (dongLop) dongLop();
        moHop([h('h3', null, `Đã tạo ${ds.length} key`), h('p', null, 'Chép và gửi cho cộng sự. Key cũng có trong danh sách.'),
          h('div', { lop: 'ket-qua-key mono' }, ds.map((k) => h('div', null, k))),
          h('div', { lop: 'chan-hop' }, h('button', { lop: 'nut', bam: () => chep(ds.join('\n')) }, bieu('sao'), 'Chép'), h('button', { lop: 'nut chinh-nut', bam: () => { if (dongLop) dongLop(); } }, 'Xong'))]);
        if (sauKhiXong) sauKhiXong();
      } catch (e) { loi.textContent = e.message; tao.disabled = false; }
    } }, 'Tạo key');
    moHop([h('h3', null, 'Tạo key Hir-Login'), h('p', null, 'Key dạng HLG-XXXX-XXXX-XXXX, dùng chung kho với bot Telegram.'),
      truong('Ghi chú', ghiChu, 'Tên cộng sự, để dễ tra sau này.'),
      h('div', { lop: 'hai-cot' }, truong('Số ngày dùng', soNgay, '0 = vĩnh viễn'), truong('Số máy mỗi key', soMay)),
      truong('Số lượng key', soLuong), loi,
      h('div', { lop: 'chan-hop' }, h('button', { lop: 'nut', bam: () => dongLop && dongLop() }, 'Huỷ'), tao)]);
  }
  // Danh sách key HLG: lấy từng trang nhỏ cho tới hết (kho key không trả cả nghìn key một lần), nhớ 5 phút.
  let khoKey = { luc: 0, ds: null, dang: null };
  function layHetKey(epLay) {
    if (!epLay && khoKey.ds && Date.now() - khoKey.luc < 300000) return Promise.resolve(khoKey.ds);
    if (khoKey.dang) return khoKey.dang;
    khoKey.dang = (async () => {
      const gom = new Map();
      for (let off = 0; off < 20000; off += 90) {
        const r = await key({ viec: 'danhSach', off, limit: 90 });
        r.hang.forEach((x) => gom.set(x.key, x));
        if (r.het) break;
      }
      khoKey.ds = [...gom.values()];
      khoKey.luc = Date.now();
      return khoKey.ds;
    })().finally(() => { khoKey.dang = null; });
    return khoKey.dang;
  }
  const luuKey = (ds) => { khoKey.ds = ds; khoKey.luc = Date.now(); return ds; };

  // ----- Tạo key số lượng lớn: mỗi key dùng cho 1 máy -----
  const LO_KEY = 25, KEY_TOI_DA = 5000;
  function moTaoLo(sauKhiXong) {
    const chonTool = h('select', { lop: 'o', 'aria-label': 'Tool' }, h('option', { value: 'HLG' }, 'Hir-Login (HLG)'));
    key({ viec: 'sanPham' }).then((r) => { xoa(chonTool).append(...r.hang.map((s) => h('option', { value: s.tien }, `${s.ten} (${s.tien})`))); chonTool.value = 'HLG'; capNhat(); }).catch(() => {});
    const soLuong = h('input', { lop: 'o', type: 'number', inputmode: 'numeric', min: 1, max: KEY_TOI_DA, giaTri: 50, 'aria-label': 'Số lượng key', oninput: () => capNhat() });
    const nhanh = h('div', { lop: 'nhanh' }, [10, 50, 100, 200, 500, 1000].map((n) => h('button', { lop: 'nut nho', type: 'button', bam: () => { soLuong.value = n; capNhat(); } }, String(n))));
    const ghiChu = h('input', { lop: 'o', maxlength: 100, placeholder: 'Ví dụ: Team Shopee' });
    const danhSo = h('input', { type: 'checkbox', checked: true, id: 'danh-so', onchange: () => capNhat() });
    const soNgay = h('input', { lop: 'o', type: 'number', min: 0, max: 3650, giaTri: 30, oninput: () => capNhat() });
    const tomTat = h('div', { lop: 'tom-tat' }), loi = h('div', { lop: 'loi-hop' });
    const soDuoc = () => { const n = Math.floor(Number(soLuong.value)); return n >= 1 && n <= KEY_TOI_DA ? n : 0; };
    function capNhat() {
      const n = soDuoc(), ten = chonTool.selectedOptions[0] ? chonTool.selectedOptions[0].textContent : '';
      tao.disabled = !n;
      tao.textContent = n ? `Tạo ${so(n)} key` : 'Tạo key';
      tomTat.textContent = n ? `${so(n)} key ${ten}, mỗi key dùng cho 1 máy, ${Number(soNgay.value) > 0 ? `hạn ${so(Number(soNgay.value))} ngày kể từ lúc tạo` : 'không giới hạn thời gian'}.` : `Gõ số lượng từ 1 đến ${so(KEY_TOI_DA)}.`;
    }
    const tao = h('button', { lop: 'nut chinh-nut', bam: () => chay() }, 'Tạo key');
    chonTool.addEventListener('change', capNhat);
    capNhat();
    moHop([h('h3', null, 'Tạo key số lượng lớn'), h('p', null, 'Mỗi key dùng cho 1 máy. Gõ số lượng để tạo ít hay nhiều tuỳ ý.'),
      truong('Tool', chonTool),
      truong('Số lượng key', [soLuong, nhanh], `Tối đa ${so(KEY_TOI_DA)} key mỗi lần.`),
      truong('Ghi chú', ghiChu, 'Ghi vào mọi key trong đợt này, để tra sau.'),
      h('label', { lop: 'tich', for: 'danh-so' }, danhSo, 'Đánh số thứ tự sau ghi chú (001, 002, 003…)'),
      truong('Số ngày dùng', soNgay, '0 = vĩnh viễn. Tính từ lúc tạo.'),
      tomTat, loi,
      h('div', { lop: 'chan-hop' }, h('button', { lop: 'nut', bam: () => dongLop && dongLop() }, 'Huỷ'), tao)]);

    async function chay() {
      const n = soDuoc();
      if (!n) return;
      const tien = chonTool.value, ten = chonTool.selectedOptions[0].textContent, rong = Math.max(3, String(n).length);
      const mo = { ghiChu: ghiChu.value.trim(), soNgay: Number(soNgay.value), danhSo: danhSo.checked };
      const hang = [];
      let loiLo = '', dung = false;
      const thanh = h('i'), chu = h('div', { lop: 'tien-do-chu' }, '0 / ' + so(n));
      const nutDung = h('button', { lop: 'nut', bam: () => { dung = true; nutDung.disabled = true; nutDung.textContent = 'Đang dừng…'; } }, 'Dừng');
      const hop = h('div', { lop: 'hop', role: 'dialog', 'aria-modal': 'true' },
        h('h3', null, `Đang tạo ${so(n)} key ${ten}`), h('p', null, 'Giữ trang này mở cho tới khi xong. Key đã tạo vẫn được giữ nếu bạn dừng giữa chừng.'),
        h('div', { lop: 'tien-do' }, thanh), chu, h('div', { lop: 'chan-hop' }, nutDung));
      if (dongLop) dongLop();
      moLop(hop);
      lopKhoa = true;
      const truocKhiDong = (e) => { e.preventDefault(); e.returnValue = ''; };
      window.addEventListener('beforeunload', truocKhiDong);
      for (let i = 0; i < n && !dung; i += LO_KEY) {
        if (!document.body.contains(hop)) { dung = true; break; } // trang bị chuyển đi: dừng, giữ những key đã tạo
        try {
          const r = await key({ viec: 'taoLo', tien, so_luong: Math.min(LO_KEY, n - i), ghi_chu: mo.ghiChu, danh_so_tu: mo.danhSo ? i + 1 : null, rong, so_ngay: mo.soNgay });
          hang.push(...r.hang);
          if (r.loi) { loiLo = r.loi; break; }
        } catch (e) { loiLo = e.message; break; }
        thanh.style.width = `${Math.round((hang.length / n) * 100)}%`;
        chu.textContent = `${so(hang.length)} / ${so(n)}`;
      }
      window.removeEventListener('beforeunload', truocKhiDong);
      lopKhoa = false;
      const tenFile = `key-${tien.toLowerCase()}-${hang.length}-${isoNgay(Date.now())}.csv`;
      const csv = () => [['Key', 'Tool', 'Ghi chú', 'Hạn dùng'].map(oCsv).join(',')].concat(hang.map((x) => [x.key, ten, x.ghi_chu, x.het_han ? ngayDu(Date.parse(x.het_han)) : 'Vĩnh viễn'].map(oCsv).join(','))).join('\r\n');
      if (sauKhiXong) sauKhiXong();
      if (!document.body.contains(hop)) {
        if (hang.length) { taiFile(tenFile, csv()); bao(`Đã tạo ${so(hang.length)} key ${ten}, file đã tải về máy`); }
        return;
      }
      const hien = hang.slice(0, 200);
      them(xoa(hop), [
        h('h3', null, hang.length === n ? `Đã tạo ${so(n)} key ${ten}` : `Đã tạo ${so(hang.length)} / ${so(n)} key ${ten}`),
        loiLo ? h('p', { lop: 'loi-hop' }, `Dừng vì lỗi: ${loiLo}. Những key đã tạo vẫn dùng được.`) : dung ? h('p', null, 'Đã dừng theo yêu cầu. Những key đã tạo vẫn dùng được.') : h('p', null, 'Tải file về để gửi cho từng người.'),
        tien === 'HLG' ? null : h('p', { lop: 'chu-y' }, `Key ${ten} không hiện ở trang Key (trang này chỉ quản lý Hir-Login). Hãy tải file về lưu lại.`),
        hang.length ? h('div', { lop: 'ket-qua-key mono' }, hien.map((x) => h('div', null, x.key)), hang.length > hien.length ? h('div', { lop: 'mo' }, `… và ${so(hang.length - hien.length)} key nữa, có trong file.`) : null) : null,
        h('div', { lop: 'chan-hop' },
          hang.length ? h('button', { lop: 'nut', bam: () => chep(hang.map((x) => x.key).join('\n')) }, bieu('sao'), 'Chép tất cả') : null,
          hang.length ? h('button', { lop: 'nut', bam: () => taiFile(tenFile, csv()) }, bieu('tai'), 'Tải file') : null,
          h('button', { lop: 'nut chinh-nut', bam: () => dongLop && dongLop() }, 'Xong'))]);
    }
  }
  function tinhTrangKey(k) {
    if (k.khoa) return ['khoa', 'Bị khoá', 'loi'];
    if (k.het_han) {
      const con = Date.parse(k.het_han) - Date.now();
      if (con < 0) return ['het', 'Hết hạn', 'loi'];
      if (con < 7 * 86400000) return ['sap', `Còn ${Math.max(1, Math.ceil(con / 86400000))} ngày`, 'chu-y'];
    }
    return (k.kich_hoat || []).length ? ['dung', 'Đang dùng', 'ok'] : ['dung', 'Chưa kích hoạt', ''];
  }
  function trangKey(goc) {
    const than = h('div', { lop: 'khoi' }), dai = h('div', { lop: 'dai-so' }), bdKey = h('section', { lop: 'khoi co-duoi than-khoi' });
    let tim = '', loai = '', ds = [];
    goc.append(dauTrang('Quản lý key', 'Danh sách và sửa key Hir-Login (HLG-). Tạo số lượng lớn được cho mọi tool dùng chung kho key.', [h('button', { lop: 'nut', bam: () => moTaoLo(nap) }, bieu('cong'), 'Tạo số lượng lớn'), h('button', { lop: 'nut chinh-nut', bam: () => moTaoKey(nap) }, bieu('cong'), 'Tạo key')]),
      dai, bdKey, h('div', { lop: 'loc' }, oTim((v) => { tim = v; ve(); }, 'Tìm key, tên, Zalo, ghi chú…'), tabNho([['', 'Tất cả'], ['dung', 'Đang dùng'], ['sap', 'Sắp hết hạn'], ['het', 'Hết hạn'], ['khoa', 'Bị khoá']], '', (v) => { loai = v; ve(); })), than);
    function ve() {
      const s = tim.toLowerCase();
      const hang = ds.filter((k) => (!s || `${k.key} ${k.ho_ten} ${k.zalo} ${k.gmail} ${k.ghi_chu} ${(k.kich_hoat || []).map((m) => m.ten_may).join(' ')}`.toLowerCase().includes(s)) && (!loai || tinhTrangKey(k)[0] === loai)).slice(0, 300);
      const tong = ds.filter((k) => (!s || `${k.key} ${k.ho_ten} ${k.zalo} ${k.gmail} ${k.ghi_chu} ${(k.kich_hoat || []).map((m) => m.ten_may).join(' ')}`.toLowerCase().includes(s)) && (!loai || tinhTrangKey(k)[0] === loai)).length;
      them(xoa(than), hang.length
        ? [bang(['Key', 'Người dùng', 'Ghi chú', 'Hạn dùng', { t: 'Máy', lop: 'phai' }, 'Trạng thái', ''], hang, (k) => {
          const [, chu, mau] = tinhTrangKey(k);
          return [
            h('span', { lop: 'mono' }, k.key),
            h('div', null, h('div', { lop: 'cat' }, k.ho_ten || h('span', { lop: 'mo' }, 'Chưa khai')), k.zalo ? h('div', { lop: 'dong-phu' }, k.zalo) : null),
            h('span', { lop: 'cat phu', title: k.ghi_chu }, k.ghi_chu || '—'),
            h('span', { lop: 'phu so-mono' }, k.het_han ? ngayDu(Date.parse(k.het_han)) : 'Vĩnh viễn'),
            h('td', { lop: 'phai so-mono' }, `${(k.kich_hoat || []).length}/${k.so_may}`),
            nhanTrang(chu, mau),
            h('td', { lop: 'phai' }, h('button', { lop: 'nut icon', 'aria-label': 'Chép key', title: 'Chép key', bam: (e) => { e.stopPropagation(); chep(k.key); } }, bieu('sao'))),
          ];
        }, moKey), tong > hang.length ? h('div', { lop: 'chan-bang' }, h('span', null, `Hiển thị ${so(hang.length)} trong ${so(tong)} key. Dùng ô tìm kiếm hoặc bộ lọc để thu hẹp.`)) : null]
        : trong('Không có key nào', tim || loai ? 'Thử bỏ bớt điều kiện lọc.' : 'Bấm Tạo key để bắt đầu.'));
    }
    function moKey(k) {
      const ten = h('input', { lop: 'o', giaTri: k.ho_ten, maxlength: 80 });
      const zalo = h('input', { lop: 'o', giaTri: k.zalo, maxlength: 15 });
      const gmail = h('input', { lop: 'o', giaTri: k.gmail, maxlength: 120, type: 'email' });
      const gc = h('input', { lop: 'o', giaTri: k.ghi_chu, maxlength: 120 });
      const soMay = h('input', { lop: 'o', type: 'number', min: 1, max: 50, giaTri: k.so_may });
      const giaHan = h('input', { lop: 'o', type: 'number', min: 0, max: 3650, placeholder: 'Để trống: giữ nguyên' });
      const lam = async (viec, tb, them) => {
        try { const r = await key({ viec, key: k.key, ...(them || {}) }); bao(tb); if (r.the) ds = luuKey(ds.map((x) => (x.key === r.the.key ? r.the : x))); veTatCa(); if (dongLop) dongLop(); if (r.the) moKey(r.the); }
        catch (e) { bao(e.message, true); }
      };
      moNgan(k.key, tinhTrangKey(k)[1], [
        h('div', { lop: 'hanh-dong' },
          h('button', { lop: 'nut', bam: () => chep(k.key) }, bieu('sao'), 'Chép key'),
          k.khoa
            ? h('button', { lop: 'nut', bam: () => lam('mo', 'Đã mở khoá') }, 'Mở khoá')
            : h('button', { lop: 'nut nguy', bam: async () => { if (await xacNhan('Khoá key này?', `${k.key}: máy đang dùng sẽ bị chặn ở lần kiểm tra tiếp theo.`, 'Khoá key', true)) lam('khoa', 'Đã khoá'); } }, 'Khoá key')),
        h('div', { lop: 'muc-ngan' }, h('h4', null, 'Thông tin'),
          truong('Họ tên', ten), h('div', { lop: 'hai-cot' }, truong('Zalo', zalo), truong('Gmail', gmail)), truong('Ghi chú', gc),
          h('div', { lop: 'hai-cot' }, truong('Số máy tối đa', soMay), truong('Đặt hạn còn (ngày)', giaHan, `Hiện tại: ${k.het_han ? ngayDu(Date.parse(k.het_han)) : 'vĩnh viễn'}. 0 = vĩnh viễn`)),
          h('button', { lop: 'nut chinh-nut', bam: () => {
            const b = { thay: { ho_ten: ten.value.trim(), zalo: zalo.value.trim(), gmail: gmail.value.trim(), ghi_chu: gc.value.trim(), so_may: Number(soMay.value) } };
            if (giaHan.value !== '') b.so_ngay = Number(giaHan.value);
            lam('sua', 'Đã lưu', b);
          } }, 'Lưu thay đổi')),
        h('div', { lop: 'muc-ngan' }, h('h4', null, `Máy đã kích hoạt (${(k.kich_hoat || []).length}/${k.so_may})`),
          (k.kich_hoat || []).length ? k.kich_hoat.map((m) => h('div', { lop: 'dong-gon' }, h('span', { lop: 'cat' }, m.ten_may || m.ma_may.slice(0, 12)), h('span', { lop: 'mo' }, truoc(Date.parse(m.lan_cuoi))))) : h('div', { lop: 'mo' }, 'Chưa có máy nào'),
          (k.kich_hoat || []).length ? h('button', { lop: 'nut nho', bam: async () => { if (await xacNhan('Gỡ mọi máy khỏi key?', 'Key sẽ nhận máy mới ở lần kích hoạt tiếp theo. Máy cũ phải kích hoạt lại.', 'Gỡ máy', true)) lam('goMay', 'Đã gỡ máy'); } }, 'Gỡ mọi máy') : null),
        h('button', { lop: 'nut nguy', bam: async () => { if (await xacNhan('Xoá hẳn key này?', `${k.key} sẽ biến mất khỏi kho key, không khôi phục được. Máy đang dùng sẽ bị chặn.`, 'Xoá key', true)) { try { await key({ viec: 'xoa', key: k.key }); bao('Đã xoá key'); ds = luuKey(ds.filter((x) => x.key !== k.key)); veTatCa(); if (dongLop) dongLop(); } catch (e) { bao(e.message, true); } } } }, 'Xoá key'),
      ]);
    }
    async function nap() {
      than.classList.add('dang-tai');
      try { ds = await layHetKey(true); } finally { than.classList.remove('dang-tai'); }
      veTatCa();
    }
    function veTatCa() {
      const dem = { dung: 0, sap: 0, het: 0, khoa: 0 };
      ds.forEach((k) => { dem[tinhTrangKey(k)[0]]++; });
      const daDung = ds.filter((k) => (k.kich_hoat || []).length).length;
      xoa(dai).append(...[['Tổng key', so(ds.length), `${so(daDung)} đã kích hoạt`, true], ['Đang dùng', so(dem.dung), 'còn hạn, không khoá'], ['Sắp hết hạn', so(dem.sap), 'trong 7 ngày'], ['Hết hạn', so(dem.het), ''], ['Bị khoá', so(dem.khoa), '']].map(([a, b, c, noi]) => h('div', { lop: 'o-so' + (noi ? ' noi' : '') }, h('div', { lop: 'nhan-so' }, a), h('div', { lop: 'so' }, b), h('div', { lop: 'ghi' }, c))));
      const chuaDung = ds.length - daDung;
      xoa(bdKey).append(thanhChong([
        { ten: 'Đang dùng', gia: Math.max(0, dem.dung - chuaDung), chu: so(Math.max(0, dem.dung - chuaDung)), mau: 'mau-ok' },
        { ten: 'Chưa kích hoạt', gia: chuaDung, chu: so(chuaDung), mau: 'mau-xam' },
        { ten: 'Sắp hết hạn', gia: dem.sap, chu: so(dem.sap), mau: 'mau-chu-y' },
        { ten: 'Hết hạn', gia: dem.het, chu: so(dem.het), mau: 'mau-loi' },
        { ten: 'Bị khoá', gia: dem.khoa, chu: so(dem.khoa), mau: 'mau-loi-dam' },
      ], 'Tình trạng key'));
      ve();
    }
    return { nap, thua: true };
  }

  // ----- Cài đặt -----
  function trangCaiDat(goc) {
    const nhip = h('input', { lop: 'o', type: 'number', min: 60, max: 3600 });
    const giu = h('input', { lop: 'o', type: 'number', min: 7, max: 365 });
    const ban = h('input', { lop: 'o', maxlength: 20, placeholder: 'Để trống: lấy bản cao nhất đang có máy dùng' });
    goc.append(dauTrang('Cài đặt'), h('div', { lop: 'mau-cd' },
      h('section', { lop: 'khoi' }, h('div', { lop: 'dau-khoi' }, h('h2', null, 'Thu thập')), h('div', { lop: 'than-khoi' },
        h('div', { lop: 'hai-cot' }, truong('Nhịp báo cáo (giây)', nhip, 'Mở hoặc đóng profile được gửi ngay, không chờ nhịp.'), truong('Giữ lịch sử (ngày)', giu)),
        truong('Phiên bản mới nhất', ban, 'Máy thấp hơn bản này sẽ được báo là bản cũ.'),
        h('button', { lop: 'nut chinh-nut', bam: async () => {
          try { await ghi('luuCauHinh', { sau_giay: Number(nhip.value), giu_ngay: Number(giu.value), phien_ban_moi: ban.value }); bao('Đã lưu'); } catch (e) { bao(e.message, true); }
        } }, 'Lưu'))),
      h('section', { lop: 'khoi' }, h('div', { lop: 'dau-khoi' }, h('h2', null, 'Máy gửi những gì (chỉ để quản lý)')), h('div', { lop: 'than-khoi ghi-thu-thap' },
        h('div', null, h('b', null, 'Có gửi'), h('ul', null, ['Tên máy, phiên bản, hệ điều hành', 'Số lượng profile trên máy', 'Profile đang mở: tên, folder, đã mở bao lâu', 'Automation đang chạy, số dòng xong/lỗi', 'Địa chỉ máy chủ team đang nối'].map((x) => h('li', null, x)))),
        h('div', null, h('b', null, 'Không gửi'), h('ul', null, ['Dữ liệu trong profile: cookie, mật khẩu, phiên đăng nhập', 'Danh sách các profile không mở', 'Trang web đang xem, ảnh màn hình, phím gõ', 'Thông tin proxy'].map((x) => h('li', null, x))))))));
    return {
      async nap() {
        if (document.activeElement && [nhip, giu, ban].includes(document.activeElement)) return;
        const c = await doc('cauHinh');
        nhip.value = c.sau_giay; giu.value = c.giu_ngay; ban.value = c.phien_ban_moi || '';
      },
      thua: true,
    };
  }

  // ---- Khung ứng dụng ----
  const NAV = [
    ['Giám sát', [['tongQuan', 'Tổng quan', trangTongQuan], ['thietBi', 'Thiết bị', trangThietBi], ['nhanSu', 'Cộng sự', trangNhanSu]]],
    ['Dữ liệu', [['lichSu', 'Lịch sử', trangLichSu], ['baoCao', 'Báo cáo', trangBaoCao]]],
    ['Hệ thống', [['key', 'Key', trangKey], ['caiDat', 'Cài đặt', trangCaiDat]]],
  ];
  let trangHien = null, hen = null;
  const nutNav = {};
  const chinh = () => document.querySelector('.chinh');
  const dieuHuong = (t) => { location.hash = t; };
  function capNhatNav(canhBao) {
    const n = nutNav.tongQuan;
    if (!n) return;
    const cu = n.querySelector('.dem');
    if (cu) cu.remove();
    if (canhBao) n.append(h('span', { lop: 'dem' }, canhBao));
  }
  function napLai() {
    if (trangHien) Promise.resolve(trangHien.nap()).catch((e) => { if (e.message !== 'Chưa đăng nhập') bao(e.message, true); });
  }
  function moTrang() {
    const ten = location.hash.slice(1) || 'tongQuan';
    const muc = NAV.flatMap((n) => n[1]).find((m) => m[0] === ten) || NAV[0][1][0];
    Object.entries(nutNav).forEach(([k, b]) => { b.classList.toggle('bat', k === muc[0]); if (k === muc[0]) b.setAttribute('aria-current', 'page'); else b.removeAttribute('aria-current'); });
    if (dongLop) dongLop();
    const goc = xoa(chinh());
    goc.scrollTop = 0;
    document.title = `${muc[1]} · Hir-Login Quản trị`;
    trangHien = muc[2](goc);
    napLai();
  }
  function doiGiaoDien() {
    const moi = document.documentElement.dataset.theme === 'dark' ? 'light' : 'dark';
    document.documentElement.dataset.theme = moi;
    try { localStorage.setItem('hir-qt-giao-dien', moi); } catch (_) { /* chế độ riêng tư */ }
  }
  function veKhung() {
    const goc = xoa(document.getElementById('goc'));
    const ben = h('aside', { lop: 'ben' }, h('div', { lop: 'nhan-hieu' }, h('img', { src: '/logo.png', alt: '' }), h('span', null, 'Hir-Login'), h('span', { lop: 'nho' }, 'Quản trị')),
      h('nav', { 'aria-label': 'Chính' }, NAV.map(([nhom, muc]) => h('div', { lop: 'nhom-nav' }, h('div', { lop: 'nhan-nhom' }, nhom), muc.map(([id, ten]) => { nutNav[id] = h('button', { lop: 'muc-nav', bam: () => dieuHuong(id) }, bieu(id), h('span', null, ten)); return nutNav[id]; })))),
      h('div', { lop: 'chan-ben' },
        h('div', { lop: 'hanh-dong' },
          h('button', { lop: 'nut icon', 'aria-label': 'Đổi sáng tối', title: 'Đổi sáng tối', bam: doiGiaoDien }, bieu('toi')),
          h('button', { lop: 'nut icon', 'aria-label': 'Đăng xuất', title: 'Đăng xuất', bam: async () => { await api('/api/dang-xuat', {}); veDangNhap(); } }, bieu('ra')))));
    goc.append(h('div', { lop: 'khung' }, ben, h('main', { lop: 'chinh' })));
    moTrang();
    clearInterval(hen);
    // Trang có dữ liệu động tự làm mới. Không làm mới khi đang mở lớp nổi hoặc đang ẩn tab.
    hen = setInterval(() => { if (!document.hidden && !dongLop && trangHien && !trangHien.thua) napLai(); }, 15000);
  }
  function veDangNhap() {
    clearInterval(hen);
    trangHien = null;
    if (dongLop) dongLop();
    const mk = h('input', { lop: 'o', type: 'password', placeholder: 'Mật khẩu quản trị', autocomplete: 'current-password', 'aria-label': 'Mật khẩu quản trị', required: true });
    const loi = h('div', { lop: 'loi-hop' });
    const nut = h('button', { lop: 'nut chinh-nut', type: 'submit' }, 'Đăng nhập');
    const form = h('form', { onsubmit: async (e) => {
      e.preventDefault();
      nut.disabled = true; loi.textContent = '';
      try { await api('/api/dang-nhap', { mat_khau: mk.value }); veKhung(); }
      catch (x) { loi.textContent = x.message; nut.disabled = false; mk.select(); }
    } }, h('div', { lop: 'nhan-hieu' }, h('img', { src: '/logo.png', alt: '' }), h('span', null, 'Hir-Login'), h('span', { lop: 'nho' }, 'Quản trị')), mk, nut, loi);
    xoa(document.getElementById('goc')).append(h('div', { lop: 'cua' }, form));
    mk.focus();
  }

  try { const t = localStorage.getItem('hir-qt-giao-dien'); if (t) document.documentElement.dataset.theme = t; } catch (_) { /* chế độ riêng tư */ }
  window.addEventListener('hashchange', () => { if (chinh()) moTrang(); });
  fetch('/api/qt/cauHinh').then((r) => (r.status === 401 ? veDangNhap() : veKhung())).catch(() => veDangNhap());
})();
