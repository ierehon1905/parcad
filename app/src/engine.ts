/**
 * Wiring: editor -> DSL -> intent graph -> Rust core -> viewport.
 *
 * The script runs here in the webview rather than in an embedded interpreter,
 * because the webview already has a very good JavaScript engine and the script
 * does no geometry — it only builds a graph. Everything expensive happens in
 * Rust.
 *
 * This file is the machinery, and it is deliberately not components. The
 * evaluation cycle is a small state machine with a debounce, an in-flight
 * guard and a coalescing flag; the live session has an echo rule that depends
 * on an identity minted once per window. None of that is a render, and the
 * Preact port left every line of it alone — the components in `ui/` observe the
 * signals in `state.ts` that these functions write.
 */

import { isolateHistory } from "@codemirror/commands";
import { EditorView } from "@codemirror/view";
import * as backend from "./backend";
import * as dsl from "./dsl";
import { Shape } from "./dsl";
import { setTreatmentHover } from "./editor-marks";
import { verticesFromEdges } from "./entities";
import { describeProjects, label as projectLabel, partAt } from "./projects";
import { instrumentTreatmentCalls, sourceOffset, treatmentAtCursor, treatmentCallRange } from "./source-link";
import { shortestUniqueSelector } from "./shortest-selector";
import * as S from "./state";
import type { EdgeCurve, Evaluated, TargetPreview } from "./state";

/** Whether the camera has been framed on this part yet. */
let framed = false;
/**
 * Which part the geometry currently on screen belongs to.
 *
 * A failed build deliberately leaves the last good part rendered, so a broken
 * keystroke does not blank the viewport. That is only defensible while the
 * geometry is still *this* part's: if the first build after opening another one
 * fails, everything on screen — the solid, its volume, its watertightness —
 * describes a part the titlebar is no longer naming.
 */
let shownPath: string | undefined;
let targetPreviewRequest = 0;
/** A clicked treatment keeps its authored chain visible after pointer-leave. */
let pinnedTreatment: dsl.TreatmentSource | undefined;

interface BuiltGraph {
  graph: dsl.Doc;
  source: string;
  treatments: dsl.TreatmentSource[];
}

/** The snapshot's bounds in the shape the viewport and the section take. */
function boundsOf(snapshot: S.EvaluationSnapshot): S.Bounds {
  const v = ([x, y, z]: [number, number, number]) => ({ x, y, z });
  return { min: v(snapshot.bounds_min), max: v(snapshot.bounds_max) };
}

// ------------------------------------------------------------ the eval cycle

let timer: number | undefined;
let running = false;
let dirty = false;

/**
 * Re-evaluate shortly after typing stops.
 *
 * Short enough to feel live, long enough that a burst of keystrokes is one
 * evaluation rather than twenty.
 *
 * 350 ms was chosen when a rebuild cost 300-450 ms, so the wait and the work
 * were the same size. The kernel now answers this window in 71-103 ms on the
 * bracket, which made the timer the longer half of the latency — you were
 * waiting on the clock rather than on the geometry. 120 ms roughly halves what
 * a keystroke feels like and still coalesces an ordinary typing burst.
 */
const DEBOUNCE_MS = 120;

export function schedule() {
  window.clearTimeout(timer);
  timer = window.setTimeout(run, DEBOUNCE_MS);
}

export async function run() {
  if (running) {
    // Coalesce: remember that something changed and pick it up on the way out.
    dirty = true;
    return;
  }
  running = true;
  dirty = false;

  S.setStatus("evaluating", "busy");

  // Tell the host what this window is showing, on the same debounce. Without
  // this an agent's get_session would report the last thing it wrote itself
  // rather than what the user has typed since. Fire and forget: geometry must
  // not wait on it, and pushing even a script that will fail to build below is
  // the point — a broken draft is still what is on screen.
  const source = S.editor().state.doc.toString();
  void backend.pushSession(S.openPath.value ?? null, source, viewerId).catch(() => {});

  try {
    const built = buildGraph(source);
    const result = await backend.evaluate<Evaluated>(
      built.graph,
      S.DEPTH,
      S.kernel.value,
    );

    S.lastGraph.value = built.graph;
    S.lastSource.value = built.source;
    S.lastTreatments.value = built.treatments;
    // Resolved targets belong to one graph. A cached count that outlived the
    // edit that changed it would be a confident wrong answer.
    resolvedTargets.clear();
    show(result);
    previewTreatmentAtCursor();
    clearError();
    const ms =
      result.snapshot.backend === "brep"
        ? result.snapshot.kernel_ms
        : result.timings.lower_and_mesh_ms + result.timings.normals_ms;
    S.setStatus(`${result.snapshot.triangles.toLocaleString()} tris · ${ms} ms`);
    void captureFirstThumbnail();
  } catch (e) {
    showError(e);
    S.setStatus("failed", "failed");
  } finally {
    running = false;
    if (dirty) schedule();
  }
}

/**
 * Run a script and flatten what it returns.
 *
 * The DSL is injected as named parameters rather than as globals, so a script
 * cannot accidentally depend on anything the page happens to have lying around.
 */
function buildGraph(source: string): BuiltGraph {
  const api = { ...dsl } as Record<string, unknown>;
  const names = Object.keys(api);

  let fn: (...args: unknown[]) => unknown;
  try {
    const executable = instrumentTreatmentCalls(S.editor().state, source);
    fn = new Function(...names, `${executable}\n//# sourceURL=parcad-editor.js`) as (
      ...args: unknown[]
    ) => unknown;
  } catch (e) {
    throw new Error(`the script did not parse:\n${(e as Error).message}`);
  }

  const result = fn(...names.map((n) => api[n]));
  if (!(result instanceof Shape)) {
    throw new Error(
      "the script must return a shape.\n" + "End it with something like:  return body.cut(hole)",
    );
  }
  const treatments: dsl.TreatmentSource[] = [];
  return { graph: dsl.build(result, treatments), source, treatments };
}

function show(result: Evaluated) {
  const snapshot = result.snapshot;
  shownPath = S.openPath.value;
  S.snapshot.value = snapshot;
  const edges = result.edges;
  S.visibleEdges.value = edges;
  S.visibleVertices.value = verticesFromEdges(edges);
  const bounds = boundsOf(snapshot);

  S.viewportRef.current?.setGeometry(
    {
      positions: new Float32Array(result.positions),
      normals: new Float32Array(result.normals),
      indices: new Uint32Array(result.indices),
      edges,
      faceRuns: result.face_runs,
    },
    bounds,
  );

  // Frame once, then leave the camera alone — nothing is more irritating than a
  // view that resets itself every time you change a number.
  if (!framed) {
    S.viewportRef.current?.frameAll(bounds);
    framed = true;
  }

  // The plane's travel is the part's own extent, so the section covers exactly
  // the cuts that can show anything and no more.
  S.bounds.value = bounds;

  S.lastFaces.value = result.faces;

  S.linkedEdgeCount.value = edges.filter((edge) => treatmentForEdge(edge)).length;
  S.linkedMethods.value = [
    ...new Set(
      edges
        .map(treatmentForEdge)
        .filter((treatment): treatment is dsl.TreatmentSource => !!treatment)
        .map((treatment) => `.${treatment.source?.method ?? treatment.kind}`),
    ),
  ];
}

// --------------------------------------------------------- treatment targets

/**
 * Exact targets already resolved for the current graph, keyed by node.
 *
 * The cursor preview and the hover tooltip ask the same question of the same
 * node, and the answer costs a worker round trip that rebuilds the treatment's
 * child. Cleared whenever a new graph evaluates.
 */
const resolvedTargets = new Map<number, Promise<TargetPreview>>();

export function resolveTarget(node: number): Promise<TargetPreview> {
  const cached = resolvedTargets.get(node);
  if (cached) return cached;

  const pending = backend
    .inspectEdgeTarget<TargetPreview>(S.lastGraph.value, node)
    .then((preview) => {
      if (preview.node !== node) throw new Error("the worker returned a different treatment");
      return preview;
    });
  // A failed resolve is not cached: the next hover should ask again rather
  // than repeat a stale failure after the model is fixed.
  pending.catch(() => resolvedTargets.delete(node));
  resolvedTargets.set(node, pending);
  return pending;
}

/** Clear an in-flight or displayed source-to-viewport target preview. */
function clearTargetPreview() {
  targetPreviewRequest++;
  S.viewportRef.current?.setTargetPreview([]);
  S.targetPreview.value = undefined;
}

/** Resolve and highlight the treatment call under the editor cursor. */
export async function previewTreatmentAtCursor() {
  const request = ++targetPreviewRequest;
  const editor = S.editor();
  const source = editor.state.doc.toString();
  const treatment =
    source === S.lastSource.value
      ? treatmentAtCursor(editor.state, source, S.lastTreatments.value, editor.state.selection.main.head)
      : undefined;
  if (!treatment || !S.lastGraph.value) {
    if (request === targetPreviewRequest) clearTargetPreview();
    return;
  }

  const method = treatment.source!.method;
  S.viewportRef.current?.setTargetPreview([]);
  S.targetPreview.value = { method, detail: "resolving exact target…", resolved: false };

  try {
    const preview = await resolveTarget(treatment.node);
    if (request !== targetPreviewRequest) return;
    const vertices = preview.vertices ?? [];
    S.viewportRef.current?.setTargetPreview(preview.edges, vertices);
    const entities = [
      vertices.length > 0 && `${vertices.length} selected corner${vertices.length === 1 ? "" : "s"}`,
      `${preview.edges.length} selected edge${preview.edges.length === 1 ? "" : "s"}`,
    ].filter(Boolean);
    S.targetPreview.value = {
      method: vertices.length > 0 ? `${method} input corner and edges` : `${method} input edges`,
      detail: entities.join(" · "),
      resolved: true,
    };
  } catch {
    if (request !== targetPreviewRequest) return;
    clearTargetPreview();
  }
}

// ---------------------------------------------------- source <-> viewport

/** The source call whose exact builder history generated this final edge. */
export function treatmentForEdge(edge: EdgeCurve): dsl.TreatmentSource | undefined {
  return edge.treatment_node === undefined
    ? undefined
    : S.lastTreatments.value.find((treatment) => treatment.node === edge.treatment_node);
}

/** Mark the source call for a final edge without stealing keyboard focus. */
export function highlightTreatmentForEdge(edge: EdgeCurve | undefined) {
  const editor = S.editor();
  const treatment =
    edge && editor.state.doc.toString() === S.lastSource.value ? treatmentForEdge(edge) : undefined;
  if (!treatment && pinnedTreatment) return;
  const range = treatmentRange(treatment);
  editor.dispatch({ effects: setTreatmentHover.of(range) });
  if (range && !rangeIsVisible(range)) {
    editor.dispatch({ effects: EditorView.scrollIntoView(range.from, { y: "center" }) });
  }
}

function rangeIsVisible(range: { from: number; to: number }): boolean {
  const editor = S.editor();
  const start = editor.coordsAtPos(range.from);
  const end = editor.coordsAtPos(range.to);
  const pane = editor.scrollDOM.getBoundingClientRect();
  return !!start && !!end && start.top >= pane.top && end.bottom <= pane.bottom;
}

/** Select and reveal a treatment call after its generated edge is clicked. */
export function focusTreatmentForEdge(edge: EdgeCurve | undefined) {
  const editor = S.editor();
  if (!edge || editor.state.doc.toString() !== S.lastSource.value) {
    clearPinnedTreatment();
    return;
  }
  const treatment = treatmentForEdge(edge);
  const range = treatmentRange(treatment);
  if (!treatment?.source || !range) {
    clearPinnedTreatment();
    return;
  }

  const start = sourceOffset(S.lastSource.value, treatment.source.line, treatment.source.column);
  if (start === undefined) return;
  pinnedTreatment = treatment;
  editor.dispatch({
    selection: { anchor: start, head: start + treatment.source.method.length },
    effects: [setTreatmentHover.of(range), EditorView.scrollIntoView(start, { y: "center" })],
  });
}

export function treatmentRange(treatment: dsl.TreatmentSource | undefined) {
  return treatment?.source
    ? treatmentCallRange(S.editor().state, S.lastSource.value, treatment.source)
    : undefined;
}

function clearPinnedTreatment() {
  pinnedTreatment = undefined;
  S.editor().dispatch({ effects: setTreatmentHover.of(undefined) });
}

/** Forget everything about the caret's treatment; a keystroke invalidated it. */
export function forgetTreatmentPreview() {
  clearTargetPreview();
  pinnedTreatment = undefined;
}

// ------------------------------------------------------------- selectors

export function directionLabel(direction: [number, number, number]): string {
  const components = direction.map(Math.abs);
  const index = components.indexOf(Math.max(...components));
  return `|${["X", "Y", "Z"][index]}`;
}

/**
 * Derive the shortest `>X`, `<Y`, `|Z` conjunction that identifies this
 * currently visible edge.
 */
export function suggestEdgeSelector(edge: EdgeCurve, all: EdgeCurve[]): string | undefined {
  return shortestUniqueSelector(edge, all, (e) => e.center, 1e-4, {
    of: (e) => (e.direction ? alignedAxis(e.direction) : undefined),
    matches: (candidate, axis) =>
      candidate.direction !== null && Math.abs(candidate.direction[axis]) >= ALIGNED,
  });
}

/** How square to an axis a direction must be before it is called that axis. */
const ALIGNED = 0.999;

function alignedAxis(direction: [number, number, number]): string | undefined {
  const components = direction.map(Math.abs);
  const axis = components.indexOf(Math.max(...components));
  return components[axis] >= ALIGNED ? `|${["X", "Y", "Z"][axis]}` : undefined;
}

// ------------------------------------------------------------------ errors

export function showError(e: unknown) {
  S.errorText.value = e instanceof Error ? e.message : String(e);
  // Leave the last good geometry on screen. A broken edit mid-typing should not
  // blank the viewport — but only while that geometry is still this part's.
  // Once another part is open, the solid and every measurement beside it
  // describe something the titlebar is no longer naming, and a stale volume
  // presented under a new name is the confident wrong answer this project
  // refuses to give.
  if (shownPath !== S.openPath.value) discardShownPart();
}

/** Drop the rendered part and everything measured from it. */
function discardShownPart() {
  shownPath = undefined;
  S.snapshot.value = undefined;
  S.lastGraph.value = null;
  S.lastSource.value = "";
  S.lastTreatments.value = [];
  S.visibleEdges.value = [];
  S.visibleVertices.value = [];
  S.lastFaces.value = undefined;
  S.hoveredFace.value = undefined;
  S.hoveredEdge.value = undefined;
  S.selectedEdge.value = undefined;
  S.hoveredVertex.value = undefined;
  S.selectedVertex.value = undefined;
  S.bounds.value = undefined;
  S.linkedEdgeCount.value = 0;
  S.linkedMethods.value = [];
  resolvedTargets.clear();
  S.viewportRef.current?.clearPart();
}

export function clearError() {
  S.errorText.value = "";
}

// ---------------------------------------------------------------- MCP status

/**
 * Whether a model is on the third transport, refreshed on a timer.
 *
 * An agent reaches the same `service.rs` this window does and writes to the
 * same project folder, and nothing on screen would otherwise say so: the part
 * you are looking at can be replaced under you by a caller you cannot see.
 *
 * Everything shown is measured — a request that arrived, a tool that was
 * called — which is why an idle client is reported with the age of its last
 * call rather than as a flat "connected". A client that was killed cannot say
 * goodbye, and claiming it is still there would be the confident wrong answer
 * this codebase refuses everywhere else.
 */
const MCP_POLL_MS = 4000;

export function watchMcp(): () => void {
  const poll = async () => {
    try {
      S.mcp.value = await backend.mcpStatus();
    } catch {
      // The host answers this window's every other call too, so a failure here
      // is not an MCP fact and must not be shown as one.
      S.mcp.value = undefined;
    }
  };
  void poll();
  const handle = window.setInterval(poll, MCP_POLL_MS);
  return () => window.clearInterval(handle);
}

// --------------------------------------------------------------- the part

/** Re-read the project folder without opening anything. */
export async function reloadProjects() {
  S.projects.value = describeProjects(await backend.listProjects());
  return S.projects.value;
}

/** The title the manifest gives a part, or its file name. */
export function titleFor(path: string): string {
  return partAt(S.projects.value?.tree ?? [], path)?.title ?? projectLabel(path);
}

/**
 * Load a part into the editor, replacing whatever is there.
 *
 * Opening the part that is *already* open, unmodified, does nothing — and that
 * is a correctness rule rather than an optimisation. The dispatch below throws
 * the document away and puts an identical one back, which costs an undo entry,
 * a re-evaluation, a re-framed camera and a full rebuild of the viewport's edge
 * lines. For a moment there is no pickable geometry, so a pointer already
 * resting on an edge is hovering nothing. `bracket.e2e.mjs` fails on exactly
 * that, and it is right to: re-opening a file you are already looking at should
 * not make the thing under your cursor disappear.
 *
 * A part whose text differs — unsaved edits, or a file changed underneath — is
 * still reloaded, because there the dispatch is the point.
 */
export async function openProject(path: string) {
  const source = await backend.readProject(path);
  const editor = S.editor();
  if (path === S.openPath.value && editor.state.doc.toString() === source) {
    clearError();
    return;
  }

  editor.dispatch({ changes: { from: 0, to: editor.state.doc.length, insert: source } });
  S.openPath.value = path;
  S.savedSource.value = source;
  S.docSource.value = source;
  framed = false;
  clearError();
}

/**
 * Write the part, and the two files that describe it, together.
 *
 * The description is built from the *measured* report rather than from the
 * script, and says which kernel measured it — a README claiming dimensions
 * nobody evaluated is the confident wrong answer this project refuses. If the
 * current source has not evaluated cleanly, the script is still saved and the
 * description is left alone rather than being rewritten from stale numbers.
 */
export async function saveOpenPart() {
  const path = S.openPath.value;
  if (!path) return;
  const source = S.editor().state.doc.toString();
  const clean = source === S.lastSource.value && S.snapshot.value !== undefined;

  S.setStatus("saving", "busy");
  try {
    await backend.saveProject(path, source, {
      readme: clean ? readmeFor(path, source) : undefined,
      preview: clean ? S.viewportRef.current?.snapshot() || undefined : undefined,
    });
    S.savedSource.value = source;
    await reloadProjects();
    S.setStatus(clean ? "saved" : "saved — description left as it was");
  } catch (e) {
    showError(e);
    S.setStatus("could not save", "failed");
  }
}

/**
 * Give a part its first thumbnail, once, from the part as it is on disk.
 *
 * Without this a picker of thumbnails shows nothing until each part has been
 * edited and saved, which is backwards: the parts worth seeing are the ones
 * nobody has touched yet. Only when the editor still matches the file — a
 * picture of a half-typed edit would be a picture of something that is not
 * there — and only when the bundle has none, so it never fights a saved one.
 */
async function captureFirstThumbnail() {
  const path = S.openPath.value;
  if (!path || S.editor().state.doc.toString() !== S.savedSource.value) return;
  const part = partAt(S.projects.value?.tree ?? [], path);
  if (!part?.bundle || part.thumbnail) return;

  const png = S.viewportRef.current?.snapshot();
  if (!png) return;
  try {
    await backend.saveProjectPreview(path, png);
    await reloadProjects();
  } catch {
    // A thumbnail nobody asked for must not become an error anybody sees.
  }
}

/** What a reader of the folder finds beside the script. */
function readmeFor(path: string, source: string): string {
  const snapshot = S.snapshot.value!;
  const [sx, sy, sz] = snapshot.size;
  // The author's own opening comment says what the part is for; nothing
  // generated here could say it better. Only the *leading* block: comments
  // further down explain one step and read as non-sequitur out of context.
  const intro: string[] = [];
  for (const line of source.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed && intro.length === 0) continue;
    if (!trimmed.startsWith("//")) break;
    intro.push(trimmed.replace(/^\/+\s?/, ""));
  }

  const kernel = snapshot.backend === "brep" ? "the exact B-rep kernel" : "the implicit backend";
  return [
    `# ${titleFor(path)}`,
    "",
    ...(intro.length ? [intro.join("\n"), ""] : []),
    "## Measured",
    "",
    `- **${fmt(sx)} × ${fmt(sy)} × ${fmt(sz)} mm**`,
    `- volume ${fmt(snapshot.volume_mm3)} mm³, area ${fmt(snapshot.area_mm2)} mm²`,
    ...(snapshot.faces !== undefined
      ? [`- ${snapshot.faces} faces, ${snapshot.topological_edges} edges`]
      : []),
    `- mesh ${snapshot.triangles.toLocaleString()} triangles, ` +
      (snapshot.watertight
        ? "watertight"
        : `NOT watertight — ${snapshot.non_manifold_edges} bad edges`),
    ...(snapshot.tags.length ? [`- tags: ${snapshot.tags.join(", ")}`] : []),
    "",
    `Measured by ${kernel} when this part was last saved, not read off the ` +
      "script. `part.js` beside this file is the source and the only thing here " +
      "that is authoritative — rebuild it rather than trusting these numbers if " +
      "it has been edited since.",
    "",
  ].join("\n");
}

export const fmt = (v: number) =>
  Math.abs(v) >= 1000 ? v.toFixed(0) : v.toFixed(2).replace(/\.00$/, "");

// -------------------------------------------------------------- exporting

/** Write one of the two files, and report where it went. */
export async function runExport(format: "stl" | "step") {
  const graph = S.lastGraph.value;
  if (!graph) return;
  S.setStatus(format === "step" ? "exporting STEP" : "exporting STL", "busy");
  try {
    const project = S.openPath.value;
    const name =
      format === "step"
        ? await backend.exportStep(graph, project)
        : await backend.exportStl(graph, S.DEPTH, S.kernel.value, project);
    // The whole path, not the file name. On the desktop it is where the file
    // actually is, and the file manager has just been opened on it; saying only
    // "exported part.stl" is what made the old export impossible to find.
    S.setStatus(`exported ${name}`);
    clearError();
  } catch (err) {
    showError(err);
    S.setStatus("export failed", "failed");
  }
}

// ------------------------------------------------------------- live session

/**
 * This window's identity in the shared session, minted fresh per load.
 *
 * Its one job is to break the echo loop: every change this window pushes
 * carries it, and the handler below drops any broadcast carrying it back. Two
 * tabs and the desktop window each apply the others' changes and never their
 * own reflection. An agent's edits carry "agent", which no window owns, so
 * they are applied everywhere — which is the point of the session.
 *
 * Not `crypto.randomUUID()`: that only exists in a secure context, and whether
 * a Tauri webview's origin counts as one is a platform detail this constant
 * must not depend on. A module-level throw here would take the whole editor
 * down, in the desktop window only, with nothing on screen to say why.
 */
const viewerId = `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;

export function subscribeSession(): void {
  backend.subscribeSession((session) => {
    if (session.origin === viewerId) return;

    const editor = S.editor();
    const current = editor.state.doc.toString();
    const opened = (session.name ?? undefined) !== S.openPath.value;
    if (opened) {
      S.openPath.value = session.name ?? undefined;
      // A name change means someone opened a project, so the script beside it
      // is what the disk had at that moment — the saved text, not a dirty edit.
      S.savedSource.value = session.script;
      framed = false;
      clearError();
    }

    if (current !== session.script) {
      // An ordinary edit, deliberately: dispatched like typing, it lands in the
      // normal undo history and Cmd-Z takes an agent's change back exactly like
      // the user's own. Only the differing span is replaced, so a cursor outside
      // it stays put while the user keeps typing. The dispatch triggers the same
      // docChanged path as a keystroke — titlebar, debounce, push — and the push
      // of an applied change is a no-op on the host, which ends the ripple.
      //
      // `isolateHistory` is what makes "one Cmd-Z" true rather than usually
      // true. CodeMirror joins changes that land within half a second of each
      // other into a single undo group, so an agent edit arriving hard on the
      // heels of your own typing would be reverted *together with your typing*
      // by one undo. That used to be hidden by a slow round trip and surfaced
      // the moment the debounce dropped to 120 ms — `session.e2e.mjs` caught
      // it. An edit by another author is a separate action whoever is quick;
      // this says so instead of relying on them being slow.
      let from = 0;
      const next = session.script;
      while (from < current.length && from < next.length && current[from] === next[from]) from++;
      let toCurrent = current.length;
      let toNext = next.length;
      while (toCurrent > from && toNext > from && current[toCurrent - 1] === next[toNext - 1]) {
        toCurrent--;
        toNext--;
      }
      editor.dispatch({
        changes: { from, to: toCurrent, insert: next.slice(from, toNext) },
        annotations: isolateHistory.of("full"),
      });
    } else if (opened) {
      // Same text, different part — a rename-shaped case the dispatch above
      // would otherwise cover. The evaluation still needs to follow.
      schedule();
    }
  });
}

/**
 * Fill the titlebar and open a part.
 *
 * Evaluation is deliberately not started before this resolves: running the
 * empty document would report "the script must return a shape", which is true
 * and useless as a first impression.
 */
export async function start() {
  S.setStatus("loading projects", "busy");
  let projects;
  try {
    projects = await reloadProjects();
  } catch (e) {
    showError(e);
    S.setStatus("failed", "failed");
    return;
  }

  if (!projects.initial) {
    S.setStatus("no projects");
    showError(
      new Error(
        `parcad's project folder is empty:\n  ${projects.directory}\n` +
          "Use New part in the picker, or put a .js file in that folder.",
      ),
    );
    return;
  }

  await openProject(projects.initial);
  void run();
}
