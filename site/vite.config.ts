import { reactRouter } from "@react-router/dev/vite";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";

export default defineConfig({
  // parcad.dev serves the site at its root; set this to lay it out anywhere else.
  base: process.env.PARCAD_SITE_BASE ?? "/",
  plugins: [tailwindcss(), reactRouter()],
  // three is imported lazily; pre-bundling it up front stops the dev server
  // re-optimising mid-session and failing that import with a 504.
  optimizeDeps: {
    include: [
      "three",
      "three/examples/jsm/loaders/STLLoader.js",
      "three/examples/jsm/controls/OrbitControls.js",
      "three/examples/jsm/utils/BufferGeometryUtils.js",
    ],
  },
});
