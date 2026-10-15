import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [tailwindcss(), react()],

  // The app has one page. Without this the dependency scan looks for every `.html` in the whole
  // project — `src-tauri/target` included, which after a few builds holds hundreds of thousands
  // of files — and the dev server answers nothing (a blank window) until it is done.
  optimizeDeps: {
    entries: ["index.html"],
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    // Bind the literal IPv4 loopback (not `false`, which lets Node resolve
    // `localhost` to `::1` on Windows and listen IPv6-only — then Tauri's
    // health-check hangs forever at "Waiting for your frontend dev server to
    // start"). Must match devUrl in tauri.conf.json.
    host: host || "127.0.0.1",
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
