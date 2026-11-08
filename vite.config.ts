import { defineConfig } from "vite";
// @ts-expect-error type error without @types/node package
import process from "node:process";
// @ts-expect-error type error without @types/node package
import { readFileSync } from "node:fs";
// @ts-expect-error type error without @types/node package
import { execSync } from "node:child_process";
const host = process.env.TAURI_DEV_HOST;

/** Commit, mit dem gebaut wird: git, sonst .git-commit (schreiben die Hooks, z. B. für Builds ohne git), sonst leer. */
function commit() {
  for (const read of [() => execSync("git rev-parse --short HEAD", { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }), () => readFileSync(".git-commit", "utf8")]) {
    try {
      const c = read().trim();
      if (c) return c;
    } catch {}
  }
  return "";
}

// https://vite.dev/config/
export default defineConfig(() => ({
  define: { __COMMIT__: JSON.stringify(commit()) },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
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
