import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { defineConfig, type Plugin } from "vite";
import preact from "@preact/preset-vite";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig(({ mode }) => {
  const web = mode === "web";
  return {
    // Tailwind v4 as a Vite plugin: no PostCSS pipeline and no config file. The
    // design tokens live in `src/style.css`'s `@theme` block, which is also where
    // anything that is not markup — the CodeMirror theme, the viewport's clear
    // colour — reads them from as ordinary custom properties.
    // Preact rather than React: the same JSX and the same hooks, which is what a
    // model that has never seen this repository already knows, at a tenth of the
    // weight. `@preact/signals` carries the state the whole window shares — see
    // src/state.ts for why that is signals and not component state.
    plugins: [preact(), tailwindcss(), ...(web ? [kernel()] : [])],
    // ParCAD web is served under the project page, https://<owner>.github.io/parcad/app/;
    // the landing page (site/) has the root.
    base: web ? (process.env.PARCAD_WEB_BASE ?? "/parcad/app/") : "/",
    // Tauri drives the dev server on a fixed port and fails loudly rather than
    // silently moving if it is taken.
    clearScreen: false,
    server: {
      port: 1420,
      strictPort: true,
      // The seed parts live in examples/ at the repository root, outside the
      // Vite root; ParCAD web bundles them from there (src/page/host.ts).
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
      outDir: web ? "dist-web" : "dist",
    },
  };
});

/**
 * The two WebAssembly modules — the kernel and the host — as files the page
 * fetches after first paint.
 *
 * Built by web/build-kernel.sh into target/wasm/web (or
 * PARCAD_KERNEL_DIR), copied under a directory named for their content hash so
 * a deploy never serves one build's JavaScript with another's module, and
 * described to the page as `__PARCAD_KERNEL__`, with the kernel's size, which is
 * what the loading state shows, and `__PARCAD_HOST__`.
 */
function kernel(): Plugin {
  const dir = resolve(process.env.PARCAD_KERNEL_DIR || resolve(__dirname, "../target/wasm/web"));
  const script = resolve(dir, "parcad-wasm.js");
  const wasm = resolve(dir, "parcad_wasm.wasm");
  const hostScript = resolve(dir, "parcad-wasm-host.js");
  const hostWasm = resolve(dir, "parcad_wasm_host.wasm");
  const missing = () =>
    new Error(
      `ParCAD web needs its WebAssembly modules at ${dir}, and it is not there.\n` +
        "Build it with:  EMSDK=/path/to/emsdk web/build-kernel.sh   (see web/README.md)",
    );
  // The part recorded by web/prebuild.sh, if it has been run: shipped
  // beside the kernel so a visitor has geometry before the kernel arrives.
  const first = resolve(process.env.PARCAD_FIRST_PART_DIR || resolve(__dirname, "../target/web"));
  const firstPart = resolve(first, "first-part.json");
  const firstMesh = resolve(first, "first-part.drc");
  const firstScript = resolve(first, "first-part-script.js");
  const firstName = resolve(first, "first-part-name.txt");
  const hasFirst = () =>
    existsSync(firstPart) && existsSync(firstMesh) && existsSync(firstScript) && existsSync(firstName);
  // Three's copy of Draco's decoder, which the page loads only to unpack that mesh.
  const dracoDir = resolve(__dirname, "node_modules/three/examples/jsm/libs/draco");
  const dracoFiles = ["draco_wasm_wrapper.js", "draco_decoder.wasm"];
  let hash = "";
  let bytes = 0;
  return {
    name: "parcad-kernel",
    config() {
      if (![script, wasm, hostScript, hostWasm].every(existsSync)) throw missing();
      const module = readFileSync(wasm);
      hash = createHash("sha256")
        .update(module)
        .update(readFileSync(script))
        .update(readFileSync(hostWasm))
        .update(readFileSync(hostScript))
        .digest("hex")
        .slice(0, 12);
      bytes = module.length;
      return {
        define: {
          __PARCAD_KERNEL__: JSON.stringify({
            script: `kernel/${hash}/parcad-wasm.js`,
            wasm: `kernel/${hash}/parcad_wasm.wasm`,
            bytes,
          }),
          __PARCAD_HOST__: JSON.stringify({
            script: `kernel/${hash}/parcad-wasm-host.js`,
            wasm: `kernel/${hash}/parcad_wasm_host.wasm`,
          }),
          // Matched on the part and its text rather than on the graph: the
          // editor instruments treatment calls, so the graph it builds is not
          // the one a headless run of the same script produces.
          __PARCAD_FIRST_PART__: hasFirst()
            ? JSON.stringify({
                url: `kernel/${hash}/first-part.json`,
                mesh: `kernel/${hash}/first-part.drc`,
                decoder: `kernel/${hash}/draco/`,
                part: readFileSync(firstName, "utf8").trim(),
                script: readFileSync(firstScript, "utf8"),
              })
            : "undefined",
        },
      };
    },
    configureServer(server) {
      server.middlewares.use((request, response, next) => {
        const match = request.url?.match(
          /\/kernel\/[0-9a-f]+\/(parcad-wasm\.js|parcad_wasm\.wasm|parcad-wasm-host\.js|parcad_wasm_host\.wasm|first-part\.json|first-part\.drc|draco\/[\w.]+)$/,
        );
        if (!match) return next();
        if (match[1].startsWith("first-part") || match[1].startsWith("draco/")) {
          if (!hasFirst()) return next();
          const served = match[1] === "first-part.json" ? firstPart : match[1] === "first-part.drc" ? firstMesh : resolve(dracoDir, match[1].slice("draco/".length));
          if (!existsSync(served)) return next();
          response.setHeader("content-type", served.endsWith(".json") ? "application/json" : served.endsWith(".wasm") ? "application/wasm" : served.endsWith(".js") ? "text/javascript" : "application/octet-stream");
          response.end(readFileSync(served));
          return;
        }
        const file = { "parcad-wasm.js": script, "parcad_wasm.wasm": wasm, "parcad-wasm-host.js": hostScript, "parcad_wasm_host.wasm": hostWasm }[match[1]]!;
        response.setHeader("content-type", file.endsWith(".wasm") ? "application/wasm" : "text/javascript");
        response.end(readFileSync(file));
      });
    },
    generateBundle() {
      this.emitFile({ type: "asset", fileName: `kernel/${hash}/parcad-wasm.js`, source: readFileSync(script) });
      this.emitFile({ type: "asset", fileName: `kernel/${hash}/parcad_wasm.wasm`, source: readFileSync(wasm) });
      this.emitFile({ type: "asset", fileName: `kernel/${hash}/parcad-wasm-host.js`, source: readFileSync(hostScript) });
      this.emitFile({ type: "asset", fileName: `kernel/${hash}/parcad_wasm_host.wasm`, source: readFileSync(hostWasm) });
      if (hasFirst()) {
        this.emitFile({ type: "asset", fileName: `kernel/${hash}/first-part.json`, source: readFileSync(firstPart) });
        this.emitFile({ type: "asset", fileName: `kernel/${hash}/first-part.drc`, source: readFileSync(firstMesh) });
        for (const name of dracoFiles) {
          this.emitFile({ type: "asset", fileName: `kernel/${hash}/draco/${name}`, source: readFileSync(resolve(dracoDir, name)) });
        }
      }
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
