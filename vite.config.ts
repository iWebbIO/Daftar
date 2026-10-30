import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";





const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],


  // Tauri expects a fixed port and fails if the port is taken.
  clearScreen: false,
  server: {
    port: 5183,
    strictPort: true,
    // Bind IPv4 explicitly: Tauri's devUrl and any tool that resolves
    // `localhost` to 127.0.0.1 must reach the same server.
    host: host || "127.0.0.1",
    hmr: host ? { protocol: "ws", host, port: 5184 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  // Tauri builds for the webview it actually ships: WebView2 on Windows,
  // Android System WebView on the phone.
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "chrome100",
    // Vite 8 minifies with oxc by default; naming "esbuild" here would make
    // esbuild a separate dependency for no gain.
    minify: !process.env.TAURI_ENV_DEBUG,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
});
