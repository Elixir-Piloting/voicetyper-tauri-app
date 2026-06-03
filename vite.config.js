import { defineConfig } from "vite";

const host = process.env.TAURI_DEV_HOST;

import { copyFileSync } from "fs";

export default defineConfig({
  root: "src",
  build: {
    outDir: "../dist",
  },
  plugins: [
    {
      name: "copy-extra-html",
      closeBundle() {
        copyFileSync("src/dynamic-island.html", "dist/dynamic-island.html");
      },
    },
  ],
  clearScreen: false,
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
      ignored: ["**/src-tauri/**"],
    },
  },
});
