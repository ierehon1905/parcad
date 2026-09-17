/**
 * The tab's host, as the page sees it: `http.rs`'s routes answered by the same
 * Rust in a Web Worker (`host-worker.ts`), and the relay an agent reaches it by.
 *
 * Imported only by `backend.ts`, which hands `fetch` the same route and
 * request it would send a desktop host and reads the same `Response` back. The
 * page's other job is the kernel: the host asks for a request packet to be
 * run, and `kernel.ts` runs it in the kernel's own worker.
 */

import type { FromHost, HostEvent, HostReply, RelayState, ToHost } from "./host-worker";
import { run } from "./kernel";

/** Where the build put the host module. See `vite.config.ts`. */
declare const __PARCAD_HOST__: { script: string; wasm: string };

/** The seed parts, bundled at build time from `examples/`, the folder the desktop seeds from. */
const SEEDS = import.meta.glob("../../../examples/**/*.js", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

/** The part a first visit opens. */
const PREFERRED = "twisted-planter";

let worker: Worker | undefined;
let started: Promise<Worker> | undefined;
let nextId = 0;
const replies = new Map<number, (reply: HostReply) => void>();
const sessionListeners = new Set<(event: Extract<HostEvent, { kind: "session" }>["session"]) => void>();
const mcpListeners = new Set<() => void>();
const relayListeners = new Set<(state: RelayState) => void>();
let relayState: RelayState = { kind: "off" };
let relayWanted: { relay: string; key: string } | null = null;

const url = (path: string) => new URL(path, document.baseURI).href;

function launch(): Promise<Worker> {
  const next = new Worker(new URL("./host-worker.ts", import.meta.url), { type: "module" });
  const ready = new Promise<Worker>((resolve, reject) => {
    next.onmessage = (event: MessageEvent<FromHost>) => {
      const message = event.data;
      switch (message.kind) {
        case "ready":
          resolve(next);
          if (relayWanted) next.postMessage({ kind: "relay", ...relayWanted } satisfies ToHost);
          break;
        case "failed":
          reject(new Error(`this tab could not start its host: ${message.error}`));
          break;
        case "reply":
          replies.get(message.id)?.(message.reply);
          replies.delete(message.id);
          break;
        case "kernel":
          void run(message.packet, message.timeoutMs).then((end) =>
            next.postMessage(
              { kind: "kernel-end", ticket: message.ticket, end } satisfies ToHost,
              end.kind === "replied" ? [end.packet.buffer] : [],
            ),
          );
          break;
        case "events":
          for (const e of message.events) {
            if (e.kind === "session") for (const listener of sessionListeners) listener(e.session);
            else if (e.kind === "mcp") for (const listener of mcpListeners) listener();
          }
          break;
        case "download":
          download(new Blob([message.bytes as BlobPart]), message.name);
          break;
        case "relay":
          relayState = message.state;
          for (const listener of relayListeners) listener(relayState);
          break;
      }
    };
    next.onerror = (event) => reject(new Error(`this tab's host did not load (${event.message})`));
  });
  const seeds = Object.fromEntries(
    Object.entries(SEEDS).map(([file, script]) => [file.replace(/^.*\/examples\//, "").replace(/\.js$/, ""), script]),
  );
  next.postMessage({
    kind: "start",
    script: url(__PARCAD_HOST__.script),
    wasm: url(__PARCAD_HOST__.wasm),
    viewer: url("viewer.html"),
    seeds,
    preferred: PREFERRED,
  } satisfies ToHost);
  worker = next;
  return ready;
}

function host(): Promise<Worker> {
  started ??= launch().catch((e) => {
    started = undefined;
    throw e;
  });
  return started;
}

/** Start the host now, rather than on the first request. */
export function preload() {
  void host().catch(() => {});
}

async function call(path: string, method: string, body: unknown): Promise<HostReply> {
  const current = await host();
  const id = nextId++;
  return new Promise<HostReply>((resolve) => {
    replies.set(id, (reply) => {
      // A host that failed inside the tab has state nobody should trust; the next call starts another.
      if (reply.kind === "refused" && reply.status === 500 && reply.error.startsWith("the host failed inside the tab")) {
        if (worker === current) {
          current.terminate();
          worker = undefined;
          started = undefined;
        }
      }
      resolve(reply);
    });
    current.postMessage({ kind: "call", id, input: { route: { method, path, body } } } satisfies ToHost);
  });
}

/** One of the host's routes, answered as the desktop host answers it over HTTP. */
export async function request(route: string, init: RequestInit = {}): Promise<Response> {
  const method = (init.method ?? "GET").toUpperCase();
  const body = typeof init.body === "string" ? JSON.parse(init.body) : null;
  const path = route.split("/").map(decodeURIComponent).join("/");
  const reply = await call(path, method, body);
  switch (reply.kind) {
    case "json":
      return new Response(JSON.stringify(reply.json), { headers: { "content-type": "application/json" } });
    case "bytes":
      return new Response(reply.bytes as BlobPart, { headers: { "content-type": reply.contentType } });
    case "refused":
      return new Response(JSON.stringify({ error: reply.error }), {
        status: reply.status,
        headers: { "content-type": "application/json" },
      });
    case "meshed":
      return new Response(JSON.stringify(unpackMeshed(reply.bytes)), { headers: { "content-type": "application/json" } });
  }
}

/** An evaluation, with its mesh left as the arrays it arrived as. */
export async function evaluate(graph: unknown): Promise<unknown> {
  const reply = await call("evaluate", "POST", { graph });
  if (reply.kind === "refused") throw new Error(reply.error);
  if (reply.kind !== "meshed") throw new Error("the host answered an evaluation without its mesh");
  return unpackMeshed(reply.bytes);
}

/** `meshed` in crates/parcad-host/src/page.rs: lengths, JSON, then the arrays. */
function unpackMeshed(payload: Uint8Array) {
  const buffer = payload.buffer.slice(payload.byteOffset, payload.byteOffset + payload.byteLength);
  const [textLen, positionsLen, normalsLen, indicesLen] = new Uint32Array(buffer, 0, 4);
  const json = JSON.parse(new TextDecoder().decode(new Uint8Array(buffer, 16, textLen)));
  let at = 16 + Math.ceil(textLen / 4) * 4;
  const take = (count: number) => buffer.slice(at, (at += 4 * count));
  json.positions = new Float32Array(take(positionsLen));
  json.normals = new Float32Array(take(normalsLen));
  json.indices = new Uint32Array(take(indicesLen));
  return json;
}

function download(blob: Blob, name: string) {
  const href = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = href;
  link.download = name;
  link.click();
  setTimeout(() => URL.revokeObjectURL(href), 10_000);
}

/** Every change to the session the host makes, including an agent's. */
export function onSession(listener: (session: { name: string | null; script: string; revision: number; origin: string }) => void) {
  sessionListeners.add(listener);
  preload();
  return () => sessionListeners.delete(listener);
}

/** Every request an agent makes, as it arrives. */
export function onMcp(listener: () => void) {
  mcpListeners.add(listener);
  return () => mcpListeners.delete(listener);
}

// ------------------------------------------------------------------ the relay

export function watchRelay(listener: (state: RelayState) => void): () => void {
  relayListeners.add(listener);
  listener(relayState);
  return () => relayListeners.delete(listener);
}

/** Open this tab to agents through `relay`, under `key`; null closes it. */
export async function useRelay(relay: string | null, key: string): Promise<void> {
  relayWanted = relay ? { relay, key } : null;
  const current = await host();
  current.postMessage({ kind: "relay", relay, key } satisfies ToHost);
}
