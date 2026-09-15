/**
 * What is downloading, and how big it is, while the kernel is on its way.
 *
 * Only a page that carries its own kernel ever shows this: the desktop window
 * and a browser on the host have one before anything draws, so the signal
 * stays undefined and this renders nothing.
 */

import { fmt } from "../engine";
import * as S from "../state";
import { Glass } from "./components/Glass";

const mb = (bytes: number) => fmt(bytes / 1_000_000);

export function KernelLoading() {
  const load = S.kernelLoad.value;
  if (!load) return null;

  const text =
    load.phase === "failed"
      ? `The geometry kernel could not start: ${load.error ?? "unknown error"}`
      : load.phase === "compiling"
        ? `Compiling the geometry kernel — ${mb(load.total)} MB of WebAssembly, OpenCASCADE and parcad's own measurements`
        : `Downloading the geometry kernel — ${mb(load.received)} of ${mb(load.total)} MB of WebAssembly. It runs in this tab; nothing is sent anywhere.`;

  return (
    <Glass
      variant="hud"
      id="kernel-loading"
      layout="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-[min(28rem,80%)] px-4 py-3
              text-ink-dim text-small leading-relaxed pointer-events-none"
    >
      <div>{text}</div>
      {load.phase === "downloading" && (
        <div class="mt-2 h-1 rounded-full bg-line overflow-hidden">
          <div class="h-full bg-accent" style={{ width: `${Math.min(100, (100 * load.received) / load.total)}%` }} />
        </div>
      )}
    </Glass>
  );
}
