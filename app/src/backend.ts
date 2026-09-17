/**
 * The one way the editor reaches the host.
 *
 * There are three transports and deliberately no third behaviour. Inside the
 * desktop webview a call goes over Tauri IPC; in a browser it goes to the API
 * the desktop process hosts on a local port, which lands in the same Rust
 * functions; and in the playground build there is no host at all, so the same
 * Rust — `parcad_occt::serve` and `parcad_evaluation`, compiled to WebAssembly
 * — runs in a Web Worker in the tab (`page/kernel.ts`) and parts live in the
 * browser's storage (`page/store.ts`). Nothing above this module may branch on
 * which one it got — the browser build used to serve a frozen geometry
 * fixture, and every feature the editor gated on `inTauri` was a difference
 * the user had to learn.
 *
 * The one honest difference is where an export goes, and it is about the host,
 * not the model: the desktop writes a file, the browser downloads one. Both are
 * the same bytes from the same kernel, so `export*` reports what happened
 * rather than exposing the distinction as a capability.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/**
 * Is the Tauri bridge present, or are we a browser talking to the host?
 *
 * Deliberately not exported. It selects a transport and must never become a
 * condition anywhere else: the moment it is importable, it is a capability
 * check, and the two hosts start to differ again.
 */
const inTauri = "__TAURI_INTERNALS__" in window || "__TAURI__" in window;

/**
 * The playground: a static site with the kernel in the page. Fixed when the
 * site is built (`vite build --mode playground`), so the desktop bundle carries
 * none of it, and not exported for the same reason `inTauri` is not.
 */
const inPage = import.meta.env.VITE_PARCAD_PAGE === "1";

// The condition is repeated rather than read from `inPage` so the bundler can see
// it is constant and leave the page's modules out of every other build.
const page = (): Promise<typeof import("./page")> =>
  import.meta.env.VITE_PARCAD_PAGE === "1"
    ? import("./page")
    : Promise.reject(new Error("the page kernel exists only in the playground build"));

const NOT_ANSWERING =
  "the parcad desktop process is not answering.\n" +
  "It hosts this page and its geometry kernel; start it with:\n" +
  "  cd app && bun run tauri dev";

const sending = (method: string, body: unknown): RequestInit => ({
  method,
  headers: { "content-type": "application/json" },
  body: JSON.stringify(body),
});

/**
 * Ask the host over HTTP.
 *
 * Same-origin: the page was served by the host it is calling, so there is no
 * port to configure and no cross-origin request to permit. Under `vite dev` the
 * dev server proxies `/api` to the desktop port, which keeps that true.
 */
async function request(route: string, init: RequestInit, refused: string): Promise<Response> {
  let response: Response;
  try {
    response = await fetch(`/api/${route}`, init);
  } catch {
    throw new Error(NOT_ANSWERING);
  }
  if (!response.ok) {
    // The service layer writes messages that name the fix. Show them whole
    // rather than replacing them with a status code.
    const detail = await response
      .json()
      .then((body) => (body as { error?: string }).error)
      .catch(() => undefined);
    throw new Error(detail ?? `${refused} (${response.status})`);
  }
  return response;
}

const send = async <T>(route: string, init: RequestInit): Promise<T> =>
  (await request(route, init, "the host refused the request")).json() as Promise<T>;

const post = <T>(route: string, body: unknown) => send<T>(route, sending("POST", body));
const get = <T>(route: string) => send<T>(route, { method: "GET" });
const put = <T>(route: string, body: unknown) => send<T>(route, sending("PUT", body));

const download = async (route: string, body: unknown): Promise<Blob> =>
  (await request(route, sending("POST", body), "the export failed")).blob();

function save(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  URL.revokeObjectURL(url);
}

/** One part, as much as the host can say without evaluating it. */
export interface ProjectPart {
  kind: "part";
  /** The leaf, for a label. */
  name: string;
  /** Slash-separated from the project root. This is the id everything takes. */
  path: string;
  /** The manifest's title, or the file stem made readable. Never empty. */
  title: string;
  /** False for a loose `.js`, which cannot carry a title or a thumbnail. */
  bundle: boolean;
  /** Whether there is a `preview.png` to ask for. */
  thumbnail: boolean;
  tags: string[];
  /** Seconds since the epoch, from the file itself. */
  modified: number | null;
}

export interface ProjectFolder {
  kind: "folder";
  name: string;
  path: string;
  children: ProjectEntry[];
}

export type ProjectEntry = ProjectFolder | ProjectPart;

export interface ProjectList {
  /** Every part, flattened — the same paths MCP lists. */
  projects: string[];
  /** The same parts with their folders, which is what the picker draws. */
  tree: ProjectEntry[];
  /** The folder on disk, so the UI can tell the user where their parts are. */
  directory: string;
  /** The part to open on a first visit, when this transport has an opinion. */
  preferred?: string;
}

export function listProjects(): Promise<ProjectList> {
  if (inPage) return page().then((p) => p.store.list());
  return inTauri ? invoke<ProjectList>("list_projects") : get<ProjectList>("projects");
}

/**
 * A project path in a URL.
 *
 * `encodeURIComponent` would escape the separators, and the host's route is a
 * wildcard that expects real ones — a part in a folder would 404. Each segment
 * is escaped instead. The host re-validates whatever arrives; this is about
 * addressing, not safety.
 */
const route = (name: string) => name.split("/").map(encodeURIComponent).join("/");

export async function readProject(name: string): Promise<string> {
  const project = inPage
    ? await (await page()).store.read(name)
    : inTauri
    ? await invoke<{ script: string }>("read_project", { name })
    : await get<{ script: string }>(`projects/${route(name)}`);
  return project.script;
}

/**
 * The script, and the two derived files that live beside it.
 *
 * They travel in one call because a README describing a shape the script no
 * longer builds is worse than no README. Both are dropped silently for a loose
 * `.js`, which has nowhere to keep them.
 */
export interface Derived {
  readme?: string;
  /** A `data:image/png;base64,` URL from the viewport canvas. */
  preview?: string;
}

export async function saveProject(
  name: string,
  script: string,
  derived: Derived = {},
): Promise<string> {
  const body = { script, readme: derived.readme, preview: derived.preview };
  const saved = inPage
    ? await (await page()).store.save(name, script, derived.preview)
    : inTauri
    ? await invoke<{ path: string }>("save_project", { name, ...body })
    : await put<{ path: string }>(`projects/${route(name)}`, body);
  return saved.path;
}

/** Refuses to overwrite. "New part" and "save" must not be the same call. */
export async function createProject(name: string, script: string): Promise<string> {
  const made = inPage
    ? await (await page()).store.create(name, script)
    : inTauri
    ? await invoke<{ path: string }>("create_project", { name, script })
    : await post<{ path: string }>(`projects/${route(name)}`, { op: "create", script });
  return made.path;
}

export async function createFolder(name: string): Promise<string> {
  const made = inPage
    ? await (await page()).store.createFolder(name)
    : inTauri
    ? await invoke<{ path: string }>("create_folder", { name })
    : await post<{ path: string }>(`projects/${route(name)}`, { op: "folder" });
  return made.path;
}

/** Renames or moves; the two are one operation on disk and one here. */
export async function renameProject(name: string, to: string): Promise<string> {
  const moved = inPage
    ? await (await page()).store.rename(name, to)
    : inTauri
    ? await invoke<{ path: string }>("rename_project", { name, to })
    : await post<{ path: string }>(`projects/${route(name)}`, { op: "rename", to });
  return moved.path;
}

/** The readable name, which is not the path. */
export async function setProjectTitle(name: string, title: string): Promise<void> {
  if (inPage) return (await page()).store.setTitle(name, title);
  if (inTauri) {
    await invoke("set_project_title", { name, title });
    return;
  }
  await post(`projects/${route(name)}`, { op: "title", title });
}

/** Moves to the project folder's `.trash`. Not an unlink — say so in the UI. */
export async function deleteProject(name: string): Promise<string> {
  const gone = inPage
    ? await (await page()).store.remove(name)
    : inTauri
    ? await invoke<{ trashed: string }>("delete_project", { name })
    : await send<{ trashed: string }>(`projects/${route(name)}`, { method: "DELETE" });
  return gone.trashed;
}

/** Loose `.js` to `.parcad` folder, keeping the script byte for byte. */
export async function convertProject(name: string): Promise<string> {
  if (inPage) throw new Error(`${JSON.stringify(name)} is already a project folder.`);
  const made = inTauri
    ? await invoke<{ path: string }>("convert_project", { name })
    : await post<{ path: string }>(`projects/${route(name)}`, { op: "convert" });
  return made.path;
}

/**
 * Write a thumbnail without touching the script.
 *
 * The app does this the first time it draws a part that has none, so browsing
 * fills the picker in. Going through `saveProject` would rewrite `part.js` —
 * and its modified time, which the picker reports — to store a picture.
 */
export async function saveProjectPreview(name: string, preview: string): Promise<void> {
  if (inPage) return (await page()).store.setPreview(name, preview);
  if (inTauri) {
    await invoke("save_project_preview", { name, preview });
    return;
  }
  await put(`preview/${route(name)}`, { preview });
}

/**
 * A part's thumbnail as something an `<img>` can take, or null when there is
 * none. Asked for one card at a time: the listing stays small, and a folder of
 * a hundred parts does not become a megabyte of base64 to draw a dozen tiles.
 */
export async function projectPreview(name: string): Promise<string | null> {
  try {
    if (inPage) return await (await page()).store.preview(name);
    if (inTauri) return await invoke<string>("project_preview", { name });
    const response = await fetch(`/api/preview/${route(name)}`);
    if (!response.ok) return null;
    const blob = await response.blob();
    return URL.createObjectURL(blob);
  } catch {
    // A missing thumbnail is not an error worth showing anyone; the card falls
    // back to drawing the part's name.
    return null;
  }
}

/** What the host has seen an agent do on the MCP endpoint it also serves. */
export interface McpStatus {
  /** Sessions that handshook and have not hung up or gone silent. */
  clients: number;
  /** The most recent client's own name and version. */
  client: string | null;
  tool_calls: number;
  last_tool: string | null;
  /** Seconds since the last request; null if there has never been one. */
  idle_secs: number | null;
  url: string;
}

/** Whether an agent can reach this kernel over MCP at all; the playground has no endpoint. */
export const mcpServedHere = !inPage;

export function mcpStatus(): Promise<McpStatus> {
  // No host, so no endpoint: refused like an unreachable one, which hides the chip.
  if (inPage) return Promise.reject(new Error("the playground has no MCP endpoint"));
  return inTauri ? invoke<McpStatus>("mcp_status") : get<McpStatus>("mcp");
}

/** A release newer than the running app. */
export interface AvailableUpdate {
  version: string;
  /** The version running now. */
  current: string;
  notes: string | null;
}

/**
 * Whether the app hosting this window has a newer release. Only the desktop
 * app can replace itself: a browser tab, `parcad serve` (updated by its
 * package manager) and the playground answer that there is nothing to install.
 */
export function checkForUpdate(): Promise<AvailableUpdate | null> {
  return inTauri ? invoke<AvailableUpdate | null>("check_for_update") : Promise.resolve(null);
}

/** Install the update `checkForUpdate` announced and relaunch; resolves only on failure paths. */
export function installUpdate(): Promise<void> {
  return inTauri
    ? invoke<void>("install_update")
    : Promise.reject(new Error("only the desktop app can update itself"));
}

/**
 * The live session: which part is on screen and what its script says, shared
 * by every window and by an agent over MCP. The state and the event are the
 * same shape on purpose — a viewer that missed a broadcast asks for the state
 * and treats the answer identically.
 */
export interface Session {
  /** The open project's path, or null before anything is opened. */
  name: string | null;
  /** The document being typed — not necessarily the file on disk. */
  script: string;
  /** Increases with every real change. */
  revision: number;
  /** The viewer that made the change: a window's id, or "agent" over MCP. */
  origin: string;
}

/**
 * Tell the host what this window is showing. Called on the evaluation
 * debounce, so `get_session` answers with what the user actually typed rather
 * than the last thing an agent wrote. An unchanged document is a no-op on the
 * host — no revision bump, no broadcast — which is what stops a viewer's push
 * of an applied remote change from echoing back out.
 */
export function pushSession(
  name: string | null,
  script: string,
  origin: string,
  base: number | null,
): Promise<Session> {
  // One window and no agent: the session is this tab's own, and nothing echoes.
  if (inPage) return Promise.resolve({ name, script, revision: (base ?? 0) + 1, origin });
  return inTauri
    ? invoke<Session>("push_session", { name, script, origin, base })
    : post<Session>("session", { name, script, origin, base });
}

/** The session as the host holds it, for a window that has just loaded. */
export function getSession(): Promise<Session> {
  if (inPage) return Promise.reject(new Error("the playground shares no session"));
  return inTauri ? invoke<Session>("get_session") : get<Session>("session");
}

/** What this window put on screen, so an agent can tell shown from sent. */
export interface Shown {
  id: string;
  revision: number;
  built: boolean;
  error?: string;
  volume_mm3?: number;
}

export function reportShown(shown: Shown): Promise<unknown> {
  if (inPage) return Promise.resolve();
  const report = { ...shown, kind: inTauri ? "desktop" : "browser" };
  return inTauri
    ? invoke("report_shown", { shown: report })
    : post("session/shown", report);
}

/**
 * Session changes as they happen — one broadcast, two transports. In a browser
 * this is SSE from the host (`EventSource` reconnects on its own); in the
 * webview it is the Tauri event the host forwards, because the webview's
 * origin cannot open an EventSource against `/api`. Callers never learn which:
 * that is this module's whole job.
 */
export function subscribeSession(viewer: string, onEvent: (session: Session) => void): void {
  if (inPage) return;
  if (inTauri) {
    void listen<Session>("session-changed", (event) => onEvent(event.payload));
    return;
  }
  const events = new EventSource(`/api/session/events?viewer=${encodeURIComponent(viewer)}`);
  events.onmessage = (event) => onEvent(JSON.parse(event.data) as Session);
}

/**
 * What the shipped first build is replaced with, once this tab has built it.
 *
 * Only the playground ever calls back: every other transport builds the part it
 * shows. See `page/prebuilt.ts` for why one evaluation travels with the site.
 */
let onRebuilt: ((evaluated: unknown) => void) | undefined;

export function watchFirstRebuild(listener: (evaluated: unknown) => void): () => void {
  onRebuilt = listener;
  return () => (onRebuilt = undefined);
}

export async function evaluate<T>(graph: unknown, showing?: { part?: string; source: string }): Promise<T> {
  if (inPage) {
    const host = await page();
    const shipped = showing && (await host.prebuilt.take(showing.part, showing.source));
    if (shipped) {
      void host.prebuilt
        .rebuild(graph)
        .then((evaluated) => onRebuilt?.(evaluated))
        .catch(() => {});
      return shipped as T;
    }
    const already = showing && host.prebuilt.inFlight(showing.part, showing.source);
    if (already) return (await already) as T;
    return json<T>(await host.kernel.call({ op: "evaluate", graph }));
  }
  return inTauri ? invoke<T>("evaluate", { graph }) : post<T>("evaluate", { graph });
}


export async function inspectEdgeTarget<T>(graph: unknown, node: number): Promise<T> {
  if (inPage) return json<T>(await (await page()).kernel.call({ op: "inspect-edge-target", graph, node }));
  return inTauri
    ? invoke<T>("inspect_edge_target", { graph, node })
    : post<T>("inspect-edge-target", { graph, node });
}

/**
 * Where an export ended up, phrased for the status line.
 *
 * The two transports genuinely differ here and the difference is not worth
 * hiding. The desktop writes the file beside the part and asks the system to
 * reveal it, so it returns an absolute path there is a point in showing. A
 * browser cannot write anywhere and cannot open Finder; it hands the bytes to
 * the download machinery, which puts them wherever that browser puts downloads,
 * and the most this can honestly return is the file name.
 *
 * `project` is the open part's path, because the host resolves the destination
 * from the project folder. It has to: this window knows which part is open and
 * not where that folder lives.
 */
export async function exportStl(graph: unknown, project: string | undefined): Promise<string> {
  if (inTauri && project) {
    return invoke<string>("export_stl", { graph, project });
  }
  save(inPage ? await pageExport("export-stl", graph, "model/stl") : await download("export/stl", { graph }), "part.stl");
  return "part.stl";
}

export async function export3mf(graph: unknown, project: string | undefined): Promise<string> {
  if (inTauri && project) {
    return invoke<string>("export_3mf", { graph, project });
  }
  save(inPage ? await pageExport("export-3mf", graph, "model/3mf") : await download("export/3mf", { graph }), "part.3mf");
  return "part.3mf";
}

export async function exportStep(graph: unknown, project: string | undefined): Promise<string> {
  if (inTauri && project) {
    return invoke<string>("export_step", { graph, project });
  }
  save(inPage ? await pageExport("export-step", graph, "application/step") : await download("export/step", { graph }), "part.step");
  return "part.step";
}

type PageReply = { json: unknown } | { bytes: Uint8Array };

function json<T>(reply: PageReply): T {
  if (!("json" in reply)) throw new Error("the kernel answered with bytes where a reply was expected");
  return reply.json as T;
}

async function pageExport(op: string, graph: unknown, type: string): Promise<Blob> {
  const reply = await (await page()).kernel.call({ op, graph });
  if (!("bytes" in reply)) throw new Error("the kernel answered without the file");
  return new Blob([reply.bytes as BlobPart], { type });
}

/**
 * How far the kernel has got to arriving, for a page that has to download it.
 * The two host transports have their kernel before the window opens, so this
 * never calls back for them.
 */
export interface KernelLoad {
  phase: "downloading" | "compiling" | "ready" | "failed";
  received: number;
  total: number;
  error?: string;
}

export function watchKernelLoad(listener: (load: KernelLoad) => void): () => void {
  if (!inPage) return () => {};
  let stop = () => {};
  let stopped = false;
  void page().then((p) => {
    if (stopped) return;
    stop = p.kernel.watchLoad(listener);
    p.kernel.preload();
  });
  return () => {
    stopped = true;
    stop();
  };
}
