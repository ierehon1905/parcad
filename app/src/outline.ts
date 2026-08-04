/**
 * Logical edges, recovered in screen space.
 *
 * A user cares about the edges and faces of the *part*, not about the triangles
 * we happened to approximate it with. An implicit model has no face or edge
 * records to draw, so they have to be found — and the one thing that must not
 * leak into the answer is the tessellation.
 *
 * Two discontinuities, each chosen so that mesh density cannot fake it:
 *
 * - **Normals.** The vertex normals come from the distance field's gradient, not
 *   from facets, so they vary smoothly across a curved patch however coarsely it
 *   is triangulated, and jump only where the surface genuinely creases. This is
 *   why the same idea fails with `EdgesGeometry`: that works on facet normals,
 *   where a coarse cylinder is all creases.
 *
 * - **Depth, second difference.** `|left + right - 2*centre|` is zero for any
 *   flat surface at *any* angle to the camera, because a plane is linear in
 *   screen space. Comparing raw neighbouring depths instead would paint every
 *   steeply-angled surface as an edge — the same mistake, in screen space, that
 *   a fixed threshold makes in the CPU renderer.
 */

import * as THREE from "three";
import { EffectComposer } from "three/examples/jsm/postprocessing/EffectComposer.js";
import { RenderPass } from "three/examples/jsm/postprocessing/RenderPass.js";
import { ShaderPass } from "three/examples/jsm/postprocessing/ShaderPass.js";

const OutlineShader = {
  uniforms: {
    tDiffuse: { value: null as THREE.Texture | null },
    tNormal: { value: null as THREE.Texture | null },
    tDepth: { value: null as THREE.Texture | null },
    resolution: { value: new THREE.Vector2() },
    cameraNear: { value: 0.1 },
    cameraFar: { value: 1000 },
    /** Cosine of the crease angle. Lower value = only sharper creases drawn. */
    normalThreshold: { value: Math.cos((48 * Math.PI) / 180) },
    /** Screen-space depth curvature above which this counts as an edge. */
    depthThreshold: { value: 0.00035 },
    edgeColor: { value: new THREE.Color(0x2b3340) },
    edgeStrength: { value: 0.9 },
    /**
     * Whether to draw creases as well as silhouettes.
     *
     * Turned off when the backend supplies real edge curves. Inferring a crease
     * from neighbouring pixels is a fallback for not knowing where it is; once
     * the kernel tells us, drawing both means one line laid over a slightly
     * different line, which looks worse than either alone. The silhouette half
     * is still wanted either way — the outline of a cylinder is not an edge of
     * the solid, so no kernel will ever hand it to us.
     */
    creases: { value: 1.0 },
  },

  vertexShader: /* glsl */ `
    varying vec2 vUv;
    void main() {
      vUv = uv;
      gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
    }
  `,

  fragmentShader: /* glsl */ `
    uniform sampler2D tDiffuse;
    uniform sampler2D tNormal;
    uniform sampler2D tDepth;
    uniform vec2 resolution;
    uniform float cameraNear;
    uniform float cameraFar;
    uniform float normalThreshold;
    uniform float depthThreshold;
    uniform vec3 edgeColor;
    uniform float edgeStrength;
    uniform float creases;
    varying vec2 vUv;

    // Depth as a fraction of the view frustum, so thresholds mean the same
    // thing whatever the part's scale.
    float linearDepth(vec2 uv) {
      float z = texture2D(tDepth, uv).x;
      float ndc = z * 2.0 - 1.0;
      float view = (2.0 * cameraNear * cameraFar) /
                   (cameraFar + cameraNear - ndc * (cameraFar - cameraNear));
      return (view - cameraNear) / (cameraFar - cameraNear);
    }

    vec3 normalAt(vec2 uv) {
      return normalize(texture2D(tNormal, uv).xyz * 2.0 - 1.0);
    }

    void main() {
      vec2 texel = 1.0 / resolution;
      vec4 base = texture2D(tDiffuse, vUv);

      float dC = linearDepth(vUv);

      // Background: nothing in front of the far plane, so nothing to outline.
      if (dC >= 0.9999) {
        gl_FragColor = base;
        return;
      }

      float dL = linearDepth(vUv - vec2(texel.x, 0.0));
      float dR = linearDepth(vUv + vec2(texel.x, 0.0));
      float dU = linearDepth(vUv - vec2(0.0, texel.y));
      float dD = linearDepth(vUv + vec2(0.0, texel.y));

      // Second difference: flat in screen space means flat in the world, at any
      // orientation. Only genuine steps and creases survive this.
      float curvature = max(abs(dL + dR - 2.0 * dC), abs(dU + dD - 2.0 * dC));
      float depthEdge = step(depthThreshold, curvature);

      // A silhouette: a neighbour that is background while we are not.
      float silhouette = 0.0;
      if (max(max(dL, dR), max(dU, dD)) >= 0.9999) silhouette = 1.0;

      vec3 nC = normalAt(vUv);
      float minDot = 1.0;
      minDot = min(minDot, dot(nC, normalAt(vUv - vec2(texel.x, 0.0))));
      minDot = min(minDot, dot(nC, normalAt(vUv + vec2(texel.x, 0.0))));
      minDot = min(minDot, dot(nC, normalAt(vUv - vec2(0.0, texel.y))));
      minDot = min(minDot, dot(nC, normalAt(vUv + vec2(0.0, texel.y))));
      float normalEdge = 1.0 - step(normalThreshold, minDot);

      float creaseEdge = max(depthEdge, normalEdge) * creases;
      float edge = max(creaseEdge, silhouette) * edgeStrength;
      gl_FragColor = vec4(mix(base.rgb, edgeColor, edge), base.a);
    }
  `,
};

/** Beauty pass plus the outline pass, and the normal buffer it needs. */
export class OutlineRenderer {
  private readonly composer: EffectComposer;
  private readonly outlinePass: ShaderPass;
  private readonly normalTarget: THREE.WebGLRenderTarget;
  private readonly normalMaterial = new THREE.MeshNormalMaterial();

  constructor(
    private readonly renderer: THREE.WebGLRenderer,
    private readonly scene: THREE.Scene,
    private readonly camera: THREE.PerspectiveCamera,
  ) {
    const size = renderer.getSize(new THREE.Vector2());
    const ratio = renderer.getPixelRatio();
    const w = Math.max(1, Math.floor(size.x * ratio));
    const h = Math.max(1, Math.floor(size.y * ratio));

    this.normalTarget = new THREE.WebGLRenderTarget(w, h);
    this.normalTarget.depthTexture = new THREE.DepthTexture(w, h);
    this.normalTarget.depthTexture.type = THREE.UnsignedIntType;

    // Multisampling on the composer's own target: routing through a render
    // target otherwise throws away the antialiasing the canvas would have had.
    const beauty = new THREE.WebGLRenderTarget(w, h, { samples: 4 });
    this.composer = new EffectComposer(renderer, beauty);
    this.composer.addPass(new RenderPass(scene, camera));

    this.outlinePass = new ShaderPass(OutlineShader);
    this.outlinePass.uniforms.tNormal.value = this.normalTarget.texture;
    this.outlinePass.uniforms.tDepth.value = this.normalTarget.depthTexture;
    this.composer.addPass(this.outlinePass);

    this.setSize(size.x, size.y);
  }

  /** Draw creases, or leave them to real edge curves supplied by the backend. */
  setCreases(on: boolean) {
    this.outlinePass.uniforms.creases.value = on ? 1.0 : 0.0;
  }

  setSize(width: number, height: number) {
    const ratio = this.renderer.getPixelRatio();
    const w = Math.max(1, Math.floor(width * ratio));
    const h = Math.max(1, Math.floor(height * ratio));

    this.composer.setSize(width, height);
    this.normalTarget.setSize(w, h);
    this.outlinePass.uniforms.resolution.value.set(w, h);
  }

  /**
   * Render one frame.
   *
   * `outlined` is the subset that should get edges — the part, not the grid.
   * Everything else is hidden for the normal pass only, so it contributes
   * neither normals nor depth and therefore cannot be outlined.
   */
  render(outlined: THREE.Object3D[]) {
    const hidden: THREE.Object3D[] = [];
    this.scene.traverse((o) => {
      if (o === this.scene || !o.visible) return;
      const isOutlined = outlined.some((root) => root === o || isDescendant(o, root));
      const drawable = o as THREE.Mesh & THREE.Line & THREE.Points;
      // Lines and points are annotation, never subjects. Left visible they
      // would write their own normals and depth into the buffer and the pass
      // would dutifully outline the outlines.
      if (drawable.isLine || drawable.isPoints) {
        hidden.push(o);
      } else if (!isOutlined && drawable.isMesh) {
        hidden.push(o);
      }
    });

    for (const o of hidden) o.visible = false;
    const prevOverride = this.scene.overrideMaterial;
    const prevBackground = this.scene.background;

    this.scene.overrideMaterial = this.normalMaterial;
    // Clear to a far background so silhouette detection has something definite
    // to compare against.
    this.scene.background = null;
    this.renderer.setRenderTarget(this.normalTarget);
    this.renderer.clear();
    this.renderer.render(this.scene, this.camera);
    this.renderer.setRenderTarget(null);

    this.scene.overrideMaterial = prevOverride;
    this.scene.background = prevBackground;
    for (const o of hidden) o.visible = true;

    this.outlinePass.uniforms.cameraNear.value = this.camera.near;
    this.outlinePass.uniforms.cameraFar.value = this.camera.far;

    this.composer.render();
  }

  dispose() {
    this.normalTarget.dispose();
    this.normalMaterial.dispose();
    this.composer.dispose();
  }
}

function isDescendant(o: THREE.Object3D, root: THREE.Object3D): boolean {
  let p: THREE.Object3D | null = o.parent;
  while (p) {
    if (p === root) return true;
    p = p.parent;
  }
  return false;
}
