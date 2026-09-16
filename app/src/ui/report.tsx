/**
 * What the kernel measured, over the part it measured.
 *
 * Every value here comes out of the `EvaluationSnapshot` the host built. This
 * component selects and labels; it never derives. The one thing it does decide
 * is wording: "within 0.010 mm" is a deflection, a bound on how far any
 * triangle sits from the true surface, and it is said that way so it is not
 * read as a grid spacing.
 *
 * Hidden when there is nothing measured, rather than standing empty — the
 * border and backdrop are visible with no text in them, and an empty panel over
 * an empty viewport reads as a part that failed to draw. No placeholder either:
 * the error beside it already says why there is no part.
 *
 * Three lines, and the rest only when it is wrong. A green "watertight" on every
 * part is furniture you stop reading, which is what makes the red one easy to
 * miss on the part where it appears; same for the source links. Area and the tag
 * list went because the script beside the viewport already says both.
 *
 * The z range sits on the size line because a size alone hides where the part
 * is: a stand whose pegs were placed on the wrong face measured a plausible
 * 45 mm tall and stood 5.5 mm below its own base, and nothing on screen said so.
 * The fourth line is the other half of that lesson — what the part stands on —
 * and turns red when that surface is under a tenth of the footprint, which is
 * what eighteen stubs look like and a slab never does.
 */

import type { ComponentChildren } from "preact";
import { fmt } from "../engine";
import * as S from "../state";
import { Glass } from "./components/Glass";

export function Report() {
  const snapshot = S.snapshot.value;
  if (!snapshot) return null;

  const [sx, sy, sz] = snapshot.size;
  const [, , zLow] = snapshot.bounds_min;
  const [, , zHigh] = snapshot.bounds_max;
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
        mm · z <Strong>{fmt(zLow)}</Strong> to <Strong>{fmt(zHigh)}</Strong>
      </div>
      <div>
        {snapshot.volume_mm3 !== undefined ? (
          <>
            <Strong>{fmt(snapshot.volume_mm3)}</Strong> mm³
          </>
        ) : (
          <>
            surface of <Strong>{fmt(snapshot.area_mm2)}</Strong> mm²
          </>
        )}
        {snapshot.faces !== undefined && (
          <>
            {" · "}
            <Strong>{snapshot.faces}</Strong> faces · <Strong>{snapshot.topological_edges}</Strong>{" "}
            edges
          </>
        )}
      </div>
      <div>
        <Strong>{snapshot.triangles.toLocaleString()}</Strong> tris within{" "}
        <Strong>{snapshot.resolution_mm.toFixed(3)}</Strong> mm
      </div>
      {snapshot.surface && (
        <div>
          {snapshot.surface.open ? (
            <>
              open along <Strong>{fmt(snapshot.surface.free_edge_length_mm)}</Strong> mm in{" "}
              <Strong>{snapshot.surface.boundary_loops}</Strong>{" "}
              {snapshot.surface.boundary_loops === 1 ? "loop" : "loops"}
            </>
          ) : (
            "closed, with no inside yet"
          )}
          {" · "}thicken to print
        </div>
      )}
      {snapshot.thickened_mm && (
        <div>
          thickened <Strong>{fmt(snapshot.thickened_mm.min)}</Strong>
          {snapshot.thickened_mm.max !== snapshot.thickened_mm.min && (
            <>
              {" to "}
              <Strong>{fmt(snapshot.thickened_mm.max)}</Strong>
            </>
          )}{" "}
          mm, measured
        </div>
      )}
      {snapshot.stands_on && (
        <div class={snapshot.stands_on.footprint_fraction < 0.1 ? "text-bad" : undefined}>
          stands on <Strong>{fmt(snapshot.stands_on.area_mm2)}</Strong> mm² in{" "}
          <Strong>{snapshot.stands_on.patches}</Strong>{" "}
          {snapshot.stands_on.patches === 1 ? "patch" : "patches"}
        </div>
      )}
      {snapshot.bodies > (snapshot.named_bodies?.length ?? 1) && (
        <div class="text-bad">
          <Strong>{snapshot.bodies}</Strong> separate bodies
          {snapshot.named_bodies && ` for ${snapshot.named_bodies.length} named`}
        </div>
      )}
      {snapshot.named_bodies?.map((body) => (
        <div key={body.name} class={body.pieces > 1 || body.watertight === false ? "text-bad" : undefined}>
          {body.name}:{" "}
          {body.volume_mm3 !== undefined ? (
            <>
              <Strong>{fmt(body.volume_mm3)}</Strong> mm³
            </>
          ) : (
            "a surface"
          )}
          {body.pieces > 1 && ` in ${body.pieces} pieces`}
        </div>
      ))}
      {snapshot.between_bodies?.map((pair) => (
        <div key={`${pair.a}/${pair.b}`} class={pair.verdict === "interfering" ? "text-bad" : undefined}>
          {pair.a} · {pair.b}: {pair.verdict}
          {pair.clearance_mm !== undefined && (
            <>
              {" by "}
              <Strong>{fmt(pair.clearance_mm)}</Strong> mm
            </>
          )}
          {pair.verdict === "interfering" && (
            <>
              {", "}
              <Strong>{fmt(pair.interference_mm3)}</Strong> mm³ shared
            </>
          )}
        </div>
      ))}
      {snapshot.watertight === false && (
        <div class="text-bad">
          NOT watertight — {snapshot.non_manifold_edges} bad edges
        </div>
      )}
      {!linked && S.lastTreatments.value.length > 0 && (
        <div class="text-bad">no final treatment curves are available</div>
      )}
      {dead > 0 && <div class="text-bad">{dead} unused nodes</div>}
    </Glass>
  );
}

/** A measured number, picked out of the sentence around it. */
const Strong = ({ children }: { children: ComponentChildren }) => (
  <b class="text-ink font-medium">{children}</b>
);
