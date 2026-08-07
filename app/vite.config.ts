import { defineConfig } from "vite";
import preact from "@preact/preset-vite";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  // Tailwind v4 as a Vite plugin: no PostCSS pipeline and no config file. The
  // design tokens live in `src/style.css`'s `@theme` block, which is also where
  // anything that is not markup — the CodeMirror theme, the viewport's clear
  // colour — reads them from as ordinary custom properties.
  // Preact rather than React: the same JSX and the same hooks, which is what a
  // model that has never seen this repository already knows, at a tenth of the
  // weight. `@preact/signals` carries the state the whole window shares — see
  // src/state.ts for why that is signals and not component state.
  plugins: [preact(), tailwindcss()],
  // Tauri drives the dev server on a fixed port and fails loudly rather than
  // silently moving if it is taken.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // The example scripts live in examples/ at the repository root, outside the
    // Vite root, and src/examples.ts reads them from there so the app and the
    // eval corpus cannot drift apart.
    fs: { allow: [".."] },
    // A browser on the dev server is the same application as the desktop
    // window, so it needs the same backend. The desktop process hosts one on
    // PARCAD_HTTP_PORT; proxying keeps the frontend's calls same-origin, which
    // is what lets it ship no CORS configuration at all.
    proxy: {
      "/api": {
        target: `http://127.0.0.1:${process.env.PARCAD_HTTP_PORT ?? 4242}`,
        changeOrigin: false,
      },
    },
  },
  build: {
    target: "es2022",
    sourcemap: true,
  },
});
