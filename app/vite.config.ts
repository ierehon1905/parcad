import { defineConfig } from "vite";

export default defineConfig({
  // Tauri drives the dev server on a fixed port and fails loudly rather than
  // silently moving if it is taken.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    target: "es2022",
    sourcemap: true,
  },
});
