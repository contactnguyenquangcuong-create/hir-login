// Tên miền hirlogin.nguyenquangcuong.vn nằm ở tài khoản Cloudflare khác với web quản trị, nên một Worker
// không gắn thẳng được. Worker nhỏ này đứng ở tài khoản giữ tên miền và chuyển mọi yêu cầu sang web quản trị.
export default {
  async fetch(req, env) {
    const u = new URL(req.url);
    const headers = new Headers(req.headers);
    headers.set('x-hir-chuyen', env.BI_MAT);
    headers.set('x-hir-ip', req.headers.get('cf-connecting-ip') || '');
    return fetch(new URL(u.pathname + u.search, env.DICH), { method: req.method, headers, body: req.body, redirect: 'manual' });
  },
};
