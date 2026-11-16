// Obfuscates the built frontend (dist/assets/*.js) in place, to raise the cost of reverse
// engineering the client once it is in a customer's hands — not to defeat anyone else's
// protection. Runs AFTER `vite build`, right before the result is packaged into the installer:
//   - scripts/phat-hanh/windows.mjs calls this after its own `vite build`.
//   - .github/workflows/release-mac.yml calls this after its own `vite build`, before `tauri build`.
// Never wired into `npm run build`/`npm run dev` — those stay unobfuscated for local debugging.
//
// Usage: node scripts/obfuscate.mjs
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import JavaScriptObfuscator from 'javascript-obfuscator';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const ASSETS = path.join(ROOT, 'dist', 'assets');

const files = fs.existsSync(ASSETS) ? fs.readdirSync(ASSETS).filter((f) => f.endsWith('.js')) : [];
if (!files.length) throw new Error('Không thấy file .js nào trong dist/assets — chạy `vite build` trước');

let done = 0;
for (const name of files) {
  const file = path.join(ASSETS, name);
  const code = fs.readFileSync(file, 'utf8');
  try {
    const out = JavaScriptObfuscator.obfuscate(code, {
      compact: true,
      controlFlowFlattening: true,
      controlFlowFlatteningThreshold: 0.75,
      deadCodeInjection: true,
      deadCodeInjectionThreshold: 0.4,
      identifierNamesGenerator: 'hexadecimal',
      renameGlobals: false, // never touch window/document/globalThis-level names
      stringArray: true,
      stringArrayEncoding: ['base64'],
      stringArrayThreshold: 0.75,
      splitStrings: true,
      splitStringsChunkLength: 10,
      numbersToExpressions: true,
      simplify: true,
      selfDefending: true,
      disableConsoleOutput: false, // keep console.* working — this app's own logging relies on it
    }).getObfuscatedCode();
    fs.writeFileSync(file, out);
    console.log(`Đã làm rối: assets/${name} (${(code.length / 1024).toFixed(0)} KB -> ${(out.length / 1024).toFixed(0)} KB)`);
    done++;
  } catch (e) {
    // A file the tool can't parse must still ship — never let one bad file fail the whole release.
    console.warn(`BỎ QUA assets/${name} (không làm rối được, giữ nguyên): ${e.message}`);
  }
}
if (!done) throw new Error('Không file nào làm rối được — kiểm tra lại trước khi đóng gói');
