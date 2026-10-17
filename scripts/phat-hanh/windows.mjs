// Releases the Windows build of Hir-Login from THIS PC to the Cloudflare update channel.
//
//   1. Bump the version (package.json, package-lock.json, src-tauri/Cargo.toml + Cargo.lock,
//      src-tauri/tauri.conf.json) and commit it.
//   2. node scripts/phat-hanh/windows.mjs --ghi-chu "what changed, in Vietnamese"
//
// Builds the NSIS installer, signs it with D:\AI\HirLogin-goi\hir-login.key (password in ky.json),
// uploads it, and only then switches windows/x86_64/latest.json to the new version — so a client
// that sees the new version can always download it. Tags vX.Y.Z (pushing is left to you: no CI
// runs on tags any more; the Mac build is started by hand on GitHub, see release-mac.yml).
// Never packages or prints a secret.
import fs from 'fs';
import path from 'path';
import crypto from 'crypto';
import { execFileSync } from 'child_process';
import { fileURLToPath } from 'url';
import { GOI, capNhat, dungWorker } from './dung-worker.mjs';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const TARGET_DIR = path.join(ROOT, 'src-tauri', 'target', 'rel');
const arg = (name) => { const i = process.argv.indexOf(name); return i > 0 ? process.argv[i + 1] : undefined; };
const notes = arg('--ghi-chu') || '';
const sh = (cmd, args, opts = {}) => execFileSync(cmd, args, { cwd: ROOT, stdio: 'inherit', shell: process.platform === 'win32', ...opts });
const out = (cmd, args) => execFileSync(cmd, args, { cwd: ROOT, encoding: 'utf8' }).trim();

const version = JSON.parse(fs.readFileSync(path.join(ROOT, 'src-tauri', 'tauri.conf.json'), 'utf8')).version;
const tag = 'v' + version;
console.log(`== Hir-Login ${version} cho Windows ==`);

// 1) The build has to be exactly what is committed.
const dirty = out('git', ['status', '--porcelain']).split('\n').filter((l) => l.trim() && !/gen\/schemas\//.test(l));
if (dirty.length) throw new Error('Còn thay đổi chưa commit:\n' + dirty.join('\n'));
const head = out('git', ['rev-parse', 'HEAD']);
const tagged = (() => { try { return out('git', ['rev-parse', tag + '^{commit}']); } catch { return null; } })();
if (tagged && tagged !== head) throw new Error(`Tag ${tag} đã có ở commit khác — nâng phiên bản trước khi phát hành`);

// 2) Channel + signing key.
const { url: base, uploadKey } = fs.existsSync(path.join(GOI, 'cap-nhat.json')) ? capNhat() : await dungWorker();
const key = JSON.parse(fs.readFileSync(path.join(GOI, 'ky.json'), 'utf8'));
const current = await (await fetch(`${base}/windows/x86_64/latest.json?t=${Date.now()}`)).text();
if (current && JSON.parse(current).version === version) throw new Error(`Kênh Windows đã có ${version} — nâng phiên bản trước khi phát hành`);

// 3) Frontend, then the signed installer (beforeBuildCommand skipped: it reinstalls ui-kit).
sh('npx', ['tsc', '--noEmit', '-p', 'tsconfig.json']);
sh('npx', ['vite', 'build']);
fs.mkdirSync(TARGET_DIR, { recursive: true });
const overrides = path.join(TARGET_DIR, 'phat-hanh.conf.json');
fs.writeFileSync(overrides, JSON.stringify({ build: { beforeBuildCommand: '' } }));
sh('npx', ['tauri', 'build', '--bundles', 'nsis', '--config', overrides], {
  env: { ...process.env, CARGO_TARGET_DIR: TARGET_DIR, TAURI_SIGNING_PRIVATE_KEY: key.privateKeyFile, TAURI_SIGNING_PRIVATE_KEY_PASSWORD: key.password },
});
const name = `Hir-Login_${version}_x64-setup.exe`;
const exe = path.join(TARGET_DIR, 'release', 'bundle', 'nsis', name);
const sig = fs.readFileSync(exe + '.sig', 'utf8').trim();
const bytes = fs.readFileSync(exe);
const sha256 = crypto.createHash('sha256').update(bytes).digest('hex');
console.log(`Bộ cài: ${name} (${(bytes.length / 1048576).toFixed(1)} MB)`);

// 4) Upload: the file first, the manifest that points at it last.
async function put(k, body, type) {
  const r = await fetch(`${base}/upload/${encodeURIComponent(k)}`, { method: 'PUT', headers: { 'x-upload-key': uploadKey, 'content-type': type }, body });
  if (!r.ok) throw new Error(`Đẩy ${k} lỗi: HTTP ${r.status} ${await r.text()}`);
}
const fileUrl = `${base}/file/${name}`;
await put('file:' + name, bytes, 'application/vnd.microsoft.portable-executable');
const got = await fetch(fileUrl);
const gotHash = crypto.createHash('sha256').update(Buffer.from(await got.arrayBuffer())).digest('hex');
if (gotHash !== sha256) throw new Error('File trên Cloudflare không khớp file vừa build — dừng, chưa đổi latest.json');
const manifest = { version, notes, pub_date: new Date().toISOString(), url: fileUrl, signature: sig, sha256 };
await put('latest:windows-x86_64', JSON.stringify(manifest), 'application/json');
await put('tai:windows', fileUrl, 'text/plain');
const live = await (await fetch(`${base}/windows/x86_64/latest.json?t=${Date.now()}`)).json();
if (live.version !== version || live.signature !== sig) throw new Error('latest.json trên mạng chưa khớp — thử lại sau ít phút');

// 5) Keep a copy, tag the release.
fs.copyFileSync(exe, path.join(GOI, name));
if (!tagged) sh('git', ['tag', tag]);
console.log(`\nĐÃ PHÁT HÀNH Windows ${version}`);
console.log(`  Máy đang dùng tự thấy bản mới (kênh ${base}/windows/x86_64/latest.json)`);
console.log(`  Link cài cho người mới: ${base}/tai/windows`);
console.log(`  Bản sao: ${path.join(GOI, name)}`);
console.log(`  Tag ${tag} mới ở máy này — đẩy lên bằng: git push origin main ${tag}`);
