import path from "node:path";
import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// DEV ONLY: serves the real UI with a mocked Tauri backend on port 1430.
// Vite's root stays the repo root (like the main app) so Tailwind scans src/
// and public/ is served; "/" is rewritten to the preview's index.html.
const repoRoot = path.resolve(__dirname, "../..");

const previewIndex = (): Plugin => ({
  name: "ui-preview-index",
  configureServer(server) {
    server.middlewares.use((req, _res, next) => {
      if (req.url === "/" || req.url?.startsWith("/?")) {
        req.url = "/dev/ui-preview/index.html" + (req.url.slice(1) || "");
      }
      next();
    });
  },
});

export default defineConfig({
  root: repoRoot,
  plugins: [previewIndex(), react(), tailwindcss()],
  resolve: {
    alias: { "@": path.resolve(repoRoot, "src") },
  },
  clearScreen: false,
  // Keep the dependency cache apart from the main app's dev server.
  cacheDir: path.resolve(repoRoot, "node_modules/.vite-ui-preview"),
  build: { target: "esnext" }, // top-level await in main.tsx
  esbuild: { target: "esnext" },
  optimizeDeps: { esbuildOptions: { target: "esnext" } },
  server: {
    port: 1430,
    strictPort: true,
    host: false,
    fs: { allow: [repoRoot] },
    watch: { ignored: ["**/src-tauri/**"] },
  },
});
