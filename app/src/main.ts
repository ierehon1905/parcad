/**
 * Wiring: editor -> DSL -> intent graph -> Rust core -> viewport.
 *
 * The script runs here in the webview rather than in an embedded interpreter,
 * because the webview already has a very good JavaScript engine and the script
 * does no geometry — it only builds a graph. Everything expensive happens in
 * Rust.
 */

import "./style.css";
import { invoke } from "@tauri-apps/api/core";
import { EditorView, basicSetup } from "codemirror";
import { javascript } from "@codemirror/lang-javascript";
import { oneDark } from "@codemirror/theme-one-dark";
import * as dsl from "./dsl";
import { Shape } from "./dsl";
import { BRACKET, ENCLOSURE } from "./examples";
import { Viewport } from "./viewport";

interface Report {
  units: string;
  bounds: { min: Vec3; max: Vec3 };
  size: Vec3;
  framing_bounds: { min: Vec3; max: Vec3 };
  mass: { volume_mm3: number; area_mm2: number; centroid: Vec3 };
  mesh: {
    vertices: number;
    triangles: number;
    resolution_mm: number;
    watertight: boolean;
    non_manifold_edges: number;
  };
  tags: string[];
  live_nodes: number;
  total_nodes: number;
}

interface Vec3 {
  x: number;
  y: number;
  z: number;
}

interface Evaluated {
  positions: number[];
  normals: number[];
  indices: number[];
  /** Logical edge curves. B-rep only; the implicit backend has none. */
  edges: number[][][];
  /** Face and edge counts. Null on the implicit path, where the question does
   *  not apply — which is different from the answer being zero. */
  topology: { faces: number; edges: number } | null;
  backend: "implicit" | "brep";
  report: Report;
  timings: {
    lower_and_mesh_ms: number;
    normals_ms: number;
    kernel_ms: number;
  };
}

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

const statusEl = $("status");
const errorEl = $("error");
const reportEl = $("report");
const depthInput = $<HTMLInputElement>("depth");
const depthValue = $("depth-value");
const backendSelect = $<HTMLSelectElement>("backend");

const viewport = new Viewport($("viewport"));
// Handle for poking at the scene from the console during development.
(window as unknown as Record<string, unknown>).__viewport = viewport;

/** Whether the camera has been framed on this part yet. */
let framed = false;
/** The last graph that evaluated cleanly, kept for export. */
let lastGraph: unknown = null;

// ---------------------------------------------------------------- the editor

const editor = new EditorView({
  doc: BRACKET,
  parent: $("editor"),
  extensions: [
    basicSetup,
    javascript(),
    oneDark,
    EditorView.updateListener.of((v) => {
      if (v.docChanged) schedule();
    }),
  ],
});

// ------------------------------------------------------------ the eval cycle

let timer: number | undefined;
let running = false;
let dirty = false;

/**
 * Re-evaluate shortly after typing stops.
 *
 * Short enough to feel live, long enough that a burst of keystrokes is one
 * evaluation rather than twenty.
 */
function schedule() {
  window.clearTimeout(timer);
  timer = window.setTimeout(run, 350);
}

async function run() {
  if (running) {
    // Coalesce: remember that something changed and pick it up on the way out.
    dirty = true;
    return;
  }
  running = true;
  dirty = false;

  setStatus("evaluating", "busy");

  try {
    const graph = buildGraph(editor.state.doc.toString());
    lastGraph = graph;

    const depth = Number(depthInput.value);
    const result = await evaluateGraph(graph, depth, backendSelect.value);

    show(result);
    clearError();
    const ms =
      result.backend === "brep"
        ? result.timings.kernel_ms
        : result.timings.lower_and_mesh_ms + result.timings.normals_ms;
    setStatus(`${result.report.mesh.triangles.toLocaleString()} tris · ${ms} ms`);
  } catch (e) {
    showError(e);
    setStatus("failed", "failed");
  } finally {
    running = false;
    if (dirty) schedule();
  }
}

/** Is the Tauri bridge present, or are we in a plain browser? */
const inTauri = "__TAURI_INTERNALS__" in window || "__TAURI__" in window;

/**
 * Evaluate a graph, or serve a pre-baked one when there is no desktop shell.
 *
 * The fallback exists so the viewport can be developed and looked at in a normal
 * browser, where the geometry backend does not exist. It ignores the script,
 * which is exactly why the status line says so rather than pretending.
 */
async function evaluateGraph(
  graph: unknown,
  depth: number,
  backend: string,
): Promise<Evaluated> {
  if (inTauri) return invoke<Evaluated>("evaluate", { graph, depth, backend });

  const res = await fetch("/dev-geometry.json");
  if (!res.ok) {
    throw new Error(
      "no desktop backend, and no /dev-geometry.json to fall back on.\n" +
        "Generate one with:\n" +
        "  parcad graph.json --geometry app/public/dev-geometry.json",
    );
  }
  return res.json() as Promise<Evaluated>;
}

/**
 * In a plain browser there is no geometry backend, so the viewport shows a
 * pre-baked file and the script, the backend and the detail level all do
 * nothing. Say so and switch off the controls that lie, rather than leaving
 * someone to work out why the dropdowns have no effect.
 */
function markBrowserOnly() {
  if (inTauri) return;
  for (const el of [backendSelect, depthInput]) {
    el.disabled = true;
    el.title = "needs the desktop app — the browser shows pre-baked geometry";
  }
  const note = document.createElement("div");
  note.className = "notice";
  note.textContent =
    "Browser preview: showing pre-baked geometry from /dev-geometry.json. " +
    "The script is not evaluated and the controls above do nothing. " +
    "Run the desktop app for live evaluation.";
  $("viewport-pane").appendChild(note);
}

/**
 * Run a script and flatten what it returns.
 *
 * The DSL is injected as named parameters rather than as globals, so a script
 * cannot accidentally depend on anything the page happens to have lying around.
 */
function buildGraph(source: string): unknown {
  const api = { ...dsl } as Record<string, unknown>;
  const names = Object.keys(api);

  let fn: (...args: unknown[]) => unknown;
  try {
    fn = new Function(...names, source) as (...args: unknown[]) => unknown;
  } catch (e) {
    throw new Error(`the script did not parse:\n${(e as Error).message}`);
  }

  const result = fn(...names.map((n) => api[n]));
  if (!(result instanceof Shape)) {
    throw new Error(
      "the script must return a shape.\n" +
        "End it with something like:  return body.cut(hole)",
    );
  }
  return dsl.build(result);
}

function show(result: Evaluated) {
  const { report } = result;

  viewport.setGeometry(
    {
      positions: new Float32Array(result.positions),
      normals: new Float32Array(result.normals),
      indices: new Uint32Array(result.indices),
      edges: result.edges,
    },
    report.bounds,
  );

  // Frame once, then leave the camera alone — nothing is more irritating than a
  // view that resets itself every time you change a number.
  if (!framed) {
    viewport.frameAll(report.bounds);
    framed = true;
  }

  const { size, mass, mesh } = report;
  const dead = report.total_nodes - report.live_nodes;
  // "resolution" means different things per backend and the difference matters:
  // a grid spacing is where samples were taken, a deflection is a bound on how
  // far the result can be from the truth. Label them apart.
  const tol =
    result.backend === "brep"
      ? `within <b>${mesh.resolution_mm.toFixed(3)}</b> mm of the true surface`
      : `at <b>${mesh.resolution_mm.toFixed(3)}</b> mm grid`;

  reportEl.innerHTML = [
    `<b>${fmt(size.x)} × ${fmt(size.y)} × ${fmt(size.z)}</b> mm`,
    `volume <b>${fmt(mass.volume_mm3)}</b> mm³ · area <b>${fmt(mass.area_mm2)}</b> mm²`,
    result.topology
      ? `topology <b>${result.topology.faces}</b> faces · <b>${result.topology.edges}</b> edges`
      : "",
    `mesh <b>${mesh.triangles.toLocaleString()}</b> tris ${tol} ` +
      (mesh.watertight
        ? `<span class="ok">watertight</span>`
        : `<span class="warn">NOT watertight — ${mesh.non_manifold_edges} bad edges</span>`),
    report.tags.length ? `tags ${report.tags.join(", ")}` : "no tags",
    dead > 0 ? `<span class="warn">${dead} unused nodes</span>` : "",
  ]
    .filter(Boolean)
    .join("<br>");
}

const fmt = (v: number) =>
  Math.abs(v) >= 1000 ? v.toFixed(0) : v.toFixed(2).replace(/\.00$/, "");

function setStatus(text: string, cls = "") {
  statusEl.textContent = text;
  statusEl.className = `status ${cls}`;
}

function showError(e: unknown) {
  errorEl.textContent = e instanceof Error ? e.message : String(e);
  errorEl.hidden = false;
  // Leave the last good geometry on screen. A broken edit mid-typing should not
  // blank the viewport.
}

function clearError() {
  errorEl.hidden = true;
  errorEl.textContent = "";
}

// ------------------------------------------------------------------- chrome

depthInput.addEventListener("input", () => {
  depthValue.textContent = depthInput.value;
  schedule();
});

backendSelect.addEventListener("change", () => {
  syncBackendUi();
markBrowserOnly();
  // A backend swap changes the geometry, not the part, so the camera stays.
  schedule();
});

/** The detail slider is a grid depth, and B-rep has no grid. */
function syncBackendUi() {
  const brep = backendSelect.value === "brep";
  depthInput.disabled = brep || !inTauri;
  depthInput.parentElement!.classList.toggle("disabled", brep);
  depthInput.parentElement!.title = brep
    ? "B-rep meshes to a fixed 0.01 mm deflection; there is no grid to coarsen"
    : "";
}
syncBackendUi();
markBrowserOnly();

$<HTMLSelectElement>("example").addEventListener("change", (e) => {
  const which = (e.target as HTMLSelectElement).value;
  const doc = which === "enclosure" ? ENCLOSURE : BRACKET;
  editor.dispatch({
    changes: { from: 0, to: editor.state.doc.length, insert: doc },
  });
  framed = false;
});

// Draggable split between editor and viewport.
{
  const splitter = $("splitter");
  const pane = $("editor-pane");
  let dragging = false;

  splitter.addEventListener("pointerdown", (e) => {
    dragging = true;
    splitter.setPointerCapture(e.pointerId);
  });
  splitter.addEventListener("pointermove", (e) => {
    if (!dragging) return;
    const pct = (e.clientX / window.innerWidth) * 100;
    pane.style.width = `${Math.min(75, Math.max(15, pct))}%`;
  });
  splitter.addEventListener("pointerup", (e) => {
    dragging = false;
    splitter.releasePointerCapture(e.pointerId);
  });
}

// Cmd-S exports rather than saving a page nobody wants. Cmd-Shift-S writes
// STEP, which needs the B-rep kernel whatever the viewport is currently showing
// — a mesh cannot be turned into exact surfaces after the fact.
window.addEventListener("keydown", async (e) => {
  if (!(e.metaKey || e.ctrlKey) || e.key.toLowerCase() !== "s") return;
  e.preventDefault();
  if (!lastGraph) return;

  const step = e.shiftKey;
  setStatus(step ? "exporting STEP" : "exporting STL", "busy");
  try {
    const path = step
      ? await invoke<string>("export_step", { graph: lastGraph, path: "part.step" })
      : await invoke<string>("export_stl", {
          graph: lastGraph,
          depth: Number(depthInput.value),
          path: "part.stl",
          backend: backendSelect.value,
        });
    setStatus(`exported ${path}`);
    clearError();
  } catch (err) {
    showError(err);
    setStatus("export failed", "failed");
  }
});

run();
