import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 5174,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    rollupOptions: {
      output: {
        // Keep the syntax highlighter and the Svelte runtime in stable chunks
        // so an app-code change does not invalidate them in the webview cache.
        manualChunks(id) {
          if (id.includes("node_modules/highlight.js")) return "highlight";
          if (id.includes("node_modules/svelte")) return "svelte";
          return undefined;
        },
      },
    },
  },
});
