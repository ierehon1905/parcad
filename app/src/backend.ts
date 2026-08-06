/**
 * The one way the editor reaches the geometry backends.
 *
 * There are two transports and deliberately no third behaviour. Inside the
 * desktop webview a call goes over Tauri IPC; in a browser it goes to the API
 * the desktop process hosts on a local port, which lands in the same Rust
 * functions. Nothing above this module may branch on which one it got — the
 * browser build used to serve a frozen geometry fixture, and every feature the
 * editor gated on `inTauri` was a difference the user had to learn.
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
 * Ask the host over HTTP.
 *
 * Same-origin: the page was served by the host it is calling, so there is no
 * port to configure and no cross-origin request to permit. Under `vite dev` the
 * dev server proxies `/api` to the desktop port, which keeps that true.
 */
async function post<T>(route: string, body: unknown): Promise<T> {
  let response: Response;
  try {
    response = await fetch(`/api/${route}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    });
  } catch {
    throw new Error(
      "the parcad desktop process is not answering.\n" +
        "It hosts this page and its geometry backends; start it with:\n" +
        "  cd app && bun run tauri dev",
    );
  }

  if (!response.ok) {
    // The service layer writes messages that name the fix. Show them whole
    // rather than replacing them with a status code.
    const detail = await response
      .json()
      .then((body) => (body as { error?: string }).error)
      .catch(() => undefined);
    throw new Error(detail ?? `the host refused the request (${response.status})`);
  }
  return response.json() as Promise<T>;
}

/** Same failure wording as `post`, for the routes that only read. */
async function send<T>(route: string, init: RequestInit): Promise<T> {
  let response: Response;
  try {
    response = await fetch(`/api/${route}`, init);
  } catch {
    throw new Error(
      "the parcad desktop process is not answering.\n" +
        "It hosts this page and its geometry backends; start it with:\n" +
        "  cd app && bun run tauri dev",
    );
  }
  if (!response.ok) {
    const detail = await response
      .json()
      .then((body) => (body as { error?: string }).error)
      .catch(() => undefined);
    throw new Error(detail ?? `the host refused the request (${response.status})`);
  }
  return response.json() as Promise<T>;
}

const get = <T>(route: string) => send<T>(route, { method: "GET" });

const put = <T>(route: string, body: unknown) =>
  send<T>(route, {
    method: "PUT",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });

async function download(route: string, body: unknown): Promise<Blob> {
  const response = await fetch(`/api/${route}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!response.ok) {
    const detail = await response
      .json()
      .then((body) => (body as { error?: string }).error)
      .catch(() => undefined);
    throw new Error(detail ?? `the export failed (${response.status})`);
  }
  return response.blob();
}

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
}

export function listProjects(): Promise<ProjectList> {
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
  const project = inTauri
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
  const saved = inTauri
    ? await invoke<{ path: string }>("save_project", { name, ...body })
    : await put<{ path: string }>(`projects/${route(name)}`, body);
  return saved.path;
}

/** Refuses to overwrite. "New part" and "save" must not be the same call. */
export async function createProject(name: string, script: string): Promise<string> {
  const made = inTauri
    ? await invoke<{ path: string }>("create_project", { name, script })
    : await post<{ path: string }>(`projects/${route(name)}`, { op: "create", script });
  return made.path;
}

export async function createFolder(name: string): Promise<string> {
  const made = inTauri
    ? await invoke<{ path: string }>("create_folder", { name })
    : await post<{ path: string }>(`projects/${route(name)}`, { op: "folder" });
  return made.path;
}

/** Renames or moves; the two are one operation on disk and one here. */
export async function renameProject(name: string, to: string): Promise<string> {
  const moved = inTauri
    ? await invoke<{ path: string }>("rename_project", { name, to })
    : await post<{ path: string }>(`projects/${route(name)}`, { op: "rename", to });
  return moved.path;
}

/** The readable name, which is not the path. */
export async function setProjectTitle(name: string, title: string): Promise<void> {
  if (inTauri) {
    await invoke("set_project_title", { name, title });
    return;
  }
  await post(`projects/${route(name)}`, { op: "title", title });
}

/** Moves to the project folder's `.trash`. Not an unlink — say so in the UI. */
export async function deleteProject(name: string): Promise<string> {
  const gone = inTauri
    ? await invoke<{ trashed: string }>("delete_project", { name })
    : await send<{ trashed: string }>(`projects/${route(name)}`, { method: "DELETE" });
  return gone.trashed;
}

/** Loose `.js` to `.parcad` folder, keeping the script byte for byte. */
export async function convertProject(name: string): Promise<string> {
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

export function mcpStatus(): Promise<McpStatus> {
  return inTauri ? invoke<McpStatus>("mcp_status") : get<McpStatus>("mcp");
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

export function getSession(): Promise<Session> {
  return inTauri ? invoke<Session>("get_session") : get<Session>("session");
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
): Promise<Session> {
  return inTauri
    ? invoke<Session>("push_session", { name, script, origin })
    : post<Session>("session", { name, script, origin });
}

/**
 * Session changes as they happen — one broadcast, two transports. In a browser
 * this is SSE from the host (`EventSource` reconnects on its own); in the
 * webview it is the Tauri event the host forwards, because the webview's
 * origin cannot open an EventSource against `/api`. Callers never learn which:
 * that is this module's whole job.
 */
export function subscribeSession(onEvent: (session: Session) => void): void {
  if (inTauri) {
    void listen<Session>("session-changed", (event) => onEvent(event.payload));
    return;
  }
  const events = new EventSource("/api/session/events");
  events.onmessage = (event) => onEvent(JSON.parse(event.data) as Session);
}

export function evaluate<T>(graph: unknown, depth: number, backend: string): Promise<T> {
  return inTauri
    ? invoke<T>("evaluate", { graph, depth, backend })
    : post<T>("evaluate", { graph, depth, backend });
}

export function inspectEdgeTarget<T>(graph: unknown, node: number): Promise<T> {
  return inTauri
    ? invoke<T>("inspect_edge_target", { graph, node })
    : post<T>("inspect-edge-target", { graph, node });
}

/** Where an export ended up, phrased for the status line. */
export async function exportStl(
  graph: unknown,
  depth: number,
  backend: string,
): Promise<string> {
  if (inTauri) {
    return invoke<string>("export_stl", { graph, depth, path: "part.stl", backend });
  }
  save(await download("export/stl", { graph, depth, backend }), "part.stl");
  return "part.stl";
}

export async function exportStep(graph: unknown): Promise<string> {
  if (inTauri) {
    return invoke<string>("export_step", { graph, path: "part.step" });
  }
  save(await download("export/step", { graph }), "part.step");
  return "part.step";
}
