/**
 * The exact kernel in this tab: fetch and compile the WebAssembly once, run it
 * in a Web Worker, and supervise that worker the way `host.rs` supervises a
 * native one.
 *
 * Imported only by `backend.ts`. Every outcome arrives as a value, in the same
 * words the desktop host uses for the same outcome: a refusal is the kernel's
 * own message; a worker that traps is a crash naming the last breadcrumb; one
 * still running at the deadline is terminated and reported as timed out. The
 * next call starts a fresh worker from the module already compiled, so a crash
 * costs an instantiation, not another download.
 */

import type { FromWorker, ToWorker } from "./kernel-worker";

/** Where the build put the kernel, and how big it is. See `vite.config.ts`. */
declare const __PARCAD_KERNEL__: { script: string; wasm: string; bytes: number };

/**
 * Twenty seconds is the desktop's default, and the WebAssembly build measured
 * about two to three times slower than native on the corpus
 * (playground/README.md), so the same parts get the same margin.
 */
const TIMEOUT_MS = 60_000;

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
    worker.onmessage = (event: MessageEvent<FromWorker>) => {
      if (event.data.kind === "ready") resolve();
      if (event.data.kind === "died") reject(new Error(crashed("starting up", event.data.detail)));
    };
    worker.onerror = (event) => reject(new Error(crashed("starting up", event.message)));
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

/** `OcctError::Crashed`, for a tab. */
const crashed = (stage: string, detail: string) =>
  `the geometry kernel crashed while ${stage} (${detail}). ` +
  "This is usually a dimension the operation cannot satisfy — a fillet larger than the material, " +
  "a blend across a junction where several members meet or touch face-on, or a boolean between " +
  "shapes that do not overlap. The next build starts a fresh kernel.";

/** `OcctError::TimedOut`, for a tab: there is no environment variable to raise here. */
const timedOut = (stage: string) =>
  `the geometry kernel was still ${stage} after ${TIMEOUT_MS / 1000}s and was stopped. ` +
  "The WebAssembly kernel runs two to three times slower than the installed app, which gives a part 20 s " +
  "by default and lets you raise it; a part this heavy is one to build there.";

export type Reply = { json: unknown } | { bytes: Uint8Array };

export function call(request: unknown): Promise<Reply> {
  const turn = queue.then(() => send(request));
  queue = turn.catch(() => {});
  return turn;
}

async function send(request: unknown): Promise<Reply> {
  running ??= await start().catch((e: unknown) => {
    stop();
    throw e;
  });
  const current = running;
  const id = nextId++;
  current.stage = "reading the request";

  return new Promise<Reply>((resolve, reject) => {
    const deadline = window.setTimeout(() => {
      stop();
      reject(new Error(timedOut(current.stage)));
    }, TIMEOUT_MS);
    const finish = () => window.clearTimeout(deadline);

    current.worker.onmessage = (event: MessageEvent<FromWorker>) => {
      const message = event.data;
      if (message.kind === "stage") {
        current.stage = message.stage;
      } else if (message.kind === "died") {
        finish();
        stop();
        reject(new Error(crashed(current.stage, message.detail)));
      } else if (message.kind === "reply" && message.id === id) {
        finish();
        if (!message.ok) reject(new Error(message.message));
        else resolve(message.bytes ? { bytes: message.bytes } : { json: message.json });
      }
    };
    current.worker.onerror = (event) => {
      finish();
      stop();
      reject(new Error(crashed(current.stage, event.message || "the worker failed")));
    };
    current.worker.postMessage({ kind: "call", id, request } satisfies ToWorker);
  });
}
