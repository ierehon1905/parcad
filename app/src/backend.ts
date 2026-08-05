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

export interface ProjectList {
  projects: string[];
  /** The folder on disk, so the UI can tell the user where their parts are. */
  directory: string;
}

export function listProjects(): Promise<ProjectList> {
  return inTauri ? invoke<ProjectList>("list_projects") : get<ProjectList>("projects");
}

export async function readProject(name: string): Promise<string> {
  const project = inTauri
    ? await invoke<{ script: string }>("read_project", { name })
    : await get<{ script: string }>(`projects/${encodeURIComponent(name)}`);
  return project.script;
}

export async function saveProject(name: string, script: string): Promise<string> {
  const saved = inTauri
    ? await invoke<{ path: string }>("save_project", { name, script })
    : await put<{ path: string }>(`projects/${encodeURIComponent(name)}`, { script });
  return saved.path;
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
