/**
 * The editor's half of the language service.
 *
 * Started on the first thing that needs it rather than with the window: a
 * visitor who opens ParCAD web to look at a part and never types should not
 * fetch a compiler, and the desktop app should not spend its first second on
 * one either. Until then every answer is "nothing", which is exactly what the
 * editor showed before this existed.
 *
 * If the worker cannot start, it stays not-started and the editor keeps
 * working. IntelliSense is an addition to the editor, never a dependency of it.
 */

import type { Replies, Reply, Request } from "./protocol";

type Kind = Request["kind"];

let worker: Worker | undefined;
let failed = false;
let nextId = 1;
const pending = new Map<number, { resolve: (value: unknown) => void; reject: (e: Error) => void }>();

/** The document as the worker last heard it, so a late start is not a stale one. */
let latest = "";

/** Whether the service has anything to say yet, for the editor's own status. */
export function started(): boolean {
  return !!worker;
}

function ensure(): Worker | undefined {
  if (worker || failed) return worker;
  try {
    worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
    worker.onmessage = (event: MessageEvent<Reply>) => {
      const waiting = pending.get(event.data.id);
      if (!waiting) return;
      pending.delete(event.data.id);
      if ("error" in event.data) waiting.reject(new Error(event.data.error));
      else waiting.resolve(event.data.value);
    };
    worker.onerror = () => {
      failed = true;
      for (const waiting of pending.values()) waiting.reject(new Error("the language service stopped"));
      pending.clear();
      worker?.terminate();
      worker = undefined;
    };
    worker.postMessage({ id: 0, kind: "part", text: latest } satisfies Request);
  } catch {
    failed = true;
  }
  return worker;
}

/**
 * Warm the service before it is asked anything.
 *
 * Called once the window has settled, so the first hover is not also the first
 * few hundred milliseconds of compiling TypeScript's libraries.
 */
export function prewarm() {
  ensure();
}

/** Tell the service what the document says now. */
export function setPart(text: string) {
  latest = text;
  worker?.postMessage({ id: 0, kind: "part", text } satisfies Request);
}

/**
 * Ask one question.
 *
 * `undefined` whenever the service is not running, which every caller already
 * has to handle for the answer "there is nothing here".
 */
function ask<K extends Exclude<Kind, "part">>(
  kind: K,
  extra: Omit<Extract<Request, { kind: K }>, "id" | "kind">,
): Promise<Replies[K] | undefined> {
  const live = ensure();
  if (!live) return Promise.resolve(undefined);
  const id = nextId++;
  return new Promise<Replies[K]>((resolve, reject) => {
    pending.set(id, { resolve: resolve as (value: unknown) => void, reject });
    live.postMessage({ ...extra, kind, id } as Request);
  });
}

export const quickInfo = (pos: number) => ask("quickInfo", { pos });
export const completions = (pos: number) => ask("completions", { pos });
export const completionDetail = (pos: number, label: string) =>
  ask("completionDetail", { pos, label });
export const signatureHelp = (pos: number) => ask("signatureHelp", { pos });
export const diagnostics = () => ask("diagnostics", {});
