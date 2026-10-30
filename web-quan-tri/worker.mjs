// Worker `hirlogin-web`: server trung tâm của Hir-Login. Dữ liệu nằm trong Durable Object (SQLite), như `hir-key`.
import { DurableObject } from 'cloudflare:workers';
import { dung, baoCao, donDep, biChan, ghiSai, xoaSai, DOC, GHI } from './core.mjs';
import { xuLy, goiKey } from './dinh-tuyen.mjs';
import trang from './index.html';
import app from './app.js.txt';
import kieu from './style.css';
import logo from './logo.png';

const NHIP_DON_MS = 5 * 60 * 1000;

export class KhoQuanTri extends DurableObject {
  constructor(ctx, env) {
    super(ctx, env);
    this.env = env;
    this.sql = (q, ...p) => this.ctx.storage.sql.exec(q, ...p).toArray();
    this.ctx.blockConcurrencyWhile(async () => {
      dung(this.sql);
      if (!(await this.ctx.storage.getAlarm())) await this.ctx.storage.setAlarm(Date.now() + NHIP_DON_MS);
    });
  }
  async alarm() {
    donDep(this.sql, Date.now());
    await this.ctx.storage.setAlarm(Date.now() + NHIP_DON_MS);
  }
  nhanBaoCao(b, ip) {
    return baoCao(this.sql, b, ip, (key) => goiKey(this.env, 'layKey', { key }), Date.now());
  }
  doc(ten, a) {
    if (!Object.prototype.hasOwnProperty.call(DOC, ten)) return { loi: 'Không có hàm' };
    return DOC[ten](this.sql, a || {}, Date.now());
  }
  ghi(ten, a) {
    if (!Object.prototype.hasOwnProperty.call(GHI, ten)) return { loi: 'Không có hàm' };
    return GHI[ten](this.sql, a || {}, Date.now());
  }
  coBiChan(ip) { return biChan(this.sql, ip, Date.now()); }
  saiMatKhau(ip) { ghiSai(this.sql, ip, Date.now()); }
  dungMatKhau(ip) { xoaSai(this.sql, ip); }
}

export default {
  fetch(req, env) {
    const moiTruong = Object.assign(Object.create(env), { KHO: () => env.KHO.get(env.KHO.idFromName('chinh')) });
    return xuLy(req, moiTruong, { trang, app, kieu, logo });
  },
};
