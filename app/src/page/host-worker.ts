/**
 * The Web Worker ParCAD web's host runs in — the tab's counterpart of the
 * desktop host process, and long-lived where the kernel's worker is not.
 *
 * It loads `crates/parcad-wasm-host`, keeps the project folder in IndexedDB,
 * and carries every call through to its answer: a call that needs the kernel
 * pauses with a request packet, which the page runs in the kernel's worker and
 * hands back, and the call is made again (`crates/parcad-host/src/page.rs`
 * says why). An agent reaches the same host through the relay socket held
 * here, so a model's requests never wait on the page's own thread.
 */

/// <reference lib="webworker" />

import type { KernelEnd } from "./kernel";

interface HostModule {
  HEAPU8: Uint8Array;
  HEAPU32: Uint32Array;
  FS: EmscriptenFs;
  _host_alloc(len: number): number;
  _host_free(ptr: number): void;
  _host_start(ptr: number, len: number): number;
  _host_link(ptr: number, len: number): void;
  _host_viewer(ptr: number, len: number): void;
  _host_call(ptr: number, len: number): number;
  _host_retry(call: number): number;
  _host_poll(call: number): number;
  _host_forget(call: number): void;
  _host_answer(ticket: number, kind: number, ms: number, ptr: number, len: number): number;
  _host_events(): number;
}

interface EmscriptenFs {
  mkdir(path: string): void;
  mount(type: unknown, options: Record<string, unknown>, path: string): void;
  syncfs(populate: boolean, done: (error: unknown) => void): void;
  writeFile(path: string, data: string | Uint8Array): void;
  readFile(path: string): Uint8Array;
  unlink(path: string): void;
  analyzePath(path: string): { exists: boolean };
  readdir(path: string): string[];
  stat(path: string): { mtime: Date };
  filesystems: Record<string, unknown>;
}

type Factory = (options: Record<string, unknown>) => Promise<HostModule>;

/** A call's end, as the page reads it. */
export type HostReply =
  | { kind: "json"; json: unknown }
  | { kind: "bytes"; contentType: string; bytes: Uint8Array }
  | { kind: "meshed"; bytes: Uint8Array }
  | { kind: "refused"; status: number; error: string };

export type RouteInput = { route: { method: string; path: string; body?: unknown } };

/** Where the link is, as the connect dialog shows it. */
export type RelayState =
  | { kind: "off" }
  | { kind: "connecting" }
  | { kind: "ready"; link: string }
  | { kind: "elsewhere" }
  | { kind: "failed"; error: string };

export type ToHost =
  | {
      kind: "start";
      script: string;
      wasm: string;
      viewer: string;
      seeds: Record<string, string>;
      preferred?: string;
    }
  | { kind: "call"; id: number; input: RouteInput }
  | { kind: "kernel-end"; ticket: number; end: KernelEnd }
  | { kind: "relay"; relay: string | null; key: string };

export type FromHost =
  | { kind: "ready" }
  | { kind: "failed"; error: string }
  | { kind: "reply"; id: number; reply: HostReply }
  | { kind: "kernel"; ticket: number; packet: Uint8Array; timeoutMs: number }
  | { kind: "events"; events: HostEvent[] }
  | { kind: "download"; name: string; bytes: Uint8Array }
  | { kind: "relay"; state: RelayState };

export type HostEvent =
  | { kind: "session"; session: { name: string | null; script: string; revision: number; origin: string } }
  | { kind: "download"; path: string }
  | { kind: "mcp" };

const scope = self as unknown as DedicatedWorkerGlobalScope;
const post = (message: FromHost, transfer: Transferable[] = []) => scope.postMessage(message, transfer);

const PROJECTS = "/parcad";
const SEEDS = "/seed";
const EXPORTS = "/downloads";

// `page.rs`'s reply kinds, as crates/parcad-wasm-host/src/exports.rs frames them.
const JSON_REPLY = 0;
const BYTES = 1;
const MESHED = 2;
const HTTP = 3;
const REFUSED = 4;
const KERNEL = 5;
const PENDING = 6;

let host: HostModule | undefined;
let ready: Promise<void> | undefined;

interface Framed {
  kind: number;
  header: any;
  payload: Uint8Array;
}

function read(ptr: number): Framed {
  const h = host!;
  const [kind, headerLen, payloadLen] = [h.HEAPU32[ptr >>> 2], h.HEAPU32[(ptr >>> 2) + 1], h.HEAPU32[(ptr >>> 2) + 2]];
  const header = headerLen ? JSON.parse(new TextDecoder().decode(h.HEAPU8.subarray(ptr + 12, ptr + 12 + headerLen))) : null;
  const payload = h.HEAPU8.slice(ptr + 12 + headerLen, ptr + 12 + headerLen + payloadLen);
  h._host_free(ptr);
  return { kind, header, payload };
}

/** Copy `bytes` into the module for a call that takes ownership of them. */
function give(bytes: Uint8Array): [number, number] {
  const ptr = host!._host_alloc(bytes.length);
  host!.HEAPU8.set(bytes, ptr);
  return [ptr, bytes.length];
}

const encode = (value: unknown) => new TextEncoder().encode(JSON.stringify(value));

// ------------------------------------------------------------------ starting

function syncfs(populate: boolean): Promise<void> {
  return new Promise((resolve, reject) => host!.FS.syncfs(populate, (error) => (error ? reject(error) : resolve())));
}

let persisting: Promise<void> = Promise.resolve();
let persistTimer: ReturnType<typeof setTimeout> | undefined;

/** Write the folder through to IndexedDB soon, one flush at a time. */
function persist() {
  if (persistTimer) clearTimeout(persistTimer);
  persistTimer = setTimeout(() => {
    persisting = persisting.then(() => syncfs(false)).catch((e) => console.warn("parcad: keeping the project folder failed", e));
  }, 250);
}

async function start(message: Extract<ToHost, { kind: "start" }>) {
  const factory = ((await import(/* @vite-ignore */ message.script)) as { default: Factory }).default;
  host = await factory({
    locateFile: () => message.wasm,
    print: () => {},
    printErr: (line: string) => console.warn(line),
  });
  const fs = host.FS;
  fs.mkdir(PROJECTS);
  fs.mount(fs.filesystems.IDBFS, {}, PROJECTS);
  await syncfs(true);
  fs.mkdir(SEEDS);
  for (const [name, script] of Object.entries(message.seeds)) {
    const segments = name.split("/");
    for (let i = 1; i < segments.length; i++) {
      const dir = `${SEEDS}/${segments.slice(0, i).join("/")}`;
      if (!fs.analyzePath(dir).exists) fs.mkdir(dir);
    }
    fs.writeFile(`${SEEDS}/${name}.js`, script);
  }
  fs.mkdir(EXPORTS);

  const migrating = await earlierParts();
  if (migrating && !fs.analyzePath(`${PROJECTS}/.seeded`).exists) {
    // Before seeding, so a part the visitor deleted in the old playground stays deleted.
    fs.writeFile(`${PROJECTS}/.seeded`, migrating.seeded.join("\n"));
  }

  const started = read(
    host._host_start(
      ...give(
        encode({
          projects_dir: PROJECTS,
          seed_dir: SEEDS,
          export_dir: EXPORTS,
          preferred: message.preferred,
        }),
      ),
    ),
  );
  if (started.kind === REFUSED) throw new Error(started.header.error);

  if (migrating) await adopt(migrating);
  persist();

  // The in-chat viewer; a site built without it simply offers none.
  const viewer = await fetch(message.viewer).then((r) => (r.ok ? r.arrayBuffer() : undefined)).catch(() => undefined);
  if (viewer) host._host_viewer(...give(new Uint8Array(viewer)));
}

// ------------------------------------------------------------------ the old store

interface EarlierPart {
  path: string;
  script: string;
  title?: string;
  preview?: string;
}

const EARLIER = "parcad-playground";
const MIGRATED = `${PROJECTS}/.migrated-from-browser-store`;

/** The parts the kernel-only page kept before its host ran here, once. */
async function earlierParts(): Promise<{ parts: EarlierPart[]; seeded: string[] } | undefined> {
  if (host!.FS.analyzePath(MIGRATED).exists) return undefined;
  const db = await new Promise<IDBDatabase | undefined>((resolve) => {
    const request = indexedDB.open(EARLIER);
    request.onupgradeneeded = () => {
      // There was none: do not leave an empty one behind.
      request.transaction?.abort();
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => resolve(undefined);
  });
  if (!db) {
    indexedDB.deleteDatabase(EARLIER);
    return undefined;
  }
  const all = <T,>(store: string, key?: string) =>
    new Promise<T>((resolve, reject) => {
      const objects = db.transaction(store).objectStore(store);
      const request = key === undefined ? objects.getAll() : objects.get(key);
      request.onsuccess = () => resolve(request.result as T);
      request.onerror = () => reject(request.error);
    });
  try {
    const parts = await all<EarlierPart[]>("parts");
    const seeded = ((await all<string[] | undefined>("meta", "seeded")) ?? []).slice();
    return { parts, seeded };
  } catch {
    return undefined;
  } finally {
    db.close();
  }
}

async function adopt(earlier: { parts: EarlierPart[] }) {
  for (const part of earlier.parts) {
    const name = encodeURIComponent(part.path).replaceAll("%2F", "/");
    const made = await run({ route: { method: "POST", path: `projects/${name}`, body: { op: "create", script: part.script } } });
    if (made.kind === "refused") {
      // A part the new folder already has keeps the text the visitor last saved.
      await run({ route: { method: "PUT", path: `projects/${name}`, body: { script: part.script } } });
    }
    if (part.title) await run({ route: { method: "POST", path: `projects/${name}`, body: { op: "title", title: part.title } } });
    if (part.preview) await run({ route: { method: "PUT", path: `preview/${name}`, body: { preview: part.preview } } });
  }
  host!.FS.writeFile(MIGRATED, new Date().toISOString());
}

// ------------------------------------------------------------------ calls

const kernelWaits = new Map<number, Promise<void>>();
const kernelEnds = new Map<number, (end: KernelEnd) => void>();
let wakeAll: (() => void)[] = [];

/** Run a ticket's request in the page's kernel, once however many calls wait on it. */
function kernel(ticket: number, packet: Uint8Array, timeoutMs: number): Promise<void> {
  let waiting = kernelWaits.get(ticket);
  if (!waiting) {
    waiting = new Promise<KernelEnd>((resolve) => kernelEnds.set(ticket, resolve)).then((end) => {
      kernelWaits.delete(ticket);
      const [kind, bytes] =
        end.kind === "replied"
          ? [0, end.packet]
          : end.kind === "crashed"
          ? [1, encode({ stage: end.stage, detail: end.detail })]
          : end.kind === "timed_out"
          ? [2, encode({ stage: end.stage, seconds: end.seconds })]
          : [3, new TextEncoder().encode(end.message)];
      const ms = end.kind === "replied" ? end.ms : 0;
      const answered = read(host!._host_answer(ticket, kind, ms, ...give(bytes)));
      if (answered.kind === REFUSED) console.warn("parcad:", answered.header.error);
    });
    kernelWaits.set(ticket, waiting);
    post({ kind: "kernel", ticket, packet, timeoutMs }, [packet.buffer]);
  }
  return waiting;
}

/** Until something happens that a pending call may be waiting for, or `ms`. */
function wake(ms: number): Promise<void> {
  return new Promise((resolve) => {
    const timer = setTimeout(done, ms);
    function done() {
      clearTimeout(timer);
      wakeAll = wakeAll.filter((w) => w !== done);
      resolve();
    }
    wakeAll.push(done);
  });
}

type McpInput = { mcp: { method: string; headers: Record<string, string>; body: string } };
type HttpOut = { status: number; headers: Record<string, string>; body: string };

/** More kernel requests than any one call makes; past it, something is asking in a loop. */
const KERNEL_ROUNDS = 16;

async function drive(framed: Framed): Promise<Framed> {
  let rounds = 0;
  for (;;) {
    if (framed.kind === KERNEL) {
      const { call, ticket, timeout_ms } = framed.header;
      if (++rounds > KERNEL_ROUNDS) {
        host!._host_forget(call);
        return {
          kind: REFUSED,
          header: { status: 500, error: `one call asked the kernel ${KERNEL_ROUNDS} times; reload the tab` },
          payload: new Uint8Array(),
        };
      }
      deliver();
      await kernel(ticket, framed.payload, timeout_ms);
      framed = read(host!._host_retry(call));
    } else if (framed.kind === PENDING) {
      const { call, wake_ms } = framed.header;
      // What it waits for is the page acting on these, so they go first.
      deliver();
      await wake(wake_ms);
      framed = read(host!._host_poll(call));
    } else {
      return framed;
    }
  }
}

/** Hand the page what the host has done so far: a change to show, a file, an agent heard from. */
function deliver(): void {
  const events = read(host!._host_events()).header as HostEvent[];
  const told: HostEvent[] = [];
  for (const event of events) {
    if (event.kind === "download") offer(event.path);
    else told.push(event);
  }
  if (told.length) post({ kind: "events", events: told });
}

function settle(): void {
  deliver();
  persist();
  // A report of what the page drew, or an agent's edit, may be what a waiting call needs.
  for (const done of wakeAll.slice()) done();
}

/** Exports kept in the tab after they are downloaded, so an agent can still probe the one it just wrote. */
const EXPORTS_KEPT = 5;

function offer(path: string) {
  const fs = host!.FS;
  try {
    // readFile copies out of the module's memory, so the copy can be handed over whole.
    const bytes = fs.readFile(path);
    post({ kind: "download", name: path.split("/").pop() ?? path, bytes }, [bytes.buffer]);
  } catch (e) {
    console.warn(`parcad: the export at ${path} could not be handed over`, e);
  }
  const kept = fs
    .readdir(EXPORTS)
    .filter((name) => name !== "." && name !== "..")
    .map((name) => ({ name, at: fs.stat(`${EXPORTS}/${name}`).mtime.getTime() }))
    .sort((a, b) => b.at - a.at);
  for (const old of kept.slice(EXPORTS_KEPT)) fs.unlink(`${EXPORTS}/${old.name}`);
}

async function run(input: RouteInput): Promise<HostReply> {
  const framed = await drive(read(host!._host_call(...give(encode(input)))));
  settle();
  switch (framed.kind) {
    case JSON_REPLY:
      return { kind: "json", json: framed.header };
    case BYTES:
      return { kind: "bytes", contentType: framed.header.content_type, bytes: framed.payload };
    case MESHED:
      return { kind: "meshed", bytes: framed.payload };
    case REFUSED:
      return { kind: "refused", status: framed.header.status, error: framed.header.error };
    default:
      return { kind: "refused", status: 500, error: `the host answered a route with reply kind ${framed.kind}` };
  }
}

async function mcp(input: McpInput): Promise<HttpOut> {
  const framed = await drive(read(host!._host_call(...give(encode(input)))));
  settle();
  if (framed.kind === HTTP) return framed.header as HttpOut;
  const error = framed.kind === REFUSED ? framed.header.error : `reply kind ${framed.kind}`;
  return { status: 500, headers: { "content-type": "text/plain" }, body: error };
}

// ------------------------------------------------------------------ the relay

/**
 * The socket an agent's requests arrive on. The relay forwards bytes and keeps
 * nothing (relay/README.md); the link it names is the only way to this tab,
 * and it is derived from a key that never leaves this browser except to open
 * the socket.
 */
let socket: WebSocket | undefined;
let relayWanted: { relay: string; key: string } | undefined;
let retryIn = 1_000;
let heartbeat: ReturnType<typeof setInterval> | undefined;

const report = (state: RelayState) => post({ kind: "relay", state });

/** Below this a reply goes as it is; gzip costs more than it saves. */
const COMPRESS_FROM = 1024;
/** A Cloudflare WebSocket message holds 32 MiB; this leaves room for the header. */
const FRAME_LIMIT = 31 * 1024 * 1024;

/**
 * A reply as one binary frame: a u32 header length, the JSON header, then the
 * body — gzipped when that is worth it, and never escaped into a JSON string.
 * A tool reply is JSON with its pictures in base64, as MCP has it, and this is
 * the visitor's upload link it crosses (relay/README.md, "The wire").
 */
async function packReply(id: number, out: HttpOut): Promise<Uint8Array> {
  let body: Uint8Array = new TextEncoder().encode(out.body);
  let encoding = "identity";
  if (body.length >= COMPRESS_FROM) {
    const stream = new Blob([body as BlobPart]).stream().pipeThrough(new CompressionStream("gzip"));
    body = new Uint8Array(await new Response(stream).arrayBuffer());
    encoding = "gzip";
  }
  if (body.length > FRAME_LIMIT) {
    return packReply(id, {
      status: 200,
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: requestId(out),
        error: {
          code: -32000,
          message: `the reply is ${Math.round(body.length / 1048576)} MB compressed, more than the relay carries (31 MB); ask for fewer or smaller views, or use the installed app`,
        },
      }),
    });
  }
  const head = encode({ t: "res", id, status: out.status, headers: out.headers, encoding });
  const frame = new Uint8Array(4 + head.length + body.length);
  new DataView(frame.buffer).setUint32(0, head.length, true);
  frame.set(head, 4);
  frame.set(body, 4 + head.length);
  return frame;
}

/** The JSON-RPC id a reply answers, read back out of its body. */
function requestId(out: HttpOut): unknown {
  const line = out.body.split("\n").find((l) => l.startsWith("data: {")) ?? out.body;
  try {
    return JSON.parse(line.replace(/^data: /, "")).id ?? null;
  } catch {
    return null;
  }
}

async function linkId(key: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(`parcad-link:${key}`));
  return btoa(String.fromCharCode(...new Uint8Array(digest)))
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replace(/=+$/, "")
    .slice(0, 32);
}

function setLink(link: string | null) {
  host!._host_link(...give(encode(link)));
}

async function connect() {
  const wanted = relayWanted;
  if (!wanted) return;
  report({ kind: "connecting" });
  const url = `${wanted.relay.replace(/^http/, "ws")}/page/${await linkId(wanted.key)}`;
  const opened = new WebSocket(url, ["parcad.v1", `key.${wanted.key}`]);
  socket = opened;
  opened.onmessage = async (event) => {
    if (typeof event.data !== "string" || event.data === "pong") return;
    const message = JSON.parse(event.data);
    if (message.t === "ready") {
      retryIn = 1_000;
      setLink(message.link);
      report({ kind: "ready", link: message.link });
    } else if (message.t === "superseded") {
      relayWanted = undefined;
      setLink(null);
      report({ kind: "elsewhere" });
      opened.close();
    } else if (message.t === "req") {
      const out = await mcp({ mcp: { method: message.method, headers: message.headers ?? {}, body: message.body ?? "" } });
      const reply = await packReply(message.id, out);
      if (opened.readyState === WebSocket.OPEN) opened.send(reply);
    }
  };
  opened.onopen = () => {
    if (heartbeat) clearInterval(heartbeat);
    heartbeat = setInterval(() => opened.readyState === WebSocket.OPEN && opened.send("ping"), 25_000);
  };
  opened.onclose = (event) => {
    if (socket !== opened) return;
    socket = undefined;
    if (heartbeat) clearInterval(heartbeat);
    if (!relayWanted) return;
    setLink(null);
    // The relay refuses a key it cannot use with a reason; anything else is the network.
    if (event.code === 4400 || event.code === 4403) {
      relayWanted = undefined;
      report({ kind: "failed", error: event.reason || "the relay refused this tab" });
      return;
    }
    report({ kind: "connecting" });
    setTimeout(() => void connect(), retryIn);
    retryIn = Math.min(retryIn * 2, 30_000);
  };
}

function relay(message: Extract<ToHost, { kind: "relay" }>) {
  const previous = socket;
  socket = undefined;
  previous?.close();
  if (!message.relay) {
    relayWanted = undefined;
    setLink(null);
    report({ kind: "off" });
    return;
  }
  relayWanted = { relay: message.relay, key: message.key };
  retryIn = 1_000;
  void connect();
}

// ------------------------------------------------------------------ messages

scope.onmessage = async (event: MessageEvent<ToHost>) => {
  const message = event.data;
  if (message.kind === "start") {
    ready = start(message);
    try {
      await ready;
      post({ kind: "ready" });
    } catch (e) {
      post({ kind: "failed", error: e instanceof Error ? e.message : String(e) });
    }
    return;
  }
  await ready;
  if (message.kind === "call") {
    const reply = await run(message.input).catch(
      (e): HostReply => ({ kind: "refused", status: 500, error: `the host failed inside the tab: ${e instanceof Error ? e.message : e}` }),
    );
    const transfer = reply.kind === "bytes" || reply.kind === "meshed" ? [reply.bytes.buffer] : [];
    post({ kind: "reply", id: message.id, reply }, transfer);
  } else if (message.kind === "kernel-end") {
    kernelEnds.get(message.ticket)?.(message.end);
    kernelEnds.delete(message.ticket);
  } else if (message.kind === "relay") {
    relay(message);
  }
};
