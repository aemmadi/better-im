import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Vite config tuned for Tauri v2: a fixed dev port the Rust side points at
// (`devUrl`), no browser auto-open, and a Safari-compatible build target.
export default defineConfig({
  plugins: [react()],
  // Prevent Vite from obscuring Rust compile errors in the terminal.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: false,
    watch: {
      // node_modules and the Rust crate are irrelevant to the webview HMR.
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    // macOS WKWebView (Tauri) — Safari 13+ features are safe.
    target: "safari14",
    outDir: "dist",
    emptyOutDir: true,
    sourcemap: false,
  },
});
