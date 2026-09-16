/**
 * What the viewport is waiting for while it has nothing to draw yet.
 *
 * A page that carries its own kernel first downloads and compiles it; every
 * transport then builds the first part before there is geometry. Both waits
 * used to leave the viewport blank — the playground's first part takes seconds
 * in a tab — so both say what is happening. Once a part has been drawn a
 * rebuild keeps the last one on screen, and this renders nothing.
 */

import { fmt } from "../engine";
import * as S from "../state";
import { Glass } from "./components/Glass";

const mb = (bytes: number) => fmt(bytes / 1_000_000);

export function KernelLoading() {
  const load = S.kernelLoad.value;
  const building = !load && !S.snapshot.value && S.status.value.tone === "busy";
  if (!load && !building) return null;

  const text = load
    ? load.phase === "failed"
      ? `The geometry kernel could not start: ${load.error ?? "unknown error"}`
      : load.phase === "compiling"
        ? `Compiling the geometry kernel — ${mb(load.total)} MB of WebAssembly, OpenCASCADE and parcad's own measurements`
        : `Downloading the geometry kernel — ${mb(load.received)} of ${mb(load.total)} MB of WebAssembly. It runs in this tab; nothing is sent anywhere.`
    : `Building ${S.openPath.value ?? "the part"} on the exact kernel — every face measured, not previewed`;

  const progress = load?.phase === "downloading" ? Math.min(100, (100 * load.received) / load.total) : undefined;
  const waiting = load?.phase === "compiling" || building;

  return (
    <Glass
      variant="hud"
      id="kernel-loading"
      layout="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-[min(28rem,80%)] px-4 py-3
              text-ink-dim text-small leading-relaxed pointer-events-none"
    >
      <div>{text}</div>
      {(progress !== undefined || waiting) && (
        <div class="mt-2 h-1 rounded-full bg-line overflow-hidden">
          {progress !== undefined ? (
            <div class="h-full bg-accent" style={{ width: `${progress}%` }} />
          ) : (
            <div class="h-full w-1/3 bg-accent rounded-full animate-slide-across motion-reduce:animate-none" />
          )}
        </div>
      )}
    </Glass>
  );
}
