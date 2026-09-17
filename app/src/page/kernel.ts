/**
 * The exact kernel in this tab: fetch and compile the WebAssembly once, run it
 * in a Web Worker, and supervise that worker the way `host.rs` supervises a
 * native one.
 *
 * Its only client is the host (`host.ts`), which hands it request packets.
 * Every outcome is a value: a reply packet, a crash with the last breadcrumb,
 * a stop at the deadline, or no kernel at all — and the host words each one
 * the way the desktop does. The next request starts a fresh worker from the
 * module already compiled, so a crash costs an instantiation, not another
 * download.
 */

import type { FromWorker, ToWorker } from "./kernel-worker";

/** Where the build put the kernel, and how big it is. See `vite.config.ts`. */
declare const __PARCAD_KERNEL__: { script: string; wasm: string; bytes: number };

export interface KernelLoad {
  /** `downloading`, `compiling`, or `ready`. */
  phase: "downloading" | "compiling" | "ready" | "failed";
  received: number;
  total: number;
  error?: string;
}

const listeners = new Set<(load: KernelLoad) => void>();
let load: KernelLoad = { phase: "downloading", received: 0, total: __PARCAD_KERNEL__.bytes };

function report(next: KernelLoad) {
  load = next;
  for (const listener of listeners) listener(load);
}

export function watchLoad(listener: (load: KernelLoad) => void): () => void {
  listeners.add(listener);
  listener(load);
  return () => listeners.delete(listener);
}

let compiled: Promise<WebAssembly.Module> | undefined;

/** The module, downloaded with progress and compiled once for every worker. */
function compile(): Promise<WebAssembly.Module> {
  compiled ??= (async () => {
    const url = new URL(__PARCAD_KERNEL__.wasm, document.baseURI).href;
    const response = await fetch(url);
    if (!response.ok || !response.body) {
      throw new Error(`the geometry kernel did not download (${response.status} from ${url}); reload the page to try again`);
    }
    const total = __PARCAD_KERNEL__.bytes;
    const reader = response.body.getReader();
    const chunks: Uint8Array[] = [];
    let received = 0;
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      chunks.push(value);
      received += value.length;
      report({ phase: "downloading", received, total });
    }
    report({ phase: "compiling", received, total });
    const bytes = new Uint8Array(received);
    let at = 0;
    for (const chunk of chunks) {
      bytes.set(chunk, at);
      at += chunk.length;
    }
    const module = await WebAssembly.compile(bytes);
    report({ phase: "ready", received, total });
    return module;
  })().catch((e: unknown) => {
    compiled = undefined;
    const error = e instanceof Error ? e.message : String(e);
    report({ ...load, phase: "failed", error });
    throw new Error(
      `${error}\nThis browser could not start the WebAssembly kernel. It needs WebAssembly exception handling: Chrome 95, Firefox 100 or Safari 15.2, or newer.`,
    );
  });
  return compiled;
}

/** Start downloading now, rather than on the first evaluation. */
export function preload() {
  void compile().catch(() => {});
}

interface Running {
  worker: Worker;
  ready: Promise<void>;
  stage: string;
}

let running: Running | undefined;
let nextId = 0;
/** One call at a time, as a native worker serves one frame at a time. */
let queue: Promise<unknown> = Promise.resolve();

async function start(): Promise<Running> {
  const module = await compile();
  const worker = new Worker(new URL("./kernel-worker.ts", import.meta.url), { type: "module" });
  const started: Running = { worker, stage: "starting up", ready: Promise.resolve() };
  started.ready = new Promise<void>((resolve, reject) => {
    const failed = (detail: string) => reject(new Error(`the geometry kernel did not start in this tab (${detail}); reload the page`));
    worker.onmessage = (event: MessageEvent<FromWorker>) => {
      if (event.data.kind === "ready") resolve();
      if (event.data.kind === "died") failed(event.data.detail);
    };
    worker.onerror = (event) => failed(event.message);
  });
  const script = new URL(__PARCAD_KERNEL__.script, document.baseURI).href;
  worker.postMessage({ kind: "start", module, script } satisfies ToWorker);
  await started.ready;
  return started;
}

function stop() {
  running?.worker.terminate();
  running = undefined;
}

/** How one request ended, for the host to put into words. */
export type KernelEnd =
  | { kind: "replied"; packet: Uint8Array; ms: number }
  | { kind: "crashed"; stage: string; detail: string }
  | { kind: "timed_out"; stage: string; seconds: number }
  | { kind: "unavailable"; message: string };

export function run(packet: Uint8Array, timeoutMs: number): Promise<KernelEnd> {
  const turn = queue.then(() => send(packet, timeoutMs));
  queue = turn.catch(() => {});
  return turn;
}

async function send(packet: Uint8Array, timeoutMs: number): Promise<KernelEnd> {
  try {
    running ??= await start();
  } catch (e) {
    stop();
    return { kind: "unavailable", message: e instanceof Error ? e.message : String(e) };
  }
  const current = running;
  const id = nextId++;
  current.stage = "reading the request";
  const began = performance.now();

  return new Promise<KernelEnd>((resolve) => {
    const deadline = window.setTimeout(() => {
      stop();
      resolve({ kind: "timed_out", stage: current.stage, seconds: Math.round(timeoutMs / 1000) });
    }, timeoutMs);
    const end = (outcome: KernelEnd) => {
      window.clearTimeout(deadline);
      if (outcome.kind !== "replied") stop();
      resolve(outcome);
    };

    current.worker.onmessage = (event: MessageEvent<FromWorker>) => {
      const message = event.data;
      if (message.kind === "stage") current.stage = message.stage;
      else if (message.kind === "died") end({ kind: "crashed", stage: current.stage, detail: message.detail });
      else if (message.kind === "reply" && message.id === id) {
        end({ kind: "replied", packet: message.packet, ms: Math.round(performance.now() - began) });
      }
    };
    current.worker.onerror = (event) =>
      end({ kind: "crashed", stage: current.stage, detail: event.message || "the worker failed" });
    current.worker.postMessage({ kind: "call", id, packet } satisfies ToWorker, [packet.buffer]);
  });
}
