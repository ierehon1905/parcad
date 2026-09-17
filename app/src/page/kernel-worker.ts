/**
 * The Web Worker the WebAssembly kernel runs in — the tab's counterpart of the
 * `parcad-occt-worker` process, and expendable for the same reason.
 *
 * It instantiates the module the page already compiled, answers one request
 * packet at a time with a reply packet (`parcad_occt::packet`), and forwards
 * every `@stage` breadcrumb the kernel prints, so that when the page has to
 * terminate it the last one says what it was doing.
 */

/// <reference lib="webworker" />

interface KernelModule {
  HEAPU8: Uint8Array;
  HEAPU32: Uint32Array;
  _parcad_alloc(len: number): number;
  _parcad_call(ptr: number, len: number): number;
  _parcad_free(ptr: number): void;
}

type Factory = (options: Record<string, unknown>) => Promise<KernelModule>;

export type ToWorker =
  | { kind: "start"; module: WebAssembly.Module; script: string }
  | { kind: "call"; id: number; packet: Uint8Array };

export type FromWorker =
  | { kind: "ready" }
  | { kind: "stage"; stage: string }
  | { kind: "reply"; id: number; packet: Uint8Array }
  | { kind: "died"; detail: string };

const post = (message: FromWorker, transfer: Transferable[] = []) =>
  (self as unknown as DedicatedWorkerGlobalScope).postMessage(message, transfer);

const BREADCRUMB = "@stage ";
/** `parcad_call`'s reply kinds, from crates/parcad-wasm/src/kernel.rs. */
const PACKET = 1;
let kernel: KernelModule | undefined;

self.onmessage = async (event: MessageEvent<ToWorker>) => {
  const message = event.data;
  if (message.kind === "start") {
    try {
      const factory = ((await import(/* @vite-ignore */ message.script)) as { default: Factory }).default;
      kernel = await factory({
        instantiateWasm(imports: WebAssembly.Imports, success: (instance: WebAssembly.Instance, module: WebAssembly.Module) => void) {
          void WebAssembly.instantiate(message.module, imports).then((instance) => success(instance, message.module));
          return {};
        },
        print: () => {},
        printErr(line: string) {
          if (line.startsWith(BREADCRUMB)) post({ kind: "stage", stage: line.slice(BREADCRUMB.length) });
          else console.warn(line);
        },
      });
      post({ kind: "ready" });
    } catch (e) {
      post({ kind: "died", detail: String(e) });
    }
    return;
  }

  const k = kernel!;
  try {
    const ptr = k._parcad_alloc(message.packet.length);
    k.HEAPU8.set(message.packet, ptr);
    const reply = k._parcad_call(ptr, message.packet.length);
    // Views are re-read after the call: memory may have grown during it.
    // The kernel's last breadcrumb is "encoding the reply"; a stop after it is in here.
    post({ kind: "stage", stage: "handing the reply to the page" });
    const kind = k.HEAPU32[reply >>> 2];
    const len = k.HEAPU32[(reply >>> 2) + 1];
    const payload = k.HEAPU8.slice(reply + 8, reply + 8 + len);
    k._parcad_free(reply);
    if (kind === PACKET) post({ kind: "reply", id: message.id, packet: payload }, [payload.buffer]);
    // The kernel could not read what it was sent: a host bug, and no state to trust after it.
    else post({ kind: "died", detail: new TextDecoder().decode(payload) });
  } catch (e) {
    // A trap or an abort: the module's state is not to be trusted after one,
    // exactly as a native worker is not after a signal.
    post({ kind: "died", detail: e instanceof Error ? e.message : String(e) });
  }
};
