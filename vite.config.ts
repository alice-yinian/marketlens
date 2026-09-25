import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Tauri 在移动端调试时通过 TAURI_DEV_HOST 指定开发主机
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react(), tailwindcss()],

  // Tauri 期望固定端口；被占用时直接失败，而不是静默切换端口导致 Tauri 找不到前端
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: {
      // Rust 侧由 cargo 自己监听，避免 vite 重复触发
      ignored: ["**/src-tauri/**"],
    },
  },

  envPrefix: ["VITE_", "TAURI_ENV_*"],

  build: {
    // Windows 用 WebView2(Chromium)，Android 用系统 WebView
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "safari13",
    minify: process.env.TAURI_ENV_DEBUG ? false : "esbuild",
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
});
