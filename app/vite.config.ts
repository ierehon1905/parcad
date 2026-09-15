import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { defineConfig, type Plugin } from "vite";
import preact from "@preact/preset-vite";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig(({ mode }) => {
  const playground = mode === "playground";
  return {
    // Tailwind v4 as a Vite plugin: no PostCSS pipeline and no config file. The
    // design tokens live in `src/style.css`'s `@theme` block, which is also where
    // anything that is not markup — the CodeMirror theme, the viewport's clear
    // colour — reads them from as ordinary custom properties.
    // Preact rather than React: the same JSX and the same hooks, which is what a
    // model that has never seen this repository already knows, at a tenth of the
    // weight. `@preact/signals` carries the state the whole window shares — see
    // src/state.ts for why that is signals and not component state.
    plugins: [preact(), tailwindcss(), ...(playground ? [kernel()] : [])],
    // The playground is served from a project page, https://<owner>.github.io/parcad/.
    base: playground ? (process.env.PARCAD_PLAYGROUND_BASE ?? "/parcad/") : "/",
    // Tauri drives the dev server on a fixed port and fails loudly rather than
    // silently moving if it is taken.
    clearScreen: false,
    server: {
      port: 1420,
      strictPort: true,
      // The seed parts live in examples/ at the repository root, outside the
      // Vite root; the playground bundles them from there (src/page/store.ts).
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
    worker: { format: "es" },
    build: {
      target: "es2022",
      sourcemap: true,
      outDir: playground ? "dist-playground" : "dist",
    },
  };
});

/**
 * The WebAssembly kernel, as files the playground fetches after first paint.
 *
 * Built by playground/build-kernel.sh into target/wasm/web (or
 * PARCAD_KERNEL_DIR), copied under a directory named for the wasm's content
 * hash so a deploy never serves one build's JavaScript with another's module,
 * and described to the page as `__PARCAD_KERNEL__` with its size, which is
 * what the loading state shows.
 */
function kernel(): Plugin {
  const dir = resolve(process.env.PARCAD_KERNEL_DIR || resolve(__dirname, "../target/wasm/web"));
  const script = resolve(dir, "parcad-wasm.js");
  const wasm = resolve(dir, "parcad_wasm.wasm");
  const missing = () =>
    new Error(
      `the playground needs the WebAssembly kernel at ${dir}, and it is not there.\n` +
        "Build it with:  EMSDK=/path/to/emsdk playground/build-kernel.sh   (see playground/README.md)",
    );
  let hash = "";
  let bytes = 0;
  return {
    name: "parcad-kernel",
    config() {
      if (!existsSync(script) || !existsSync(wasm)) throw missing();
      const module = readFileSync(wasm);
      hash = createHash("sha256").update(module).update(readFileSync(script)).digest("hex").slice(0, 12);
      bytes = module.length;
      return {
        define: {
          __PARCAD_KERNEL__: JSON.stringify({
            script: `kernel/${hash}/parcad-wasm.js`,
            wasm: `kernel/${hash}/parcad_wasm.wasm`,
            bytes,
          }),
        },
      };
    },
    configureServer(server) {
      server.middlewares.use((request, response, next) => {
        const match = request.url?.match(/\/kernel\/[0-9a-f]+\/(parcad-wasm\.js|parcad_wasm\.wasm)$/);
        if (!match) return next();
        const file = match[1] === "parcad-wasm.js" ? script : wasm;
        response.setHeader("content-type", file === wasm ? "application/wasm" : "text/javascript");
        response.end(readFileSync(file));
      });
    },
    generateBundle() {
      this.emitFile({ type: "asset", fileName: `kernel/${hash}/parcad-wasm.js`, source: readFileSync(script) });
      this.emitFile({ type: "asset", fileName: `kernel/${hash}/parcad_wasm.wasm`, source: readFileSync(wasm) });
      // The wasm links OpenCASCADE statically, so its licences travel with it (NOTICE.md).
      const repo = resolve(__dirname, "..");
      for (const [name, from] of [
        ["NOTICE.md", "NOTICE.md"],
        ["LICENSE-MIT", "LICENSE-MIT"],
        ["LICENSE-APACHE", "LICENSE-APACHE"],
        ["OCCT_LICENSE_LGPL_21.txt", "vendor/occt-sys/OCCT/LICENSE_LGPL_21.txt"],
        ["OCCT_LGPL_EXCEPTION.txt", "vendor/occt-sys/OCCT/OCCT_LGPL_EXCEPTION.txt"],
      ]) {
        this.emitFile({ type: "asset", fileName: `licenses/${name}`, source: readFileSync(resolve(repo, from)) });
      }
    },
  };
}
