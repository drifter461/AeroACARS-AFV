import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react()],

  // v0.8.3: Chunk-Splitting. Vorher landete alles in einem 824 KB
  // index-*.js — Vite warnte "chunks larger than 500 kB". Tauri laedt
  // den Frontend-Bundle aus dem Filesystem (kein Netz-Latenz-Impact),
  // aber kleinere Chunks helfen dem Browser-Parser + erlauben in
  // Zukunft Lazy-Loading von Tab-Bundles. Splits orientieren sich an
  // Vendor-Familien — selten geaenderte deps bleiben cached.
  build: {
    // Default 500 kB ist fuer Web-Apps mit Netz-Latenz konservativ.
    // Tauri laedt aus dem Filesystem — 700 kB main chunk ist hier
    // OK. Lazy-Loading per Tab (LandingPanel/ACARS-Log etc.) wuerde
    // den main weiter shrinken, ist aber separates Refactor-Ticket
    // (geplant fuer v0.9.x — siehe DevDocs).
    chunkSizeWarningLimit: 700,
    rollupOptions: {
      output: {
        manualChunks: {
          // React + DOM-Rendering — 130-180 kB, sehr stabil
          "vendor-react": ["react", "react-dom"],
          // i18n-Stack (~80 kB) — eigenes Chunk, weil Sprachfiles
          // (locales/*.json) eh schon dynamisch via `react-i18next`
          // geladen werden koennten in v0.9.x.
          "vendor-i18n": [
            "i18next",
            "i18next-browser-languagedetector",
            "react-i18next",
          ],
          // Markdown-Rendering (~200 kB durch unified/remark/rehype)
          // — wird nur im About-Tab + Release-Notes-Anzeige
          // gebraucht, perfekter Code-Splitting-Kandidat.
          "vendor-markdown": ["react-markdown", "remark-gfm"],
        },
      },
    },
  },

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
