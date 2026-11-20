import { resolve } from "node:path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// The website (site/), built for GitHub Pages. It reuses the app's real UI
// with mocked IPC, so it lives next to the app's sources.
//   pnpm site        # dev server
//   pnpm build:site  # -> app/dist-site
export default defineConfig({
  root: resolve(__dirname, "site"),
  base: "./",
  plugins: [react()],
  // Screenshots come from docs/images, outside the site folder.
  server: { port: 1440, fs: { allow: [resolve(__dirname, "..")] } },
  build: { outDir: resolve(__dirname, "dist-site"), emptyOutDir: true, target: "es2021" },
});
