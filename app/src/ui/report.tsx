/**
 * What the kernel measured, over the part it measured.
 *
 * Every value here comes out of the `EvaluationSnapshot` the host built. This
 * component selects and labels; it never derives. The one thing it does decide
 * is wording, and one distinction there is load-bearing: "resolution" means
 * different things per backend, and the difference matters. A grid spacing is
 * where samples were *taken*; a deflection is a bound on how far the result can
 * be from the truth. They are labelled apart for that reason.
 *
 * Hidden when there is nothing measured, rather than standing empty — the
 * border and backdrop are visible with no text in them, and an empty panel over
 * an empty viewport reads as a part that failed to draw. No placeholder either:
 * the error beside it already says why there is no part.
 */

import type { ComponentChildren } from "preact";
import { fmt } from "../engine";
import * as S from "../state";
import { Glass } from "./components/Glass";

export function Report() {
  const snapshot = S.snapshot.value;
  if (!snapshot) return null;

  const [sx, sy, sz] = snapshot.size;
  const dead = snapshot.unused_nodes ?? 0;
  const linked = S.linkedEdgeCount.value;

  return (
    <Glass
      variant="hud"
      layout="absolute left-3.5 bottom-3.5 max-w-[60%] px-3 py-2.5
              text-ink-dim font-mono text-tiny leading-[1.7] pointer-events-none"
    >
      <div>
        <Strong>
          {fmt(sx)} × {fmt(sy)} × {fmt(sz)}
        </Strong>{" "}
        mm
      </div>
      <div>
        volume <Strong>{fmt(snapshot.volume_mm3)}</Strong> mm³ · area{" "}
        <Strong>{fmt(snapshot.area_mm2)}</Strong> mm²
      </div>
      {snapshot.faces !== undefined && (
        <div>
          topology <Strong>{snapshot.faces}</Strong> faces ·{" "}
          <Strong>{snapshot.topological_edges}</Strong> edges
        </div>
      )}
      <div>
        mesh <Strong>{snapshot.triangles.toLocaleString()}</Strong> tris{" "}
        {snapshot.backend === "brep" ? "within " : "at "}
        <Strong>{snapshot.resolution_mm.toFixed(3)}</Strong>
        {snapshot.backend === "brep" ? " mm of the true surface " : " mm grid "}
        {snapshot.watertight ? (
          <span class="text-good">watertight</span>
        ) : (
          <span class="text-bad">
            NOT watertight — {snapshot.non_manifold_edges} bad edges
          </span>
        )}
      </div>
      <div>{snapshot.tags.length ? `tags ${snapshot.tags.join(", ")}` : "no tags"}</div>
      <div>
        {linked ? (
          <span class="text-good">
            source links <Strong>{linked}</Strong> final curves →{" "}
            {S.linkedMethods.value.join(", ")}
          </span>
        ) : S.lastTreatments.value.length ? (
          <span class="text-bad">source links: no final treatment curves are available</span>
        ) : (
          "source links — no edge treatments"
        )}
      </div>
      {dead > 0 && <div class="text-bad">{dead} unused nodes</div>}
    </Glass>
  );
}

/** A measured number, picked out of the sentence around it. */
const Strong = ({ children }: { children: ComponentChildren }) => (
  <b class="text-ink font-medium">{children}</b>
);
