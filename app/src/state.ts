/**
 * Everything the window shares, as signals.
 *
 * The choice worth explaining is signals rather than component state, because
 * the obvious React shape would be wrong here. This app has one document and
 * one evaluation of it, and about eight places that need to read them: the
 * report, the titlebar's saved mark, the inspector, the section's travel, the
 * editor's own decorations, the exporter. Threading that through props would
 * put the whole application state in a root component and re-render the
 * viewport pane every time a hover changed a caption.
 *
 * More to the point, the evaluation cycle is not a render. It is a small state
 * machine — running, dirty, coalesce, re-enter — that predates this file and
 * has a debounce, an in-flight guard and an echo-breaking viewer id in it.
 * Signals let that machine stay exactly what it was and simply be *observed*,
 * which is the whole reason the port to Preact could be mechanical instead of a
 * rewrite of the part that is easiest to break quietly.
 *
 * The rule that keeps it honest: **nothing here computes a measurement.** Every
 * number a person reads comes out of `snapshot`, which is the object
 * `service.rs` built and every transport serialises. A signal in this file may
 * hold it, select it or say it is stale. It may not derive one.
 */

import { computed, signal } from "@preact/signals";
import type { EditorView } from "codemirror";
import type * as dsl from "./dsl";
import type { AgentLink, AvailableUpdate, KernelLoad, McpStatus } from "./backend";
import type { VertexPoint } from "./entities";
import type { Projects } from "./projects";
import { store, stored } from "./store";
import type { FaceMaterial, Viewport } from "./viewport";

/**
 * One evaluation, as the host measured it.
 *
 * The same object, field for field, that an agent gets back from MCP's
 * `evaluate_part`: `service.rs` builds exactly one of these per evaluation and
 * every transport serialises it. Nothing in the frontend may compute a
 * measurement of its own — this window and a model looking at the same part
 * have to be reading the same numbers, and the only way to guarantee that is to
 * have one producer.
 *
 * Millimetres throughout. Triples are `[x, y, z]`.
 */
/** One place the print check has something to say about. */
export interface PrintFinding {
  kind: "thin" | "collision" | "overhang";
  what: string;
  fix: string;
  body?: string;
  thickness_mm?: number;
  thin_kind?: "feather" | "wall" | "edge";
  removed_mm3?: number;
  unsupported_mm2?: number;
  at?: [number, number, number];
  between?: [string, string];
}

export interface EvaluationSnapshot {
  /** The part's own checks, judged on this build; absent when the script carries none. */
  checks?: { verdict: "passed" | "failed"; passed: number; failed?: { check: string; measured_mm?: number; measured_mm3?: number; why?: string }[] };
  /** What the part is for, judged on every build; the verdict is `meets`,
   *  `over: ...`, or the one line that says there is no brief to judge it by. */
  brief?: {
    verdict: string;
    envelope?: { given: [number, number, number]; measured: [number, number, number]; fits: boolean; over_mm?: number; on?: string };
    budget_cm3?: { given: number; measured: number; fits: boolean; over_cm3?: number };
    printer?: { given: string; fits?: boolean; note?: string };
    holds?: string[];
    gesture?: string;
    material?: string;
  };
  /** Whether the part prints, judged on every build: `failed` where nothing
   *  prints (under `floor_mm`), `flagged` for what prints but should be read. */
  print_check?: {
    verdict: "passed" | "flagged" | "failed";
    failed?: PrintFinding[];
    flagged?: PrintFinding[];
    thinnest?: PrintFinding;
    /** Each body as it prints: its bed contact and overhang in that orientation. */
    bodies?: {
      body: string;
      up: string;
      declared: boolean;
      bed_mm2: number;
      footprint_fraction: number;
      unsupported_mm2: number;
      overhanging_faces: number;
      support_mm3?: number;
      sampled: boolean;
    }[];
    floor_mm: number;
    minimum_mm: number;
    overhang_deg: number;
  };
  units: string;
  /** `solid`, `surface` (faces with no inside) or `mixed` (named bodies of both). */
  kind: "solid" | "surface" | "mixed";
  size: [number, number, number];
  bounds_min: [number, number, number];
  bounds_max: [number, number, number];
  /** The enclosed volume; absent for a surface, which encloses none. */
  volume_mm3?: number;
  area_mm2: number;
  centroid: [number, number, number];
  /** What is true of a surface: whether it is open, and where. */
  surface?: {
    area_mm2: number;
    open: boolean;
    faces: number;
    shells: number;
    free_edges: number;
    free_edge_length_mm: number;
    boundary_loops: number;
    open_chains?: number;
  };
  /** The thinnest and thickest any `.thicken(t)` measured. */
  thickened_mm?: { min: number; max: number };
  /** The kernel's own counts. */
  faces?: number;
  topological_edges?: number;
  triangles: number;
  resolution_mm: number;
  /** Whether the solid bodies' mesh closes; absent for a surface. */
  watertight?: boolean;
  non_manifold_edges?: number;
  /** Free-standing pieces of surface; one for a part, and for a part that
   *  returns several named bodies, their number when each is intact. */
  bodies: number;
  /** Closed surfaces inside another: a shell's cavity. */
  voids: number;
  /** Each named body of a part that returns several, measured alone.
   *  Absent for a one-solid part. */
  named_bodies?: {
    name: string;
    kind: "solid" | "surface";
    /** A `.reference()` body: drawn and measured against the part, not part of it. */
    reference?: boolean;
    volume_mm3?: number;
    faces: number;
    watertight?: boolean;
    /** Free-standing pieces inside this body: one when it is intact. */
    pieces: number;
  }[];
  /** How each pair of named bodies sits, on the exact solids. */
  between_bodies?: {
    a: string;
    b: string;
    verdict: "clear" | "touching" | "interfering" | "crossing";
    interference_mm3: number;
    clearance_mm?: number;
    /** How far one reaches into the other, when they interfere. */
    depth_mm?: number;
    deepest_mm?: [number, number, number];
    /** The surface they share, when they touch: a seat, or a corner at 0. */
    contact_mm2?: number;
    contact_patches?: number;
    contact_center_mm?: [number, number, number];
  }[];
  /** The surface in the part's lowest plane and how many patches it is in;
   *  what a printed part rests on. Absent only for an empty mesh. */
  stands_on?: {
    z_mm: number;
    area_mm2: number;
    patches: number;
    footprint_fraction: number;
    tolerance_mm: number;
  };
  tags: string[];
  /** Every cut that took material from a named feature besides its target:
   *  "grille cuts boss", with the mm³ it took and where. Absent when none did. */
  collisions?: {
    cut: string;
    target: string;
    feature: string;
    removed_mm3: number;
    at: [number, number, number];
    extent_mm: [number, number, number];
    body?: string;
  }[];
  treatments: { node: number; op: string; amount_mm: number; continuity?: string }[];
  /** Shapes the root never reaches. Absent when there are none. */
  unused_nodes?: number;
  /** What the script reported with `note()`: requested, never measured. */
  notes?: {
    source: "from the script, not measured";
    values: { label: string; value: number | string | number[] }[];
    dropped?: number;
  };
  /** Which kernel measured this: `brep`, the only one. */
  backend: "brep";
  kernel_ms: number;
}

export interface Vec3 {
  x: number;
  y: number;
  z: number;
}

export type Bounds = { min: Vec3; max: Vec3 };

/** A visible B-rep edge. `id` is valid only for this evaluated result. */
export interface EdgeCurve {
  id: string;
  points: number[][];
  center: [number, number, number];
  direction: [number, number, number] | null;
  length_mm: number;
  /** Ephemeral source node when this final edge was generated by a treatment. */
  treatment_node?: number;
}

/**
 * One face's triangles, as a span of the shared index buffer.
 *
 * `start` and `count` are in triangles, not indices. `face` is the face's
 * position in the kernel's own face traversal — the order `snapshot.faces`
 * counts — and deliberately not this run's position in the list: a face
 * carrying no triangulation contributes no run, so the two disagree exactly
 * where using the wrong one would still add up to a plausible total.
 */
export interface FaceRun {
  face: number;
  start: number;
  count: number;
}

/**
 * What one face is, measured off the B-rep rather than off its triangles.
 *
 * Position in `Evaluated.faces` is the face's own number — the same one
 * `FaceRun.face` carries — so the face under the pointer and the face described
 * here are the same face.
 */
export interface FaceSummary {
  area_mm2: number;
  centroid: [number, number, number];
  /** Faces sharing an edge with this one, by the same numbering. */
  adjacent: number[];
  surface: {
    /** `plane`, `cylinder`, `cone`, `sphere`, `torus`, `nurbs`, `other`. */
    kind: string;
    /** A plane's outward normal, or the axis of anything turned about one. */
    direction?: [number, number, number];
    radius?: number;
  };
  /** The innermost `.material()` on this face, if any. */
  material?: FaceMaterial;
}

export interface Evaluated {
  /** Arrays from the page's kernel, which sends the mesh as binary; numbers over HTTP and IPC. */
  positions: ArrayLike<number>;
  normals: ArrayLike<number>;
  indices: ArrayLike<number>;
  /** Logical edge curves. */
  edges: EdgeCurve[];
  /**
   * Where each face's triangles sit in `indices`, and which face each run is.
   * Optional on the wire; the viewport treats its absence as "faces are not
   * pickable here" rather than guessing at one.
   */
  face_runs?: FaceRun[];
  /** What each face is. Indexed by the kernel's own face number. */
  faces?: FaceSummary[];
  snapshot: EvaluationSnapshot;
  /**
   * Measured before this page was deployed rather than here, and standing in
   * only until the same graph has been built in front of the reader. Set by
   * the transport that shipped it; see `page/prebuilt.ts`.
   */
  shipped?: boolean;
}

export interface TargetPreview {
  node: number;
  edges: EdgeCurve[];
  vertices: { id: string; point: [number, number, number]; degree: number }[];
  /** Tags whose live edge set is exactly this target, from the kernel. */
  provenance?: string[];
}

// ------------------------------------------------------------------ the part

/**
 * Which project is open, and whether the editor still matches what is on disk.
 *
 * The project folder is shared with the user's filesystem and with agents over
 * MCP, so "the file I opened" and "the text in front of me" genuinely can
 * disagree without anybody typing. The titlebar says which.
 */
export const openPath = signal<string | undefined>(undefined);
/** The source as last read from or written to disk, to tell dirty from clean. */
export const savedSource = signal("");
/** What is in the editor right now, updated on the keystroke. */
export const docSource = signal("");
/** The parts on disk, as of the last read. */
export const projects = signal<Projects | undefined>(undefined);

export const isDirty = computed(
  () => openPath.value !== undefined && docSource.value !== savedSource.value,
);

// ------------------------------------------------------------ the evaluation

/** The last measurements, kept so a save can describe the part it saved. */
export const snapshot = signal<EvaluationSnapshot | undefined>(undefined);
/** The last graph that evaluated cleanly, kept for export. */
export const lastGraph = signal<dsl.Doc | null>(null);
export const lastSource = signal("");
export const lastTreatments = signal<dsl.TreatmentSource[]>([]);
export const visibleEdges = signal<EdgeCurve[]>([]);
export const visibleVertices = signal<VertexPoint[]>([]);
/** How many final curves this evaluation traced back to an authored call. */
export const linkedEdgeCount = signal(0);
export const linkedMethods = signal<string[]>([]);

/** The part's own extent, so a section plane can only travel where there is
 *  material. Undefined once there is no part on screen. */
export const bounds = signal<Bounds | undefined>(undefined);

// -------------------------------------------------------------- the chrome

export type Tone = "" | "busy" | "failed";
export const status = signal<{ text: string; tone: Tone }>({ text: "ready", tone: "" });
export const errorText = signal("");

/** A newer release of the app, while the user has not dismissed it. */
export const update = signal<AvailableUpdate | undefined>(undefined);

/** Whether a model is on the third transport. Undefined until first answered. */
export const mcp = signal<McpStatus | undefined>(undefined);

/** Whether this tab is open to agents, and by which link: ParCAD web only. */
export const agentLink = signal<AgentLink>({ kind: "off" });
/** Whether the panel that gives an agent that link is showing. */
export const agentPanel = signal(false);

/**
 * How far a kernel that has to be downloaded has got. Undefined whenever the
 * kernel is simply there, which is always under the two host transports.
 */
export const kernelLoad = signal<KernelLoad | undefined>(undefined);

// ------------------------------------------------------------- the selection

/**
 * The face under the pointer, when the exact kernel attributed one.
 *
 * `face` is the kernel's own face number, which is ephemeral in exactly the way
 * `edge@7` is — it is shown as a position within a count, never offered as
 * something to write into a script.
 */
export const hoveredFace = signal<{ face: number; triangles: number } | undefined>(undefined);

/** What the hovered face is, when the kernel described it. */
export const hoveredFaceSummary = computed(() =>
  hoveredFace.value ? lastFaces.value?.[hoveredFace.value.face] : undefined,
);

/**
 * The face descriptions of the part on screen.
 *
 * Kept beside the viewport's own copy rather than read back out of it: the
 * viewport owns triangles, and this is the measurement of what those triangles
 * are. See the island rule in CLAUDE.md.
 */
export const lastFaces = signal<FaceSummary[] | undefined>(undefined);

export const hoveredEdge = signal<EdgeCurve | undefined>(undefined);
export const selectedEdge = signal<EdgeCurve | undefined>(undefined);
export const hoveredVertex = signal<VertexPoint | undefined>(undefined);
export const selectedVertex = signal<VertexPoint | undefined>(undefined);

/** What the caret's treatment resolves to, once the kernel has answered. */
export const targetPreview = signal<
  { method: string; detail: string; resolved: boolean } | undefined
>(undefined);

// ------------------------------------------------------------- the view

// There is no kernel to choose and no mesh depth to set: the window evaluates
// with the exact kernel, whose mesh is a deflection bound and not a grid. A
// kernel toggle and a detail slider both lived here once and are gone rather
// than dimmed — see CLAUDE.md, "A control that cannot do anything is deleted".
export const sectionAxis = signal<"" | "x" | "y" | "z">("");
export const sectionAt = signal(0);
export const sectionKeep = signal<"below" | "above">("below");

// -------------------------------------------------------------- the editor

/**
 * The live CodeMirror view.
 *
 * Not a signal: it is an identity rather than a value, nothing re-renders when
 * it changes, and it is set exactly once when the editor island mounts. A box
 * rather than a bare `let` so importers see the assignment.
 */
export const editorRef: { current: EditorView | undefined } = { current: undefined };

/** The editor, or a loud failure. Called only from paths that need one. */
export function editor(): EditorView {
  if (!editorRef.current) throw new Error("the editor is not mounted yet");
  return editorRef.current;
}

/**
 * The live three.js scene, for the same reason and on the same terms.
 *
 * Both islands own DOM that a virtual DOM must not touch: CodeMirror maintains
 * its own view of the document, and three.js owns a canvas. They are mounted by
 * components, which is where the element comes from, and reached from the
 * machinery below through these boxes.
 */
export const viewportRef: { current: Viewport | undefined } = { current: undefined };

/** Only a hidden editor is written down; showing the code is the default. */
const CODE_VISIBLE = "parcad.code.visible";

/**
 * Whether the source is on screen at all.
 *
 * Remembered between sessions, because a window arranged to show only the part
 * — on a second screen, or while a model is doing the authoring — should still
 * be that window tomorrow. The editor itself is never unmounted when this is
 * false: CodeMirror owns the document, the undo history and the evaluation
 * cycle that starts with it, and none of that is a view.
 */
export const codeVisible = signal(stored(CODE_VISIBLE) !== "0");

codeVisible.subscribe((visible) => store(CODE_VISIBLE, visible ? null : "0"));

/** Whether the parts picker is open. */
export const browserOpen = signal(false);

export function setStatus(text: string, tone: Tone = "") {
  status.value = { text, tone };
}
