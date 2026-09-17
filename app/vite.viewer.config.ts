import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { zstdCompressSync, constants } from "node:zlib";
import { defineConfig, type Plugin } from "vite";
import tailwindcss from "@tailwindcss/vite";
import { viteSingleFile } from "vite-plugin-singlefile";
import pkg from "./package.json" with { type: "json" };

// The in-chat viewer, as one self-contained HTML file: a chat client renders it
// in an iframe that may load nothing from anywhere, so every script and style
// is inlined. Built after the app into the same dist/, which is what the host
// already serves and embeds.
export default defineConfig({
  plugins: [tailwindcss(), viteSingleFile(), packScripts()],
  define: { __PARCAD_VERSION__: JSON.stringify(pkg.version) },
  build: {
    target: "es2022",
    outDir: "dist",
    emptyOutDir: false,
    rollupOptions: { input: "viewer.html" },
  },
});

/** Three's Draco decoder built to plain JavaScript: the iframe's policy may refuse WebAssembly. */
const DRACO_DECODER = resolve(__dirname, "node_modules/three/examples/jsm/libs/draco/gltf/draco_decoder.js");
const FZSTD = resolve(__dirname, "node_modules/fzstd/umd/index.js");

/**
 * Replace the page's scripts with their zstd, and a loader that unpacks them:
 * the Draco decoder as a classic script, which defines `DracoDecoderModule`,
 * then the page itself as a module. 1.5 MB of script arrives as about 0.4 MB.
 * Inserting a script with text is inline script to a content policy, which
 * every MCP Apps host allows; evaluating a string is not, and is not done.
 */
function packScripts(): Plugin {
  return {
    name: "parcad-pack-viewer-scripts",
    enforce: "post",
    generateBundle(_options, bundle) {
      const page = bundle["viewer.html"];
      if (!page || page.type !== "asset") this.error("the viewer build produced no viewer.html");
      const html = String(page.source);
      const module = /<script type="module"[^>]*>([\s\S]*?)<\/script>/.exec(html);
      if (!module) this.error("viewer.html carries no module script to pack; has vite-plugin-singlefile changed?");
      const pack = (text: string) =>
        zstdCompressSync(Buffer.from(text), { params: { [constants.ZSTD_c_compressionLevel]: 19 } }).toString("base64");
      const loader = `${readFileSync(FZSTD, "utf8")};
(() => {
  const unpack = (b) => new TextDecoder().decode(fzstd.decompress(Uint8Array.from(atob(b), (c) => c.charCodeAt(0))));
  const add = (text, type) => {
    const script = document.createElement("script");
    if (type) script.type = type;
    script.textContent = text;
    document.body.appendChild(script);
  };
  add(unpack("${pack(readFileSync(DRACO_DECODER, "utf8"))}"));
  add(unpack("${pack(module[1])}"), "module");
})();`;
      // At the end of the body, where the elements the page looks up already exist.
      const tag = `<script>${loader.replaceAll("</script", "<\\/script")}</script>`;
      page.source = html.replace(module[0], "").replace("</body>", () => `${tag}</body>`);
    },
  };
}
