import process from "node:process";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [react(), tailwindcss()],

  // Loaded from local disk by the desktop shell, so bundle size matters far less than on the web.
  build: { chunkSizeWarningLimit: 2000 },

  // Vite options tailored for Tauri development, applied in `tauri dev` / `tauri build`:
  // 1. don't obscure Rust errors
  clearScreen: false,
  // 2. Tauri expects a fixed port; fail if it is taken
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    // 3. don't watch the Rust side
    watch: { ignored: ["**/src-tauri/**", "**/crates/**", "**/target/**"] },
  },
}));
