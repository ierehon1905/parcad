/**
 * The 3D viewport: GPU triangles at display resolution, deliberately a separate
 * renderer from the CPU rasteriser the agent looks through (`render.rs`).
 */

import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { Line2 } from "three/examples/jsm/lines/Line2.js";
import { LineGeometry } from "three/examples/jsm/lines/LineGeometry.js";
import { LineMaterial } from "three/examples/jsm/lines/LineMaterial.js";
import { LineSegments2 } from "three/examples/jsm/lines/LineSegments2.js";
import { LineSegmentsGeometry } from "three/examples/jsm/lines/LineSegmentsGeometry.js";
import { RoomEnvironment } from "three/examples/jsm/environments/RoomEnvironment.js";
import { AxisTriad } from "./axes";
import { beginDrag } from "./drag";
import { verticesFromEdges, type VertexPoint } from "./entities";
import { OutlineRenderer } from "./outline";

export interface Geometry {
  positions: Float32Array;
  normals: Float32Array;
  indices: Uint32Array;
  /** Logical edges as polylines, the kernel's own. */
  edges?: EdgeCurve[];
  /** Where each face's triangles sit in `indices`. Absent means faces are unpickable, not mispicked. */
  faceRuns?: FaceRun[];
  /** Each face's authored material, by kernel face number; a gap wears the default. */
  faceMaterials?: (FaceMaterial | undefined)[];
  /** A part in named bodies: each one's triangles, one span of `indices` each, which pose and hide as one. */
  bodies?: BodySpan[];
}

/** One named body's share of the mesh. Its edges are the ones whose `body` names it. */
export interface BodySpan {
  name: string;
  /** First triangle, and how many. */
  start: number;
  count: number;
  /** What its faces wear where they carry no material of their own; the look's `part` otherwise. */
  color?: string;
}

type Vec3 = [number, number, number];

/** A light in a look: fixed in the world (and casting the shadow), riding with the camera, or everywhere. */
export type SceneLight =
  | { kind: "sun"; color: string; intensity: number; from: Vec3 }
  | { kind: "camera"; color: string; intensity: number; from: Vec3; toward: Vec3 }
  | { kind: "ambient"; color: string; intensity: number };

/**
 * How the scene around a part is lit and dressed: the window's workbench, or
 * the chat viewer's own. Scene colours, not chrome; see `gold()`.
 */
export interface SceneLook {
  /** The backdrop: one colour, or top and bottom. */
  sky: string | [string, string];
  toneMapping: "neutral" | "none";
  exposure: number;
  /** How strongly the room environment lights and reflects; 0 for none. */
  environment: number;
  lights: SceneLight[];
  /** What a face wears where nothing else is given. */
  surface: { kind: "standard"; roughness: number; metalness: number } | { kind: "phong"; shininess: number; specular: string };
  part: string;
  /** The kernel's edges and the silhouette; `width` in CSS px draws them as screen-space lines. */
  edges: { color: string; opacity: number; width?: number };
  /** The ground grid's centre lines and the rest; no grid when absent. */
  grid?: [string, string];
  /** How dark the part's shadow lies on the ground; no shadow when absent or without a sun. */
  shadow?: number;
  shadowMap: number;
}

/** Fusion-style light canvas: dark chrome, bright work area. */
export const WORKBENCH: SceneLook = {
  sky: ["#ced6e0", "#8b9aaf"],
  toneMapping: "neutral",
  exposure: 1,
  environment: 0.5,
  lights: [
    // Fixed rather than camera-attached: a shadow that swings as you orbit reads as the part moving.
    { kind: "sun", color: "#ffffff", intensity: 2.2, from: [-0.5, -0.9, 1.4] },
    // A weak fill riding with the camera, so faces away from the sun stay readable.
    { kind: "camera", color: "#c9d6ee", intensity: 0.4, from: [0, 0, 1], toward: [0, 0, -1] },
  ],
  surface: { kind: "standard", roughness: 0.46, metalness: 0.1 },
  part: "#8a96a8",
  // Opaque, as in the chat viewer: translucent edges stack darker wherever two meet.
  edges: { color: "#2b3440", opacity: 1 },
  grid: ["#6f7a8b", "#9ba6b5"],
  shadow: 0.22,
  shadowMap: 2048,
};

/** A face's appearance, as `.material()` wrote it. */
export interface FaceMaterial {
  color: string;
  roughness: number;
  metalness: number;
  opacity: number;
  emissive?: string;
  clearcoat: number;
}

/** One face's triangles, as a span of the index buffer. See `state.ts`. */
export interface FaceRun {
  face: number;
  start: number;
  count: number;
}

/** A face of the solid, as the viewport can pick it. */
export interface FacePick {
  /** The kernel's own face number, not this run's position in the list. */
  face: number;
  /** Its triangles, as a span of the index buffer. */
  run: FaceRun;
  triangles: number;
}

/** A logical B-rep edge returned by the exact backend. */
export interface EdgeCurve {
  /** Ephemeral identifier for this evaluated model, for inspection only. */
  id: string;
  points: number[][];
  center: [number, number, number];
  direction: [number, number, number] | null;
  length_mm: number;
  /** Ephemeral source node for a final edge generated by a treatment. */
  treatment_node?: number;
  /** The named body it is on, for a part in several. */
  body?: string;
}

/** An exact corner selected by a source-to-viewport target preview. */
export interface TargetVertex {
  /** Ephemeral ID for this preview only; never a modelling-language input. */
  id: string;
  point: [number, number, number];
}

export interface EdgeCallbacks {
  onFaceHover?: (face: FacePick | undefined) => void;
  onHover?: (edge: EdgeCurve | undefined) => void;
  onSelect?: (edge: EdgeCurve | undefined) => void;
  onVertexHover?: (vertex: VertexPoint | undefined) => void;
  onVertexSelect?: (vertex: VertexPoint | undefined) => void;
}

export interface Bounds {
  min: { x: number; y: number; z: number };
  max: { x: number; y: number; z: number };
}

/** A cutting plane, in the same three fields as `parcad_core::view::Section`. */
export interface SectionPlane {
  axis: "x" | "y" | "z";
  atMm: number;
  keep: "below" | "above";
}

/** The cut face, in the raster's own colour. See `CUT_FACE` in render.rs. */
const CUT_FACE = 0xc99454;

/**
 * `--color-gold` from the stylesheet rather than a second copy of it.
 *
 * Gold means the same thing in the scene as it does in the DOM — *this is the
 * one you are pointing at* — and the editor's treatment mark and the target
 * preview already read it from the theme. Every other colour in this file is
 * the scene's own shading palette and is deliberately not a token: nothing in
 * the DOM wears them.
 *
 * Read lazily, because a custom property does not resolve until the CSS is in.
 */
let goldCache: string | undefined;
function gold(): string {
  if (goldCache === undefined) {
    goldCache = getComputedStyle(document.documentElement).getPropertyValue("--color-gold").trim();
    if (!/^#[0-9a-f]{6}$/i.test(goldCache)) {
      throw new Error(
        `--color-gold read as ${goldCache || "nothing"}. The viewport takes it from ` +
          `the @theme block in app/src/style.css; a renamed or removed token lands here.`,
      );
    }
  }
  return goldCache;
}

const goldHex = () => Number.parseInt(gold().slice(1), 16);

/** The highlight's two colours: neither survives alone against both a bright background and dark material. */
const INK = 0x0b0e13;
const HALO = 0xffffff;
/** The highlight's three widths, in CSS pixels. See `resize` for the conversion. */
const GLOW_PX = 4;
const RING_PX = 2.8;
const CORE_PX = 1.4;
/** The source-selected preview. A fat line for the same reason the hover mark
 *  is one: `linewidth` on a plain `THREE.Line` is ignored by every WebGL
 *  driver, so it drew one hairline pixel however wide it asked to be. */
const TARGET_PX = 2.6;

/** Diagnostic modes, via `?debug=` in the URL: `normals`, `flat`, `noedge`. */
const DEBUG = new URLSearchParams(location.search).get("debug") ?? "";

/**
 * The deepest the depth range may be, as far / near. Wider lets the camera get
 * closer; narrower keeps more depth-buffer precision. At 2000 a 24-bit buffer
 * still resolves 1/15000 of the range at its far end.
 */
const DEPTH_RANGE = 2000;

/** How opaque the material the camera has cut into is drawn: its cut face, then the walls behind it. */
const CUT_VEIL_OPACITY = 0.45;
const CUT_WALL_OPACITY = 0.55;

/** How far the pointer may travel between down and up and still be a click. */
const DRAG_SLOP_PX = 4;

/** How long a change keeps the loop drawing after it: past a hover's settle, a slider's last move, a resize. */
const AWAKE_MS = 300;

export class Viewport {
  private readonly renderer: THREE.WebGLRenderer;
  private readonly scene = new THREE.Scene();
  private readonly camera: THREE.PerspectiveCamera;
  private readonly controls: OrbitControls;
  private readonly partGroup = new THREE.Group();
  /** The look's first sun, which casts the ground shadow and is aimed at the part; none in a shadowless look. */
  private sun?: { light: THREE.DirectionalLight; from: THREE.Vector3 };
  private outline!: OutlineRenderer;
  private readonly axes: AxisTriad;
  private grid?: THREE.GridHelper;
  private ground?: THREE.Mesh;
  /** The frame requested, or 0 while nothing is drawing: the loop runs only while something changes. */
  private frame = 0;
  /** Until when the loop draws without anything moving the camera; see `wake`. */
  private awakeUntil = 0;
  /** Whether any of the canvas is on screen: a chat scrolled past a viewer pays nothing for it. */
  private onScreen = true;
  private readonly observers: { disconnect(): void }[] = [];
  private readonly edgeRaycaster = new THREE.Raycaster();
  private readonly edgeLines: THREE.LineSegments[] = [];
  private readonly vertexRaycaster = new THREE.Raycaster();
  private readonly vertexMarkers: THREE.Points[] = [];
  /** Gold source-selected entities, separate from the pickable final topology. */
  private targetPreview?: THREE.Group;
  /** Their fat lines, which carry a screen-space width a resize invalidates. */
  private targetLines: Line2[] = [];
  /** A look's edges drawn wide, one set of screen-space lines per body, sized like the target's. */
  private wideEdges: LineSegments2[] = [];
  private hoveredEdge?: THREE.LineSegments;
  private selectedEdge?: THREE.LineSegments;
  /**
   * The hover highlight: white core, ink ring, white glow, as `cornerSprite`
   * draws for corners. `THREE.Line` cannot do it — `linewidth` is ignored by
   * every WebGL driver that matters, so the layers would come out equal width.
   */
  private hoverGlow!: Line2;
  private hoverRing!: Line2;
  private hoverCore!: Line2;
  /** Where each face's triangles sit, when the kernel said. */
  private faceRuns: FaceRun[] = [];
  private hoveredFace?: FacePick;
  /** The lit patch over the hovered face: additive, so shading still reads through it. */
  private faceHighlight?: THREE.Mesh;
  private hoveredVertex?: THREE.Points;
  private selectedVertex?: THREE.Points;
  /** Where the pointer went down, to tell a click apart from an orbit drag. */
  private pointerDownAt?: { x: number; y: number };
  /** The section plane, or none. At most one — see `setSection`. */
  private readonly clipPlanes: THREE.Plane[] = [];
  /** Stencil helpers and the cap quad. Kept out of `partGroup` on purpose. */
  private sectionGroup?: THREE.Group;
  /** The part, one entry per named body; a one-solid part is one body with no name. */
  private bodies: DrawnBody[] = [];
  private partBounds?: Bounds;
  /** Where the bodies reach over every pose `setTravel` was given; none for a part that does not move. */
  private travelBox?: THREE.Box3;
  /** Corners of the part's box as posed and the ground's, which the depth range must enclose. */
  private depthPoints: THREE.Vector3[] = [];
  private partCorners: THREE.Vector3[] = [];
  private groundCorners: THREE.Vector3[] = [];
  /** See-through: every face drawn faint and without depth, so what is inside shows. */
  private xray = false;
  /** A camera move under way, from `look`: interpolated by the render loop. */
  private flight?: Flight;
  /** Where the near plane cuts into the part: see `buildNearCut`. */
  private nearCut?: { world: THREE.Group; screen: THREE.Group };
  /** Set by the ResizeObserver; applied by the frame that then draws. */
  private pendingResize = false;

  constructor(
    private readonly container: HTMLElement,
    private readonly edgeCallbacks: EdgeCallbacks = {},
    /** False for a viewport nobody looks at: no render loop, a frame only when `snapshot` asks. */
    private live = true,
    private readonly sceneLook: SceneLook = WORKBENCH,
  ) {
    // `stencil` defaults to false since three r163, and the cap's stencil test
    // does not fail safe without a buffer: it passes over the whole viewport.
    this.renderer = new THREE.WebGLRenderer({ antialias: true, stencil: true });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.shadowMap.enabled = sceneLook.lights.some((light) => light.kind === "sun");
    this.renderer.shadowMap.type = THREE.PCFSoftShadowMap;
    // Without it, near-white clips flat and the shape reads as a hole.
    this.renderer.toneMapping = TONE_MAPPINGS[sceneLook.toneMapping];
    this.renderer.toneMappingExposure = sceneLook.exposure;
    // Per-material, not renderer-wide: a global plane would clip the cap that
    // lies *on* it.
    this.renderer.localClippingEnabled = true;
    container.appendChild(this.renderer.domElement);
    this.axes = new AxisTriad(container);

    this.scene.background =
      typeof sceneLook.sky === "string"
        ? new THREE.Color(backdrop(sceneLook.sky, sceneLook))
        : gradientTexture([backdrop(sceneLook.sky[0], sceneLook), backdrop(sceneLook.sky[1], sceneLook)]);

    this.camera = new THREE.PerspectiveCamera(38, 1, 0.1, 100000);
    // Z is up, as everywhere else in this project.
    this.camera.up.set(0, 0, 1);
    this.camera.position.set(120, -160, 110);

    this.controls = new OrbitControls(this.camera, this.renderer.domElement);
    this.controls.enableDamping = true;
    this.controls.dampingFactor = 0.12;
    // A hand on the part outranks a preset on its way.
    this.controls.addEventListener("start", () => (this.flight = undefined));
    this.controls.addEventListener("change", this.wake);
    for (const type of ["pointerdown", "pointermove", "pointerup", "pointerleave", "wheel"]) {
      this.renderer.domElement.addEventListener(type, this.wake, { passive: true });
    }

    this.renderer.domElement.addEventListener("pointermove", this.pickEntity);
    // WKWebView's WebDriver hover emits mouse events, not pointer ones.
    // `setHovered…` is idempotent, so a browser emitting both does no extra work.
    this.renderer.domElement.addEventListener("mousemove", this.pickEntity);
    this.renderer.domElement.addEventListener("pointerleave", () => {
      this.setHoveredEdge();
      this.setHoveredVertex();
    });
    // An orbit drag also ends in a `click`, wherever the pointer stopped, so
    // without the distance test orbiting to see the selection is what clears it.
    this.renderer.domElement.addEventListener("pointerdown", (event) => {
      this.pointerDownAt = { x: event.clientX, y: event.clientY };
      // An orbit that leaves the canvas would otherwise select its way across
      // whatever panel it crosses.
      const release = beginDrag();
      const end = () => {
        release();
        window.removeEventListener("pointerup", end);
        window.removeEventListener("pointercancel", end);
      };
      window.addEventListener("pointerup", end);
      window.addEventListener("pointercancel", end);
    });
    this.renderer.domElement.addEventListener("click", (event) => {
      const down = this.pointerDownAt;
      this.pointerDownAt = undefined;
      if (down && Math.hypot(event.clientX - down.x, event.clientY - down.y) > DRAG_SLOP_PX) return;
      this.setSelectedVertex(this.hoveredVertex);
      this.setSelectedEdge(this.hoveredVertex ? undefined : this.hoveredEdge);
    });

    this.scene.add(this.partGroup);
    this.scene.add(this.camera);

    // Image-based lighting does most of the work: a standard material with no
    // environment to reflect has nothing to shade *with*.
    const pmrem = new THREE.PMREMGenerator(this.renderer);
    pmrem.compileEquirectangularShader();
    this.scene.environment = pmrem.fromScene(new RoomEnvironment(), 0.04).texture;
    this.scene.environmentIntensity = sceneLook.environment;

    for (const light of sceneLook.lights) {
      if (light.kind === "ambient") {
        this.scene.add(new THREE.AmbientLight(light.color, light.intensity));
      } else if (light.kind === "sun") {
        const sun = new THREE.DirectionalLight(light.color, light.intensity);
        sun.castShadow = true;
        sun.shadow.mapSize.set(sceneLook.shadowMap, sceneLook.shadowMap);
        sun.shadow.bias = -0.0012;
        this.scene.add(sun);
        this.scene.add(sun.target);
        this.sun ??= { light: sun, from: new THREE.Vector3(...light.from).normalize() };
      } else {
        const lamp = new THREE.DirectionalLight(light.color, light.intensity);
        lamp.position.set(...light.from);
        this.camera.add(lamp);
        // A DirectionalLight aims at its *target*, which defaults to the world
        // origin — parent only the light and it goes on lighting the scene centre.
        lamp.target.position.set(...light.toward);
        this.camera.add(lamp.target);
      }
    }

    this.outline = new OutlineRenderer(this.renderer, this.scene, this.camera);
    this.outline.setColor(sceneLook.edges.color);

    const highlight = (color: number, width: number, order: number, opacity = 1) => {
      const line = new Line2(
        new LineGeometry(),
        new LineMaterial({
          color,
          linewidth: width,
          transparent: true,
          opacity,
          depthTest: false,
          depthWrite: false,
        }),
      );
      line.visible = false;
      line.renderOrder = order;
      // Never a pick target: a broad raycast would select the mark, not the edge.
      line.raycast = () => {};
      line.castShadow = false;
      line.receiveShadow = false;
      this.scene.add(line);
      return line;
    };
    this.hoverGlow = highlight(HALO, GLOW_PX, 4, 0.55);
    this.hoverRing = highlight(INK, RING_PX, 5);
    this.hoverCore = highlight(HALO, CORE_PX, 6);

    // Recorded, not applied: resizing inline leaves the render targets empty
    // until the next frame, which is the flash while dragging the splitter.
    const ro = new ResizeObserver(() => {
      this.pendingResize = true;
      this.wake();
    });
    ro.observe(container);
    const seen = new IntersectionObserver(([entry]) => {
      this.onScreen = entry.isIntersecting;
      this.wake();
    });
    seen.observe(container);
    this.observers.push(ro, seen);
    document.addEventListener("visibilitychange", this.wake);
    this.resize();
    this.wake();
  }

  /**
   * Something drawn changed: draw for the next `AWAKE_MS`, then stop until
   * something changes again. Every mutator calls it, as does every pointer,
   * wheel and camera change on the canvas.
   */
  private wake = () => {
    this.awakeUntil = performance.now() + AWAKE_MS;
    this.schedule();
  };

  /** Ask for the next frame, unless one is asked for or nobody would see it. */
  private schedule() {
    if (this.live && this.frame === 0 && this.onScreen && !document.hidden) {
      this.frame = requestAnimationFrame(this.tick);
    }
  }

  private resize() {
    const w = this.container.clientWidth;
    const h = this.container.clientHeight;
    // A pane with no size yet stays pending rather than being marked done.
    if (w === 0 || h === 0) return;
    this.pendingResize = false;
    // Let three set the CSS size too: passing `false` lays the canvas out at
    // twice its pane on a retina display.
    this.renderer.setSize(w, h);
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
    this.outline?.setSize(w, h);

    // A fat line is a screen-space quad, sized against the *drawing buffer* and
    // not the CSS box — and its width is then in device pixels, hence the ratio.
    const buffer = this.renderer.getDrawingBufferSize(new THREE.Vector2());
    const ratio = this.renderer.getPixelRatio();
    for (const [line, width] of [
      [this.hoverGlow, GLOW_PX],
      [this.hoverRing, RING_PX],
      [this.hoverCore, CORE_PX],
    ] as const) {
      const material = line?.material as LineMaterial | undefined;
      if (!material) continue;
      material.resolution.set(buffer.x, buffer.y);
      material.linewidth = width * ratio;
    }
    for (const line of this.targetLines) this.fitFatLine(line.material, TARGET_PX);
    for (const line of this.wideEdges) this.fitFatLine(line.material, this.sceneLook.edges.width!);
  }

  /**
   * A fat line is a screen-space quad: it needs the drawing buffer's size, and
   * a width in device pixels. Both callers go through here so a preview built
   * between two resizes is not sized by a second, slightly different sum.
   */
  private fitFatLine(material: LineMaterial, widthPx: number) {
    const buffer = this.renderer.getDrawingBufferSize(new THREE.Vector2());
    material.resolution.set(buffer.x, buffer.y);
    material.linewidth = widthPx * this.renderer.getPixelRatio();
  }

  private tick = () => {
    this.frame = 0;
    if (this.pendingResize) this.resize();
    if (this.flight) this.fly(this.flight);
    // True while a drag's damping is still carrying the camera, so the loop runs until it settles.
    const moving = this.controls.update();
    this.axes.update(this.camera);
    this.fitDepthRange();
    if (DEBUG === "noedge" || DEBUG === "normals") {
      this.renderer.render(this.scene, this.camera);
    } else {
      // Only the part gets edges; the grid is scenery, not geometry.
      this.outline.render([this.partGroup]);
    }
    if (moving || this.flight || performance.now() < this.awakeUntil) this.schedule();
  };

  /**
   * A PNG of what is on screen right now, scaled to fit `size`. Without
   * `preserveDrawingBuffer` the canvas is empty once a frame is presented, so
   * the draw and the copy have to be synchronous and adjacent.
   *
   * A pane with no layout — a hidden webview, or a capture before the first
   * layout — has never sized the renderer, which leaves three's default
   * 300 × 150 canvas under a camera whose aspect is still 1, and a copy of
   * that is the part squashed two to one. Such a thumbnail is drawn at a size
   * of its own, and the renderer put back afterwards.
   */
  snapshot(size = 512): string {
    const canvas = this.renderer.domElement;
    const laidOut = this.container.clientWidth > 0 && this.container.clientHeight > 0;
    let restore: (() => void) | undefined;
    if (!laidOut) {
      const ratio = this.renderer.getPixelRatio();
      const was = { w: canvas.width / ratio, h: canvas.height / ratio, aspect: this.camera.aspect };
      const w = size;
      const h = Math.round((size * 3) / 4);
      this.renderer.setSize(w, h, false);
      this.outline?.setSize(w, h);
      this.camera.aspect = w / h;
      this.camera.updateProjectionMatrix();
      restore = () => {
        this.renderer.setSize(was.w, was.h, false);
        this.outline?.setSize(was.w, was.h);
        this.camera.aspect = was.aspect;
        this.camera.updateProjectionMatrix();
        this.pendingResize = true;
        this.wake();
      };
    }
    this.tickOnce();
    if (!canvas.width || !canvas.height) {
      restore?.();
      return "";
    }

    const scale = Math.min(1, size / Math.max(canvas.width, canvas.height));
    const out = document.createElement("canvas");
    out.width = Math.max(1, Math.round(canvas.width * scale));
    out.height = Math.max(1, Math.round(canvas.height * scale));
    const context = out.getContext("2d");
    const png = context ? (context.drawImage(canvas, 0, 0, out.width, out.height), out.toDataURL("image/png")) : "";
    restore?.();
    return png;
  }

  /** One frame, outside the animation loop, for `snapshot` to read. */
  private tickOnce() {
    if (this.pendingResize) this.resize();
    this.fitDepthRange();
    if (DEBUG === "noedge" || DEBUG === "normals") {
      this.renderer.render(this.scene, this.camera);
    } else {
      this.outline.render([this.partGroup]);
    }
  }

  dispose() {
    cancelAnimationFrame(this.frame);
    this.live = false;
    for (const observer of this.observers) observer.disconnect();
    document.removeEventListener("visibilitychange", this.wake);
    this.renderer.domElement.removeEventListener("pointermove", this.pickEntity);
    this.renderer.domElement.removeEventListener("mousemove", this.pickEntity);
    this.outline.dispose();
    this.renderer.dispose();
  }

  /** Find the visible B-rep entity beneath the pointer. */
  private pickEntity = (event: PointerEvent | MouseEvent) => {
    // A finger has no hover: its moves are an orbit, and would leave an edge lit.
    if (this.edgeLines.length === 0 || ("pointerType" in event && event.pointerType === "touch")) return;

    const rect = this.renderer.domElement.getBoundingClientRect();
    const pointer = new THREE.Vector2(
      ((event.clientX - rect.left) / rect.width) * 2 - 1,
      -((event.clientY - rect.top) / rect.height) * 2 + 1,
    );
    this.edgeRaycaster.setFromCamera(pointer, this.camera);
    // Nothing nearer than the near plane is drawn, so nothing there is picked.
    this.edgeRaycaster.near = this.camera.near;
    this.vertexRaycaster.near = this.camera.near;

    // The hit radius is in screen pixels, not world units, so it means the same
    // on a large part and a small one.
    const distance = this.camera.position.distanceTo(this.controls.target);
    const visibleHeight = 2 * Math.tan((this.camera.fov * Math.PI) / 360) * distance;
    this.edgeRaycaster.params.Line!.threshold = (visibleHeight / rect.height) * 12;
    this.vertexRaycaster.params.Points!.threshold = (visibleHeight / rect.height) * 9;

    this.vertexRaycaster.setFromCamera(pointer, this.camera);

    // Only what is visible is pickable: the lists cover the far side too, and
    // the marks are drawn without a depth test, so nothing else enforces it.
    const surface = this.visibleSurface(this.edgeRaycaster);
    const near = (surface?.distance ?? Infinity) + (visibleHeight / rect.height) * 12;
    const visible = <T extends THREE.Intersection>(hit?: T) =>
      hit && hit.distance <= near ? hit : undefined;

    // A raycaster tests layers, not `visible`: a hidden body's edges are still in the lists.
    const vertexHit = visible(this.vertexRaycaster.intersectObjects(this.vertexMarkers.filter(shown), false)[0]);
    const edgeHit = visible(this.edgeRaycaster.intersectObjects(this.edgeLines.filter(shown), false)[0]);
    const vertex = vertexHit?.object as THREE.Points | undefined;
    this.setHoveredVertex(vertex);
    const edge = vertex ? undefined : (edgeHit?.object as THREE.LineSegments | undefined);
    this.setHoveredEdge(edge);

    // Corners beat edges beat faces: the smaller the entity, the more specific
    // the thing you meant by pointing at it.
    if (vertex || edge || this.bodies.length === 0 || this.faceRuns.length === 0) {
      this.setHoveredFace(undefined);
      return;
    }
    const triangle = surface?.faceIndex;
    this.setHoveredFace(
      triangle === undefined || triangle === null ? undefined : this.faceAt(triangle),
    );
  };

  /**
   * A raycaster ignores clipping planes, so skip hits a section has cut away.
   * Every body's mesh shares one index buffer, so a hit's `faceIndex` is the
   * part's own triangle number whichever body it is on.
   */
  private visibleSurface(raycaster: THREE.Raycaster): THREE.Intersection | undefined {
    const meshes = this.bodies.filter((body) => body.group.visible).map((body) => body.mesh);
    for (const hit of raycaster.intersectObjects(meshes, false)) {
      if (this.clipPlanes.every((plane) => plane.distanceToPoint(hit.point) >= 0)) return hit;
    }
    return undefined;
  }

  /** The body a triangle of the part is on. */
  private bodyOf(triangle: number): DrawnBody | undefined {
    return this.bodies.find((body) => triangle >= body.start && triangle < body.start + body.count);
  }

  /** Which face a triangle belongs to, by the runs the kernel reported. */
  private faceAt(triangle: number): FacePick | undefined {
    for (const run of this.faceRuns) {
      if (triangle >= run.start && triangle < run.start + run.count) {
        return { face: run.face, run, triangles: run.count };
      }
    }
    return undefined;
  }

  private setHoveredFace(next?: FacePick) {
    if (next?.face === this.hoveredFace?.face) return;
    this.hoveredFace = next;
    this.showFaceHighlight(next);
    this.wake();
    this.edgeCallbacks.onFaceHover?.(next);
  }

  /** Lay a lit patch over the hovered face: a copy of its triangles, not a second material on the part. */
  private showFaceHighlight(face?: FacePick) {
    if (this.faceHighlight) {
      this.faceHighlight.parent?.remove(this.faceHighlight);
      this.faceHighlight.geometry.dispose();
      this.faceHighlight = undefined;
    }
    const body = face && this.bodyOf(face.run.start);
    if (!face || !body) return;

    const source = body.mesh.geometry;
    const index = source.getIndex();
    if (!index) return;

    const patch = new THREE.BufferGeometry();
    patch.setAttribute("position", source.getAttribute("position"));
    patch.setIndex(
      new THREE.Uint32BufferAttribute(
        Array.from(index.array).slice(face.run.start * 3, (face.run.start + face.run.count) * 3),
        1,
      ),
    );

    const mesh = new THREE.Mesh(
      patch,
      new THREE.MeshBasicMaterial({
        color: HALO,
        transparent: true,
        opacity: 0.22,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
        // Coplanar with the face it covers, so it z-fights without this.
        polygonOffset: true,
        polygonOffsetFactor: -4,
        polygonOffsetUnits: -4,
        side: THREE.DoubleSide,
        clippingPlanes: this.clipPlanes.length > 0 ? this.clipPlanes : null,
      }),
    );
    mesh.renderOrder = 3;
    mesh.castShadow = false;
    mesh.receiveShadow = false;
    mesh.raycast = () => {};

    // `EdgesGeometry` drops near-coplanar edges, which is what leaves the
    // patch boundary — except onto a *tangent* neighbour, where there is no
    // angle to find and the tint alone has to carry the highlight.
    const outline = new THREE.LineSegments(
      new THREE.EdgesGeometry(patch, 25),
      new THREE.LineBasicMaterial({
        color: HALO,
        transparent: true,
        opacity: 0.85,
        depthWrite: false,
        clippingPlanes: this.clipPlanes.length > 0 ? this.clipPlanes : null,
      }),
    );
    outline.renderOrder = 4;
    outline.raycast = () => {};
    mesh.add(outline);

    this.faceHighlight = mesh;
    body.mesh.add(mesh);
  }

  private setHoveredEdge(next?: THREE.LineSegments) {
    if (next === this.hoveredEdge) return;
    const previous = this.hoveredEdge;
    this.hoveredEdge = next;
    this.paintEdge(previous);
    this.paintEdge(next);
    this.showEdgeHighlight(next);
    this.wake();
    this.edgeCallbacks.onHover?.(next?.userData.edge as EdgeCurve | undefined);
  }

  private setSelectedEdge(next?: THREE.LineSegments) {
    if (next === this.selectedEdge) return;
    const previous = this.selectedEdge;
    this.selectedEdge = next;
    this.paintEdge(previous);
    this.paintEdge(next);
    this.wake();
    this.edgeCallbacks.onSelect?.(next?.userData.edge as EdgeCurve | undefined);
  }

  /** Gold when pinned. Hover is drawn by the highlight instead, so the edge keeps looking like an edge. */
  private paintEdge(line?: THREE.LineSegments) {
    if (!line) return;
    const material = line.material as THREE.LineBasicMaterial;
    material.color.set(line === this.selectedEdge ? goldHex() : this.sceneLook.edges.color);
    material.opacity = line === this.selectedEdge ? 1 : this.sceneLook.edges.opacity;
  }

  /** Lay the three layers over the hovered edge. Rebuilt per hover, not kept per edge. */
  private showEdgeHighlight(line?: THREE.LineSegments) {
    const edge = line?.userData.edge as EdgeCurve | undefined;
    if (!edge || edge.points.length < 2) {
      for (const target of [this.hoverGlow, this.hoverRing, this.hoverCore]) {
        target.visible = false;
      }
      return;
    }
    // The highlight lives on the scene, so a posed body's edge is posed here.
    line!.updateWorldMatrix(true, false);
    const flat = edge.points.flatMap((p) => scratch.fromArray(p).applyMatrix4(line!.matrixWorld).toArray());
    for (const target of [this.hoverGlow, this.hoverRing, this.hoverCore]) {
      const geometry = new LineGeometry();
      geometry.setPositions(flat);
      target.geometry.dispose();
      target.geometry = geometry;
      target.computeLineDistances();
      target.visible = true;
    }
  }

  private setHoveredVertex(next?: THREE.Points) {
    if (next === this.hoveredVertex) return;
    const previous = this.hoveredVertex;
    this.hoveredVertex = next;
    this.paintVertex(previous);
    this.paintVertex(next);
    this.wake();
    this.edgeCallbacks.onVertexHover?.(next?.userData.vertex as VertexPoint | undefined);
  }

  private setSelectedVertex(next?: THREE.Points) {
    if (next === this.selectedVertex) return;
    const previous = this.selectedVertex;
    this.selectedVertex = next;
    this.paintVertex(previous);
    this.paintVertex(next);
    this.wake();
    this.edgeCallbacks.onVertexSelect?.(next?.userData.vertex as VertexPoint | undefined);
  }

  /**
   * A corner is drawn only while it is pointed at. `material.visible` rather
   * than `object.visible`: the raycaster still sees the object, so a corner you
   * cannot see is still a corner you can hover.
   */
  private paintVertex(marker?: THREE.Points) {
    if (!marker) return;
    const shown = marker === this.hoveredVertex || marker === this.selectedVertex;
    marker.material =
      marker === this.selectedVertex
        ? vertexMaterial("selected")
        : shown
          ? vertexMaterial("hover")
          : vertexMaterial("hidden");
  }

  /** Replace the displayed part. */
  setGeometry(geo: Geometry, bounds: Bounds) {
    this.clearPart();

    const g = new THREE.BufferGeometry();
    g.setAttribute("position", new THREE.BufferAttribute(geo.positions, 3));
    g.setAttribute("normal", new THREE.BufferAttribute(geo.normals, 3));
    // Expanded triangles carry per-corner normals, so there is nothing to index.
    if (geo.indices.length > 0) {
      g.setIndex(new THREE.BufferAttribute(geo.indices, 1));
    }
    g.computeBoundingSphere();
    const material =
      DEBUG === "normals" ? new THREE.MeshNormalMaterial() : dressFaces(g, geo, this.sceneLook, this.scene.environment);
    const triangles = (geo.indices.length || geo.positions.length / 3) / 3;
    const spans = geo.bodies?.length ? geo.bodies : [{ name: "", start: 0, count: triangles }];
    this.partBounds = bounds;

    for (const span of spans) {
      // One body's triangles: the part's own buffers, drawn over its span.
      // Sharing the index keeps a hit's `faceIndex` the part's triangle number.
      const own = spans.length === 1 ? g : new THREE.BufferGeometry();
      if (own !== g) {
        own.setAttribute("position", g.getAttribute("position"));
        own.setAttribute("normal", g.getAttribute("normal"));
        own.setIndex(g.getIndex());
        for (const group of g.groups) own.addGroup(group.start, group.count, group.materialIndex);
        own.setDrawRange(span.start * 3, span.count * 3);
        own.boundingBox = spanBox(geo, span);
        own.boundingSphere = own.boundingBox.getBoundingSphere(new THREE.Sphere());
      }
      // A body's own colour is its default slot; what a face wears itself stays.
      const dressed =
        !("color" in span && span.color) || DEBUG === "normals" ? material
        : Array.isArray(material) ? [solidMaterial(this.sceneLook, span.color), ...material.slice(1)]
        : solidMaterial(this.sceneLook, span.color);
      const mesh = new THREE.Mesh(own, dressed);
      mesh.castShadow = true;
      // Cast onto the ground, never onto itself: a hard shadow of a fin across
      // its own base plate reads as a step that is not there. Fusion does likewise.
      mesh.receiveShadow = false;

      // Posed by `setBodyPose`, which writes this matrix and nothing else.
      const group = new THREE.Group();
      group.matrixAutoUpdate = false;
      group.add(mesh);

      // Not `EdgesGeometry`: these are the kernel's own curves, sampled. A closed
      // smooth solid — a sphere, a torus — has none a person would count, and
      // draws as the silhouette alone.
      const edges = (geo.edges ?? []).filter((edge) => spans.length === 1 || edge.body === span.name);
      const renderedEdges = edgeLines(edges, this.sceneLook.edges);
      group.add(renderedEdges.group);
      this.edgeLines.push(...renderedEdges.lines);
      if (renderedEdges.wide) {
        this.fitFatLine(renderedEdges.wide.material, this.sceneLook.edges.width!);
        this.wideEdges.push(renderedEdges.wide);
      }
      const renderedVertices = vertexMarkers(verticesFromEdges(edges));
      group.add(renderedVertices.group);
      this.vertexMarkers.push(...renderedVertices.markers);

      this.partGroup.add(group);
      this.bodies.push({
        name: span.name,
        start: span.start,
        count: span.count,
        group,
        mesh,
        box: own.boundingBox ?? spanBox(geo, span),
      });
    }
    this.faceRuns = geo.faceRuns ?? [];
    if (this.xray) this.dressXray();
    // Stop guessing at creases once told where they are. The silhouette half of
    // the pass stays on: it is a property of the view, so no kernel supplies it.
    this.outline.setCreases(false);

    this.layGround();
    this.fitPosedBox();

    // A new part keeps the plane the last one was cut on: the reason the
    // section is open is to watch one internal feature change.
    this.applyClipping();
    this.buildCap(this.section);
    this.buildNearCut();
    this.wake();
  }

  /** The plane currently cut, if any — `setSection`'s argument, remembered. */
  private section?: SectionPlane;

  /** Cut the part open on a plane, or stop. The window's half of `evaluate_part`'s `section`. */
  setSection(section?: SectionPlane) {
    this.section = section;
    this.clipPlanes.length = 0;
    if (section) {
      // three keeps dot(normal, p) + constant > 0, so the normal points *into*
      // the material that survives.
      const sign = section.keep === "below" ? -1 : 1;
      const normal = new THREE.Vector3(
        section.axis === "x" ? sign : 0,
        section.axis === "y" ? sign : 0,
        section.axis === "z" ? sign : 0,
      );
      this.clipPlanes.push(new THREE.Plane(normal, -sign * section.atMm));
    }

    this.applyClipping();
    this.buildCap(section);
    this.buildNearCut();
    this.outline.setClippingPlanes(this.clipPlanes);
    this.wake();
  }

  /**
   * Put a named body in a pose: a row-major 4×4 from the body as drawn, the
   * form a report's `worst_pose` takes. None puts it back. The exact mesh moves
   * whole; nothing is re-tessellated or approximated.
   */
  setBodyPose(name: string, pose?: number[]) {
    const body = this.bodies.find((b) => b.name === name);
    if (!body) return;
    if (pose) body.group.matrix.fromArray(pose).transpose();
    else body.group.matrix.identity();
    // The hover marks are drawn in the scene, at the pose they were made at.
    this.setHoveredEdge();
    this.setHoveredVertex();
    this.bodiesChanged();
  }

  /**
   * Every pose each body passes through as the part's motions play, row-major
   * 4×4s as `setBodyPose` takes them. The ground goes under the lowest point
   * any body reaches in any of them and the sun's shadow covers all of them,
   * so neither moves while a mechanism plays. Measured on the mesh's own
   * vertices; a body with no poses counts as drawn.
   */
  setTravel(travel: ReadonlyMap<string, readonly number[][]>) {
    const box = new THREE.Box3();
    const matrix = new THREE.Matrix4();
    for (const body of this.bodies) {
      const poses = travel.get(body.name);
      if (!poses?.length) continue;
      let points: Float32Array | undefined;
      for (const pose of poses) {
        matrix.fromArray(pose).transpose();
        // A pose that only carries the body along carries its box: no vertex reaches past it.
        const reach = isTranslation(matrix) ? body.box.clone().applyMatrix4(matrix) : reachOf((points ??= bodyPoints(body)), matrix);
        box.union(reach);
      }
    }
    this.travelBox = box.isEmpty() ? undefined : box;
    if (!this.partBounds) return;
    this.layGround();
    this.fitPosedBox();
    this.wake();
  }

  /** Hide a named body, or show it again. Hidden is hidden from picking and the section too. */
  setBodyHidden(name: string, hidden: boolean) {
    const body = this.bodies.find((b) => b.name === name);
    if (!body || body.group.visible === !hidden) return;
    body.group.visible = !hidden;
    this.setHoveredEdge();
    this.setHoveredVertex();
    this.setHoveredFace();
    this.bodiesChanged();
  }

  /** Draw every face faint and let the edges show through, or go back to solid. */
  setXray(on: boolean) {
    if (on === this.xray) return;
    this.xray = on;
    this.dressXray();
    this.wake();
  }

  /**
   * Look at what is shown from `from`, a direction toward the camera, filling
   * the view with the bodies as posed: flown to, unless `fly` is false. A drag
   * on the way cancels the flight.
   */
  look(from: readonly [number, number, number], fly = true) {
    const box = this.posedBox(true);
    if (box.isEmpty()) return;
    const goal = this.fitting(box, new THREE.Vector3(...from).normalize());
    this.flight = {
      startedAt: fly ? performance.now() : -FLIGHT_MS,
      from: { target: this.controls.target.clone(), offset: this.camera.position.clone().sub(this.controls.target) },
      to: { target: goal.target, offset: goal.position.sub(goal.target) },
    };
    if (!fly) {
      this.fly(this.flight);
      this.controls.update();
    }
    this.wake();
  }

  /**
   * Where the camera stands to fill the view with `box` seen from `from`: the
   * nearest distance at which every corner is inside the view, rather than a
   * sphere around the box, which leaves a flat part small seen square-on.
   */
  private fitting(box: THREE.Box3, from: THREE.Vector3): { position: THREE.Vector3; target: THREE.Vector3 } {
    const target = box.getCenter(new THREE.Vector3());
    const across = new THREE.Vector3().crossVectors(this.camera.up, from);
    if (across.lengthSq() < 1e-9) across.set(1, 0, 0);
    across.normalize();
    const up = new THREE.Vector3().crossVectors(from, across);
    const halfHigh = Math.tan((this.camera.fov * Math.PI) / 360) / FIT_MARGIN;
    const halfWide = halfHigh * this.camera.aspect;
    let distance = 1e-3;
    for (const corner of boxCorners(box)) {
      corner.sub(target);
      const off = Math.max(Math.abs(corner.dot(across)) / halfWide, Math.abs(corner.dot(up)) / halfHigh);
      distance = Math.max(distance, corner.dot(from) + off);
    }
    return { position: target.clone().addScaledVector(from, distance), target };
  }

  private fly(flight: Flight) {
    const t = Math.min(1, (performance.now() - flight.startedAt) / FLIGHT_MS);
    const eased = t * t * (3 - 2 * t);
    this.controls.target.lerpVectors(flight.from.target, flight.to.target, eased);
    this.camera.position.copy(this.controls.target).add(sphericalLerp(flight.from.offset, flight.to.offset, eased));
    if (t === 1) this.flight = undefined;
  }

  /** Every material a body is drawn with, x-rayed or put back as it was dressed. */
  private dressXray() {
    for (const body of this.bodies) {
      const material = body.mesh.material;
      for (const m of Array.isArray(material) ? material : [material]) {
        const solid = (m.userData.solid ??= { transparent: m.transparent, opacity: m.opacity, depthWrite: m.depthWrite });
        m.transparent = this.xray || solid.transparent;
        m.opacity = this.xray ? solid.opacity * XRAY_OPACITY : solid.opacity;
        m.depthWrite = this.xray ? false : solid.depthWrite;
        m.needsUpdate = true;
      }
    }
  }

  /** The bodies' boxes as posed, shown ones only when asked: what to frame and what to keep in depth. */
  private posedBox(shownOnly = false): THREE.Box3 {
    const box = new THREE.Box3();
    for (const body of this.bodies) {
      if (shownOnly && !body.group.visible) continue;
      box.union(body.box.clone().applyMatrix4(body.group.matrix));
    }
    return box;
  }

  /** After a pose or a visibility changes: what follows the bodies, and what is sized to them. */
  private bodiesChanged() {
    for (const body of this.bodies) body.group.matrixWorldNeedsUpdate = true;
    for (const holder of [this.sectionGroup, this.nearCut?.world]) {
      holder?.children.forEach((object) => {
        const body = object.userData.follows as DrawnBody | undefined;
        if (!body) return;
        object.visible = body.group.visible;
        object.matrixWorldNeedsUpdate = true;
      });
    }
    this.fitPosedBox();
    this.wake();
  }

  /** The depth range and the sun, around the part as it now stands. */
  private fitPosedBox() {
    const box = this.posedBox();
    if (box.isEmpty()) return;
    this.partCorners = boxCorners(box);
    this.depthPoints = [...this.partCorners, ...this.groundCorners];
    // Over the whole travel too: a sun re-aimed at every pose shifts the shadow map's texels as a mechanism plays.
    const lit = this.travelBox ? box.clone().union(this.travelBox) : box;
    this.aimSun({ min: lit.min, max: lit.max }, lit.getSize(new THREE.Vector3()));
  }

  /**
   * Draw `object`, made from `body`'s triangles somewhere outside its group,
   * in the body's pose: the matrix itself is shared, so `setBodyPose` moves it.
   */
  private follow(object: THREE.Object3D, body: DrawnBody) {
    object.matrixAutoUpdate = false;
    object.matrix = body.group.matrix;
    object.userData.follows = body;
    object.visible = body.group.visible;
  }

  /**
   * Fit near and far to the part and the ground, from wherever the camera is
   * now. Fixed once at framing, the far plane cut the part off as soon as it
   * was zoomed out or grew, and the near plane sliced it on zooming in.
   */
  private fitDepthRange() {
    if (this.depthPoints.length === 0) return;
    this.camera.updateMatrixWorld();
    const view = this.camera.matrixWorldInverse;
    const depthOf = (p: THREE.Vector3) => -scratch.copy(p).applyMatrix4(view).z;
    let nearest = Infinity;
    let farthest = 0;
    for (const p of this.depthPoints) {
      const depth = depthOf(p);
      nearest = Math.min(nearest, depth);
      farthest = Math.max(farthest, depth);
    }
    const far = Math.max(farthest * 1.05, 1e-3);
    const near = Math.max(nearest * 0.95, far / DEPTH_RANGE);
    if (near !== this.camera.near || far !== this.camera.far) {
      this.camera.near = near;
      this.camera.far = far;
      this.camera.updateProjectionMatrix();
    }
    if (this.nearCut) {
      const cuts = this.partCorners.some((p) => depthOf(p) <= near);
      this.nearCut.world.visible = cuts;
      this.nearCut.screen.visible = cuts;
      // Just past the near plane, and a little larger than the frustum there.
      const at = near * 1.01;
      const height = 2 * at * Math.tan((this.camera.fov * Math.PI) / 360) * 1.05;
      this.nearCut.screen.position.set(0, 0, -at);
      this.nearCut.screen.scale.set(height * this.camera.aspect, height, 1);
    }
  }

  /**
   * Where the near plane cuts into the part, draw what it cut through instead
   * of a hole: the walls behind as a ghost of the part's own material, under a
   * veil of it. The stencil counts crossings as `buildCap` does, but on the
   * uncut part, so a count that does not cancel means the near plane is inside
   * material. Drawn only while the near plane reaches the part's box.
   */
  private buildNearCut() {
    if (this.nearCut) {
      this.scene.remove(this.nearCut.world);
      this.camera.remove(this.nearCut.screen);
      disposeTree(this.nearCut.world);
      disposeTree(this.nearCut.screen);
      this.nearCut.screen.children.forEach((q) => (q as THREE.Mesh).geometry.dispose());
      this.nearCut = undefined;
    }
    if (this.bodies.length === 0 || DEBUG === "normals") return;
    const clipping = this.clipPlanes.length > 0 ? this.clipPlanes : null;
    // Transparent, all of it, so it sorts after the part is drawn; ordered
    // after the edges and before the marks, which stay unveiled.
    const order = { clear: 1.2, count: 1.4, walls: 1.6, veil: 1.8 };

    const world = new THREE.Group();
    for (const [side, op] of [
      [THREE.BackSide, THREE.IncrementWrapStencilOp],
      [THREE.FrontSide, THREE.DecrementWrapStencilOp],
    ] as const) {
      const counting = new THREE.MeshBasicMaterial({
        side,
        transparent: true,
        colorWrite: false,
        depthWrite: false,
        depthTest: false,
        stencilWrite: true,
        stencilFunc: THREE.AlwaysStencilFunc,
        stencilFail: op,
        stencilZFail: op,
        stencilZPass: op,
      });
      for (const body of this.bodies) {
        const counter = new THREE.Mesh(body.mesh.geometry, counting);
        counter.renderOrder = order.count;
        this.follow(counter, body);
        world.add(counter);
      }
    }

    const insideOnly = (m: THREE.Material, opacity: number) => {
      m.transparent = true;
      m.opacity *= opacity;
      m.depthWrite = false;
      m.clippingPlanes = clipping;
      m.stencilWrite = true;
      m.stencilRef = 0;
      m.stencilFunc = THREE.NotEqualStencilFunc;
      m.stencilFail = THREE.KeepStencilOp;
      m.stencilZFail = THREE.KeepStencilOp;
      m.stencilZPass = THREE.KeepStencilOp;
      return m;
    };
    // Every body is dressed in the same material, so one set of walls serves all.
    const own = this.bodies[0].mesh.material;
    const wallOf = (m: THREE.Material) => {
      const wall = insideOnly(m.clone(), CUT_WALL_OPACITY);
      wall.side = THREE.BackSide;
      return wall;
    };
    const wallMaterial = Array.isArray(own) ? own.map(wallOf) : wallOf(own);
    for (const body of this.bodies) {
      const walls = new THREE.Mesh(body.mesh.geometry, wallMaterial);
      walls.renderOrder = order.walls;
      this.follow(walls, body);
      world.add(walls);
    }

    const screen = new THREE.Group();
    const quad = new THREE.PlaneGeometry(1, 1);
    const clear = new THREE.Mesh(
      quad,
      new THREE.MeshBasicMaterial({
        transparent: true,
        colorWrite: false,
        depthWrite: false,
        depthTest: false,
        stencilWrite: true,
        stencilRef: 0,
        stencilFunc: THREE.AlwaysStencilFunc,
        stencilFail: THREE.ReplaceStencilOp,
        stencilZFail: THREE.ReplaceStencilOp,
        stencilZPass: THREE.ReplaceStencilOp,
      }),
    );
    clear.renderOrder = order.clear;
    const base = Array.isArray(own) ? own[0] : own;
    const veilMaterial = insideOnly(base.clone(), CUT_VEIL_OPACITY);
    veilMaterial.depthTest = false;
    const veil = new THREE.Mesh(quad, veilMaterial);
    veil.renderOrder = order.veil;
    for (const q of [clear, veil]) {
      q.frustumCulled = false;
      q.raycast = () => {};
      screen.add(q);
    }
    for (const object of [...world.children, ...screen.children]) {
      object.castShadow = false;
      object.receiveShadow = false;
    }
    world.children.forEach((o) => (o.raycast = () => {}));

    world.visible = false;
    screen.visible = false;
    this.scene.add(world);
    this.camera.add(screen);
    this.nearCut = { world, screen };
  }

  /** Which half to keep so the cut faces the camera. `Section::resolve`, with an orbiting camera. */
  keepFacingCamera(axis: "x" | "y" | "z"): "below" | "above" {
    const toward = this.camera.position.clone().sub(this.controls.target);
    return toward[axis] > 0 ? "below" : "above";
  }

  /** Hand the current plane to everything the part is drawn with. */
  private applyClipping() {
    const planes = this.clipPlanes.length > 0 ? this.clipPlanes : null;
    this.partGroup.traverse((object) => {
      const drawable = object as THREE.Mesh | THREE.LineSegments;
      const material = drawable.material;
      if (!material) return;
      for (const m of Array.isArray(material) ? material : [material]) {
        const before = m.clippingPlanes?.length ?? 0;
        m.clippingPlanes = planes;
        // Shadow the half that is left, not the half that went.
        m.clipShadows = true;
        // The plane count is compiled *into* the shader, so without this an
        // existing program keeps its clip-free version and the section quietly
        // does not cut.
        if (before !== (planes?.length ?? 0)) m.needsUpdate = true;
      }
    });
  }

  /**
   * Fill the cut with a flat face, or clipping leaves a hollow shell that reads
   * as a thin cup. Back faces increment the stencil and front faces decrement
   * it, both invisible and both clipped, so a count that does not cancel means
   * the ray crossed the plane inside material — and the quad paints there.
   */
  private buildCap(section?: SectionPlane) {
    if (this.sectionGroup) {
      this.scene.remove(this.sectionGroup);
      disposeTree(this.sectionGroup);
      this.sectionGroup = undefined;
    }
    if (!section || this.bodies.length === 0 || !this.partBounds) return;

    const group = new THREE.Group();

    for (const [side, op] of [
      [THREE.BackSide, THREE.IncrementWrapStencilOp],
      [THREE.FrontSide, THREE.DecrementWrapStencilOp],
    ] as const) {
      const material = new THREE.MeshBasicMaterial({
        side,
        clippingPlanes: this.clipPlanes,
        // Counting, not drawing: a crossing counts wherever it is, not only
        // where it is nearest.
        colorWrite: false,
        depthWrite: false,
        depthTest: false,
        stencilWrite: true,
        stencilFunc: THREE.AlwaysStencilFunc,
        stencilFail: op,
        stencilZFail: op,
        stencilZPass: op,
      });
      for (const body of this.bodies) {
        const counter = new THREE.Mesh(body.mesh.geometry, material);
        counter.renderOrder = -2;
        this.follow(counter, body);
        group.add(counter);
      }
    }

    const size = Math.max(
      this.partBounds.max.x - this.partBounds.min.x,
      this.partBounds.max.y - this.partBounds.min.y,
      this.partBounds.max.z - this.partBounds.min.z,
    );
    const cap = new THREE.Mesh(
      new THREE.PlaneGeometry(size * 3, size * 3),
      new THREE.MeshBasicMaterial({
        color: CUT_FACE,
        side: THREE.DoubleSide,
        stencilWrite: true,
        stencilRef: 0,
        stencilFunc: THREE.NotEqualStencilFunc,
        // Reset as it draws, so the next frame starts from a clean buffer.
        stencilFail: THREE.ReplaceStencilOp,
        stencilZFail: THREE.ReplaceStencilOp,
        stencilZPass: THREE.ReplaceStencilOp,
      }),
    );
    // A quad in XY facing +Z, turned to face along the section axis.
    if (section.axis === "x") cap.rotateY(Math.PI / 2);
    if (section.axis === "y") cap.rotateX(Math.PI / 2);
    const centre = {
      x: (this.partBounds.min.x + this.partBounds.max.x) / 2,
      y: (this.partBounds.min.y + this.partBounds.max.y) / 2,
      z: (this.partBounds.min.z + this.partBounds.max.z) / 2,
    };
    cap.position.set(centre.x, centre.y, centre.z);
    cap.position[section.axis] = section.atMm;
    cap.renderOrder = -1;
    cap.castShadow = false;
    cap.receiveShadow = false;
    group.add(cap);

    // In the scene, not `partGroup`: the outline pass hides meshes it was not
    // asked to outline, and these are the *unclipped* part.
    this.sectionGroup = group;
    this.scene.add(group);
  }

  /** Overlay exact pre-treatment entities selected from the editor source. */
  setTargetPreview(edges: EdgeCurve[], vertices: TargetVertex[] = []) {
    this.clearTargetPreview();
    if (edges.length === 0 && vertices.length === 0) return;

    const group = new THREE.Group();
    for (const edge of edges) {
      const geometry = new LineGeometry();
      geometry.setPositions(edge.points.flat());
      const line = new Line2(
        geometry,
        new LineMaterial({
          color: goldHex(),
          depthTest: false,
          depthWrite: false,
        }),
      );
      this.fitFatLine(line.material, TARGET_PX);
      // Never a pick target: this is the source's selection, not the topology.
      line.raycast = () => {};
      line.renderOrder = 2;
      this.targetLines.push(line);
      group.add(line);
    }

    if (vertices.length > 0) group.add(targetVertexMarkers(vertices));
    this.targetPreview = group;
    this.partGroup.add(group);
    this.wake();
  }

  private clearTargetPreview() {
    // Emptied first: `resize` walks this list, and a disposed material is not
    // one to be setting a resolution on.
    this.targetLines.length = 0;
    if (!this.targetPreview) return;
    this.partGroup.remove(this.targetPreview);
    disposeTree(this.targetPreview);
    this.targetPreview = undefined;
    this.wake();
  }

  /** Show nothing, without disturbing the camera. */
  clearPart() {
    this.clearTargetPreview();
    if (this.sectionGroup) {
      this.scene.remove(this.sectionGroup);
      disposeTree(this.sectionGroup);
      this.sectionGroup = undefined;
    }
    this.bodies = [];
    this.travelBox = undefined;
    this.flight = undefined;
    this.buildNearCut();
    this.hoveredEdge = undefined;
    this.selectedEdge = undefined;
    this.hoveredVertex = undefined;
    this.selectedVertex = undefined;
    this.edgeLines.length = 0;
    this.wideEdges.length = 0;
    this.vertexMarkers.length = 0;
    this.faceRuns = [];
    this.hoveredFace = undefined;
    this.faceHighlight = undefined;
    // The highlight lives on the scene, not in the part group, so it survives
    // the sweep below and has to be put away by hand.
    this.showEdgeHighlight(undefined);
    this.edgeCallbacks.onHover?.(undefined);
    this.edgeCallbacks.onSelect?.(undefined);
    this.edgeCallbacks.onVertexHover?.(undefined);
    this.edgeCallbacks.onVertexSelect?.(undefined);
    for (const child of [...this.partGroup.children]) {
      this.partGroup.remove(child);
      child.traverse((o) => {
        const drawable = o as THREE.Mesh | THREE.LineSegments;
        drawable.geometry?.dispose();
        releaseMaterial(drawable.material);
      });
    }
    this.wake();
  }

  /**
   * The ground under everything drawn — the part, its references, and
   * everywhere its travel takes it — and the corners the depth range keeps.
   */
  private layGround() {
    const { min, max } = this.partBounds!;
    const box = new THREE.Box3(new THREE.Vector3(min.x, min.y, min.z), new THREE.Vector3(max.x, max.y, max.z));
    // A reference is often what the part stands among: walls that reach the floor below a shelf.
    for (const body of this.bodies) box.union(body.box);
    if (this.travelBox) box.union(this.travelBox);
    const size = box.getSize(new THREE.Vector3());
    this.placeGround(box, size);
    // The ground's span, grid or not: three times what it lies under, as `placeGround` lays it.
    const reach = Math.max(size.x, size.y, size.z) * 1.5;
    const middle = new THREE.Vector3((box.min.x + box.max.x) / 2, (box.min.y + box.max.y) / 2, box.min.z);
    this.groundCorners = boxCorners(
      new THREE.Box3(middle.clone().add(new THREE.Vector3(-reach, -reach, 0)), middle.clone().add(new THREE.Vector3(reach, reach, 0))),
    );
  }

  /** Grid and shadow-catcher at the bottom of what is drawn: the contact shadow is what says where it sits. */
  private placeGround(bounds: Bounds, size: { x: number; y: number; z: number }) {
    const extent = Math.max(size.x, size.y, size.z);
    const step = niceStep(extent / 10);
    const span = Math.max(step * 20, extent * 3);
    const z = bounds.min.z;

    if (this.grid) {
      this.scene.remove(this.grid);
      this.grid.dispose();
      this.grid = undefined;
    }
    const grid = this.sceneLook.grid;
    if (grid) {
      this.grid = new THREE.GridHelper(span, Math.max(2, Math.round(span / step)), grid[0], grid[1]);
      // GridHelper lies in XZ; stand it up so it is the XY ground plane.
      this.grid.rotateX(Math.PI / 2);
      this.grid.position.z = z;
      this.scene.add(this.grid);
    }

    if (this.ground) {
      this.scene.remove(this.ground);
      this.ground.geometry.dispose();
      this.ground = undefined;
    }
    if (this.sun && this.sceneLook.shadow !== undefined) {
      this.ground = new THREE.Mesh(
        new THREE.PlaneGeometry(span, span),
        new THREE.ShadowMaterial({ opacity: this.sceneLook.shadow }),
      );
      this.ground.position.z = z - 0.01;
      this.ground.receiveShadow = true;
      this.scene.add(this.ground);
    }
  }

  /** Point the sun at the part and tighten its shadow frustum around it. */
  private aimSun(bounds: Bounds, size: { x: number; y: number; z: number }) {
    const radius = Math.max(1e-3, Math.hypot(size.x, size.y, size.z) / 2);
    const center = new THREE.Vector3(
      (bounds.min.x + bounds.max.x) / 2,
      (bounds.min.y + bounds.max.y) / 2,
      (bounds.min.z + bounds.max.z) / 2,
    );

    if (!this.sun) return;
    const sun = this.sun.light;
    sun.position.copy(center).addScaledVector(this.sun.from, radius * 4);
    sun.target.position.copy(center);
    sun.target.updateMatrixWorld();

    // A shadow camera much larger than the part spends its resolution on empty space.
    const cam = sun.shadow.camera;
    cam.left = -radius * 1.6;
    cam.right = radius * 1.6;
    cam.top = radius * 1.6;
    cam.bottom = -radius * 1.6;
    cam.near = radius * 0.5;
    cam.far = radius * 9;
    cam.updateProjectionMatrix();
  }

  /** Frame the part, filling the view. */
  frameAll(bounds: Bounds) {
    const box = new THREE.Box3(
      new THREE.Vector3(bounds.min.x, bounds.min.y, bounds.min.z),
      new THREE.Vector3(bounds.max.x, bounds.max.y, bounds.max.z),
    );
    const { position, target } = this.framing(box, new THREE.Vector3(...VIEWS.threeQuarter).normalize());
    this.flight = undefined;
    this.camera.position.copy(position);
    this.controls.target.copy(target);
    this.controls.update();
    this.wake();
  }

  /** Where the camera stands to fill the view with `box`, looking from `from`. */
  private framing(box: THREE.Box3, from: THREE.Vector3): { position: THREE.Vector3; target: THREE.Vector3 } {
    const target = box.getCenter(new THREE.Vector3());
    const radius = Math.max(1e-3, box.getSize(new THREE.Vector3()).length() / 2);

    // Fit the *narrower* field of view: `camera.fov` is the vertical one, and
    // fitting to it alone crops the sides off in the pane an editor leaves.
    const vFov = (this.camera.fov * Math.PI) / 180;
    const hFov = 2 * Math.atan(Math.tan(vFov / 2) * this.camera.aspect);
    const fov = Math.min(vFov, hFov);
    const distance = (radius / Math.sin(fov / 2)) * 1.06;
    return { position: target.clone().addScaledVector(from, distance), target };
  }
}

/** The directions the view presets look from, toward the camera: Z is up and the front faces -Y. */
export const VIEWS = {
  threeQuarter: [0.72, -1, 0.62],
  front: [0, -1, 0],
  side: [1, 0, 0],
  back: [0, 1, 0],
} as const satisfies Record<string, readonly [number, number, number]>;

/** A named body as drawn: a group the pose is written to, and the box it fills as drawn. */
interface DrawnBody {
  name: string;
  /** Its triangles in the part's index buffer, counted in triangles. */
  start: number;
  count: number;
  group: THREE.Group;
  mesh: THREE.Mesh;
  box: THREE.Box3;
}

interface Flight {
  startedAt: number;
  from: { target: THREE.Vector3; offset: THREE.Vector3 };
  to: { target: THREE.Vector3; offset: THREE.Vector3 };
}

/** How long a preset takes to fly to, in ms. */
const FLIGHT_MS = 420;
/** Room left around a part a preset frames, for the controls that sit on the view. */
const FIT_MARGIN = 1.18;
/** How much of a face's opacity x-ray leaves. */
const XRAY_OPACITY = 0.28;

/**
 * Between two camera offsets around Z, the way an orbit would go: azimuth the
 * short way round, elevation and distance straight. A straight line from front
 * to back runs through the part.
 */
function sphericalLerp(a: THREE.Vector3, b: THREE.Vector3, t: number): THREE.Vector3 {
  const polar = (v: THREE.Vector3) => ({
    r: v.length(),
    up: Math.atan2(v.z, Math.hypot(v.x, v.y)),
    around: Math.atan2(v.y, v.x),
  });
  const from = polar(a);
  const to = polar(b);
  let turn = to.around - from.around;
  turn -= Math.round(turn / (2 * Math.PI)) * 2 * Math.PI;
  const r = from.r + (to.r - from.r) * t;
  const up = from.up + (to.up - from.up) * t;
  const around = from.around + turn * t;
  return new THREE.Vector3(r * Math.cos(up) * Math.cos(around), r * Math.cos(up) * Math.sin(around), r * Math.sin(up));
}

/** The box one body's triangles fill, as drawn. */
function spanBox(geo: Geometry, span: { start: number; count: number }): THREE.Box3 {
  const box = new THREE.Box3();
  const point = new THREE.Vector3();
  const indexed = geo.indices.length > 0;
  for (let corner = span.start * 3; corner < (span.start + span.count) * 3; corner++) {
    box.expandByPoint(point.fromArray(geo.positions, (indexed ? geo.indices[corner] : corner) * 3));
  }
  return box;
}

/** Each vertex a body's triangles use, once, as x, y, z runs. */
function bodyPoints(body: DrawnBody): Float32Array {
  const geometry = body.mesh.geometry;
  const positions = geometry.getAttribute("position").array;
  const index = geometry.getIndex()?.array;
  const seen = new Uint8Array(positions.length / 3);
  const points: number[] = [];
  for (let corner = body.start * 3; corner < (body.start + body.count) * 3; corner++) {
    const vertex = index ? index[corner] : corner;
    if (seen[vertex]) continue;
    seen[vertex] = 1;
    points.push(positions[vertex * 3], positions[vertex * 3 + 1], positions[vertex * 3 + 2]);
  }
  return Float32Array.from(points);
}

function isTranslation(matrix: THREE.Matrix4): boolean {
  const m = matrix.elements;
  const diagonal = m[0] === 1 && m[5] === 1 && m[10] === 1;
  return diagonal && m[1] === 0 && m[2] === 0 && m[4] === 0 && m[6] === 0 && m[8] === 0 && m[9] === 0;
}

/** The box `points` fill once `matrix` has moved them. */
function reachOf(points: Float32Array, matrix: THREE.Matrix4): THREE.Box3 {
  const m = matrix.elements;
  const lo = [Infinity, Infinity, Infinity];
  const hi = [-Infinity, -Infinity, -Infinity];
  for (let i = 0; i < points.length; i += 3) {
    const x = points[i];
    const y = points[i + 1];
    const z = points[i + 2];
    // Column-major, as three keeps it.
    for (let axis = 0; axis < 3; axis++) {
      const v = m[axis] * x + m[axis + 4] * y + m[axis + 8] * z + m[axis + 12];
      if (v < lo[axis]) lo[axis] = v;
      if (v > hi[axis]) hi[axis] = v;
    }
  }
  return new THREE.Box3(new THREE.Vector3(...lo), new THREE.Vector3(...hi));
}

/** Whether an object and everything above it is drawn. */
function shown(object: THREE.Object3D): boolean {
  for (let o: THREE.Object3D | null = object; o; o = o.parent) if (!o.visible) return false;
  return true;
}

/**
 * Turn logical edges into independently pickable drawables — one primitive
 * each, at these counts. A look that gives edges a width draws them as one set
 * of screen-space lines instead, the thin ones kept unseen for picking: a GL
 * line is one device pixel, half a CSS pixel on a phone.
 */
function edgeLines(
  edges: EdgeCurve[],
  look: SceneLook["edges"],
): { group: THREE.Group; lines: THREE.LineSegments[]; wide?: LineSegments2 } {
  const group = new THREE.Group();
  const lines: THREE.LineSegments[] = [];
  const all: number[] = [];
  for (const edge of edges) {
    const pts: number[] = [];
    for (let i = 0; i + 1 < edge.points.length; i++) {
      pts.push(...edge.points[i], ...edge.points[i + 1]);
    }
    all.push(...pts);
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.Float32BufferAttribute(pts, 3));
    const line = new THREE.LineSegments(
      geometry,
      new THREE.LineBasicMaterial({
        color: look.color,
        transparent: true,
        opacity: look.opacity,
        visible: look.width === undefined,
      }),
    );
    line.userData.edge = edge;
    line.castShadow = false;
    line.receiveShadow = false;
    line.renderOrder = 1;
    group.add(line);
    lines.push(line);
  }
  if (look.width === undefined || all.length === 0) return { group, lines };
  const geometry = new LineSegmentsGeometry();
  geometry.setPositions(all);
  const wide = new LineSegments2(
    geometry,
    new LineMaterial({ color: look.color, transparent: look.opacity < 1, opacity: look.opacity }),
  );
  wide.raycast = () => {};
  wide.castShadow = false;
  wide.renderOrder = 1;
  group.add(wide);
  return { group, lines, wide };
}

/** The corner sprite: white disc, ink ring, white glow — a shape a plain `PointsMaterial` cannot be. */
function cornerSprite(fill: string, ring: string): THREE.Texture {
  const size = 64;
  const canvas = document.createElement("canvas");
  canvas.width = size;
  canvas.height = size;
  const ctx = canvas.getContext("2d")!;
  const centre = size / 2;

  // The glow first, and wider than the ring, or it is a smudge under the mark.
  ctx.shadowColor = "rgba(255,255,255,0.95)";
  ctx.shadowBlur = 9;
  ctx.fillStyle = "rgba(255,255,255,0.9)";
  ctx.beginPath();
  ctx.arc(centre, centre, 19, 0, Math.PI * 2);
  ctx.fill();
  ctx.shadowBlur = 0;

  ctx.beginPath();
  ctx.arc(centre, centre, 14, 0, Math.PI * 2);
  ctx.fillStyle = fill;
  ctx.fill();
  ctx.lineWidth = 5;
  ctx.strokeStyle = ring;
  ctx.stroke();

  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  return texture;
}

/** One material per state, shared by every corner. Lazy, so importing needs no DOM. */
const VERTEX_MATERIALS: Partial<Record<"hidden" | "hover" | "selected", THREE.PointsMaterial>> = {};

function vertexMaterial(state: "hidden" | "hover" | "selected"): THREE.PointsMaterial {
  const existing = VERTEX_MATERIALS[state];
  if (existing) return existing;

  const made =
    state === "hidden"
      ? // Not drawn, but still raycast — see `paintVertex`.
        new THREE.PointsMaterial({ visible: false })
      : new THREE.PointsMaterial({
          map: cornerSprite(state === "selected" ? gold() : "#ffffff", "#0b0e13"),
          size: state === "selected" ? 15 : 13,
          sizeAttenuation: false,
          transparent: true,
          depthWrite: false,
          depthTest: false,
        });
  made.userData.shared = true;
  VERTEX_MATERIALS[state] = made;
  return made;
}

/** Turn exact edge endpoints into pickable corners, drawn only when pointed at. */
function vertexMarkers(vertices: VertexPoint[]): { group: THREE.Group; markers: THREE.Points[] } {
  const group = new THREE.Group();
  const markers: THREE.Points[] = [];
  for (const vertex of vertices) {
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.Float32BufferAttribute(vertex.point, 3));
    const marker = new THREE.Points(geometry, vertexMaterial("hidden"));
    marker.userData.vertex = vertex;
    marker.castShadow = false;
    marker.receiveShadow = false;
    marker.renderOrder = 7;
    group.add(marker);
    markers.push(marker);
  }
  return { group, markers };
}

/** Gold exact corner targets, intentionally visible through their replacement patch. */
function targetVertexMarkers(vertices: TargetVertex[]): THREE.Group {
  const group = new THREE.Group();
  for (const vertex of vertices) {
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.Float32BufferAttribute(vertex.point, 3));
    const marker = new THREE.Points(
      geometry,
      new THREE.PointsMaterial({
        color: goldHex(),
        size: 12,
        sizeAttenuation: false,
        depthTest: false,
        transparent: true,
        opacity: 1,
      }),
    );
    marker.renderOrder = 3;
    group.add(marker);
  }
  return group;
}

const shared = {
  flatShading: DEBUG === "flat",
  // Pushed back a hair so the edge lines lying on it win the depth test.
  polygonOffset: true,
  polygonOffsetFactor: 1,
  polygonOffsetUnits: 1,
};

function solidMaterial(look: SceneLook, color = look.part): THREE.Material {
  const surface = look.surface;
  return surface.kind === "phong"
    ? new THREE.MeshPhongMaterial({ color, shininess: surface.shininess, specular: surface.specular, ...shared })
    : new THREE.MeshStandardMaterial({ color, roughness: surface.roughness, metalness: surface.metalness, ...shared });
}

const TONE_MAPPINGS: Record<SceneLook["toneMapping"], THREE.ToneMapping> = {
  neutral: THREE.NeutralToneMapping,
  none: THREE.NoToneMapping,
};

/**
 * The colour to fill the backdrop with so it shows as `hex` after the look's
 * tone mapping, which the backdrop goes through with everything else. Khronos
 * PBR Neutral's toe pulls a dark grey nearly to black and its shoulder dims a
 * light one; this is its inverse, exact for a grey.
 */
function backdrop(hex: string, look: SceneLook): string {
  if (look.toneMapping === "none") return hex;
  const start = 0.76;
  const knee = 1 - start;
  const shown = new THREE.Color(hex);
  const unrolled = [shown.r, shown.g, shown.b].map((y) => (y < start ? y : (knee * knee) / (1 - Math.min(y, 0.999)) - knee + start));
  const low = Math.min(...unrolled);
  const offset = low < 0.04 ? 0.4 * Math.sqrt(low) - low : 0.04;
  const [r, g, b] = unrolled.map((u) => (u + offset) / look.exposure);
  return `#${new THREE.Color().setRGB(r, g, b).getHexString()}`;
}

/**
 * An authored look. It carries the environment itself because three ignores a
 * material's `envMapIntensity` under `scene.environment`, and a metal lit at the
 * scene's 0.62 has almost nothing to reflect: steel drew black.
 */
function authoredMaterial(look: FaceMaterial, environment: THREE.Texture | null): THREE.MeshPhysicalMaterial {
  const seeThrough = look.opacity < 1;
  return new THREE.MeshPhysicalMaterial({
    color: look.color,
    roughness: look.roughness,
    metalness: look.metalness,
    envMap: environment,
    envMapIntensity: 0.62 + 0.9 * look.metalness,
    clearcoat: look.clearcoat,
    clearcoatRoughness: 0.08,
    emissive: look.emissive ?? 0x000000,
    opacity: look.opacity,
    transparent: seeThrough,
    // A see-through body must not hide what is behind it from the depth test.
    depthWrite: !seeThrough,
    ...shared,
  });
}

/**
 * One material for a plain part; for a part with authored materials, a group
 * per face run, and triangles no run covers left on the default so a gap in
 * the runs can never make surface disappear.
 */
function dressFaces(
  g: THREE.BufferGeometry,
  geo: Geometry,
  look: SceneLook,
  environment: THREE.Texture | null,
): THREE.Material | THREE.Material[] {
  const looks = geo.faceMaterials ?? [];
  const runs = [...(geo.faceRuns ?? [])].sort((a, b) => a.start - b.start);
  if (!looks.some(Boolean) || runs.length === 0 || geo.indices.length === 0) return solidMaterial(look);

  const materials: THREE.Material[] = [solidMaterial(look)];
  const slotOf = new Map<string, number>();
  const slot = (look?: FaceMaterial) => {
    if (!look) return 0;
    const key = JSON.stringify(look);
    let index = slotOf.get(key);
    if (index === undefined) {
      index = materials.push(authoredMaterial(look, environment)) - 1;
      slotOf.set(key, index);
    }
    return index;
  };
  const triangles = geo.indices.length / 3;
  let covered = 0;
  for (const run of runs) {
    if (run.start > covered) g.addGroup(covered * 3, (run.start - covered) * 3, 0);
    g.addGroup(run.start * 3, run.count * 3, slot(looks[run.face]));
    covered = Math.max(covered, run.start + run.count);
  }
  if (covered < triangles) g.addGroup(covered * 3, (triangles - covered) * 3, 0);
  return materials;
}

/** Free a material unless it is shared — the corner materials outlive any one part. */
function releaseMaterial(material: THREE.Material | THREE.Material[] | undefined) {
  if (!material) return;
  if (Array.isArray(material)) {
    material.forEach(releaseMaterial);
    return;
  }
  if (material.userData.shared) return;
  material.dispose();
}

/** Drop everything under an object. Not geometry: the stencil helpers share the part's buffers. */
function disposeTree(root: THREE.Object3D) {
  root.traverse((object) => {
    const drawable = object as THREE.Mesh | THREE.LineSegments;
    releaseMaterial(drawable.material);
  });
}

const scratch = new THREE.Vector3();

function boxCorners(box: THREE.Box3): THREE.Vector3[] {
  const corners: THREE.Vector3[] = [];
  for (const x of [box.min.x, box.max.x]) {
    for (const y of [box.min.y, box.max.y]) {
      for (const z of [box.min.z, box.max.z]) corners.push(new THREE.Vector3(x, y, z));
    }
  }
  return corners;
}

/** Round to 1, 2 or 5 times a power of ten. */
function niceStep(v: number): number {
  if (!(v > 0) || !Number.isFinite(v)) return 1;
  const decade = 10 ** Math.floor(Math.log10(v));
  const n = v / decade;
  const snapped = n < 1.5 ? 1 : n < 3.5 ? 2 : n < 7.5 ? 5 : 10;
  return snapped * decade;
}


/** Vertical gradient used as the canvas backdrop. */
function gradientTexture([top, bottom]: [string, string]): THREE.Texture {
  const c = document.createElement("canvas");
  c.width = 2;
  c.height = 256;
  const ctx = c.getContext("2d")!;
  const g = ctx.createLinearGradient(0, 0, 0, 256);
  g.addColorStop(0, top);
  g.addColorStop(1, bottom);
  ctx.fillStyle = g;
  ctx.fillRect(0, 0, 2, 256);

  const tex = new THREE.CanvasTexture(c);
  tex.colorSpace = THREE.SRGBColorSpace;
  return tex;
}

/** Bump when the thumbnail's camera or scene changes, and every card redraws. */
export const THUMBNAIL_LOOK = 2;

let still: Viewport | undefined;

/**
 * The part as its picker card shows it: this viewport's scene, framed from the
 * default angle, drawn off screen. One viewport serves every card, so a folder
 * of a thousand parts is a thousand frames and still one WebGL context.
 */
export function drawThumbnail(geo: Geometry, bounds: Bounds): string {
  still ??= new Viewport(document.createElement("div"), {}, false);
  still.setGeometry(geo, bounds);
  still.frameAll(bounds);
  const png = still.snapshot();
  still.clearPart();
  return png;
}
