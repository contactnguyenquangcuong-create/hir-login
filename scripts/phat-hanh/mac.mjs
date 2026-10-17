// Step 2 of a release: after the Windows build (scripts/phat-hanh/windows.mjs) has been checked,
// push the version tag to GitHub — which starts .github/workflows/release-mac.yml — then wait for
// the Mac build and confirm the Mac channel carries the new version.
//
// Usage: node scripts/phat-hanh/mac.mjs            (asks before pushing)
//        node scripts/phat-hanh/mac.mjs --yes      (no question)
import fs from 'fs';
import path from 'path';
import readline from 'readline/promises';
import { execFileSync } from 'child_process';
import { fileURLToPath } from 'url';
import { capNhat } from './dung-worker.mjs';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const REPO = 'contactnguyenquangcuong-create/hir-login';
const out = (cmd, args) => execFileSync(cmd, args, { cwd: ROOT, encoding: 'utf8' }).trim();
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const version = JSON.parse(fs.readFileSync(path.join(ROOT, 'src-tauri', 'tauri.conf.json'), 'utf8')).version;
const tag = 'v' + version;
const { url: base } = capNhat();

try { out('git', ['rev-parse', tag + '^{commit}']); } catch { throw new Error(`Chưa có tag ${tag} — chạy scripts/phat-hanh/windows.mjs trước`); }
const win = await (await fetch(`${base}/windows/x86_64/latest.json?t=${Date.now()}`)).text();
if (!win || JSON.parse(win).version !== version) throw new Error(`Kênh Windows chưa có ${version} — phát hành và thử bản Windows trước`);

if (!process.argv.includes('--yes')) {
  const rl = readline.createInterface({ input: process.stdin, output: process.stdout });
  const a = await rl.question(`Bản Windows ${version} đã thử ổn? Đẩy ${tag} lên GitHub để build bản Mac (y/N): `);
  rl.close();
  if (!/^(y|yes|c|co|có)$/i.test(a.trim())) { console.log('Đã huỷ, chưa đẩy gì.'); process.exit(0); }
}

const pushedAt = Date.now();
execFileSync('git', ['push', 'origin', 'main', tag], { cwd: ROOT, stdio: 'inherit' });
console.log(`Đã đẩy ${tag}. Chờ GitHub build bản Mac (thường 10–20 phút)…`);

// Find the run the push started, then wait for it.
const gh = (p) => fetch(`https://api.github.com/repos/${REPO}${p}`, { headers: { 'user-agent': 'hir-login-release' } }).then((r) => r.json());
let run = null;
for (let i = 0; i < 40 && !run; i++) {
  await sleep(15000);
  const runs = (await gh('/actions/runs?per_page=10')).workflow_runs || [];
  run = runs.find((r) => r.name === 'Release Mac' && r.head_branch === tag && Date.parse(r.created_at) >= pushedAt - 60000);
}
if (!run) throw new Error('Không thấy lượt build Mac trên GitHub — xem tab Actions của repo');
console.log('Lượt build: ' + run.html_url);
while (run.status !== 'completed') { await sleep(30000); run = await gh('/actions/runs/' + run.id); }
if (run.conclusion !== 'success') throw new Error(`Build Mac ${run.conclusion} — xem ${run.html_url} (thiếu GitHub Secrets? xem đầu file release-mac.yml)`);

const mac = await (await fetch(`${base}/darwin/aarch64/latest.json?t=${Date.now()}`)).json();
if (mac.version !== version) throw new Error(`Build xong nhưng kênh Mac đang là ${mac.version}, chưa phải ${version}`);
console.log(`\nĐÃ PHÁT HÀNH Mac ${version}`);
console.log(`  Máy Mac đang dùng tự thấy bản mới (kênh ${base}/darwin/aarch64/latest.json)`);
console.log(`  Link cài cho người mới dùng Mac: ${base}/tai/mac`);
