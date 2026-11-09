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
 * Two lines, and the rest only when it is wrong. Always shown: the size, and
 * every red line — a failing check, a part that won't print or prints
 * nothing, an open shell, an envelope or budget overrun, a part standing on
 * stubs, separate or broken bodies, interfering pairs, unused nodes, missing
 * treatment curves. Everything else sits behind one chevron, closed by
 * default, whose summary says how many bodies and whether print_check found
 * anything: volume and topology, the tessellation, what it stands on, the
 * bodies and pairs, an envelope it fits, print flags, the script's notes. A
 * key holder showed seventeen lines over the part, and nobody reads furniture;
 * a green line on every part is what makes the red one easy to miss.
 *
 * The z range sits on the size line because a size alone hides where the part
 * is: a stand whose pegs were placed on the wrong face measured a plausible
 * 45 mm tall and stood 5.5 mm below its own base, and nothing on screen said so.
 * What it stands on is the other half of that lesson, and turns red when that
 * surface is under a tenth of the footprint, which is what eighteen stubs look
 * like and a slab never does.
 */

import { signal } from "@preact/signals";
import type { ComponentChildren } from "preact";
import { fmt } from "../engine";
import * as S from "../state";
import { Glass } from "./components/Glass";

type Snapshot = NonNullable<typeof S.snapshot.value>;
type Body = NonNullable<Snapshot["named_bodies"]>[number];
type Pair = NonNullable<Snapshot["between_bodies"]>[number];

const OPEN_KEY = "parcad.report.open";

/** Whether the details are listed; remembered, so a panel someone opened stays open. */
const detailsOpen = signal(remembered());

function remembered(): boolean {
  try {
    return localStorage.getItem(OPEN_KEY) === "1";
  } catch {
    return false;
  }
}

function setOpen(open: boolean) {
  detailsOpen.value = open;
  try {
    localStorage.setItem(OPEN_KEY, open ? "1" : "0");
  } catch {
    // Blocked storage: the panel still opens, it is only not remembered.
  }
}

export function Report() {
  const snapshot = S.snapshot.value;
  if (!snapshot) return null;

  const [sx, sy, sz] = snapshot.size;
  const [, , zLow] = snapshot.bounds_min;
  const [, , zHigh] = snapshot.bounds_max;
  const open = detailsOpen.value;

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
      <Wrong snapshot={snapshot} />
      <button
        type="button"
        class="pointer-events-auto cursor-pointer text-left hover:text-ink"
        onClick={() => setOpen(!open)}
      >
        {open ? "▾" : "▸"} <Summary snapshot={snapshot} />
      </button>
      {open && <Details snapshot={snapshot} />}
    </Glass>
  );
}

const brokenBody = (body: Body) => body.pieces > 1 || body.watertight === false;
const interfering = (pair: Pair) => pair.verdict === "interfering";
const onStubs = (snapshot: Snapshot) => (snapshot.stands_on?.footprint_fraction ?? 1) < 0.1;

/** Every red line: shown whether the details are open or not. */
function Wrong({ snapshot }: { snapshot: Snapshot }) {
  const dead = snapshot.unused_nodes ?? 0;
  const linked = S.linkedEdgeCount.value;
  const envelope = snapshot.brief?.envelope;
  const budget = snapshot.brief?.budget_cm3;

  return (
    <>
      {snapshot.checks?.failed?.map((f) => (
        <div class="text-bad" key={f.check}>
          check fails: <Strong>{f.check}</Strong>
          {f.measured_mm !== undefined && ` measured ${fmt(f.measured_mm)} mm`}
          {f.measured_mm3 !== undefined && ` measured ${fmt(f.measured_mm3)} mm³`}
        </div>
      ))}
      {snapshot.print_check?.failed?.map((f) => (
        <div class="text-bad" key={f.what}>
          {f.kind === "thin" ? "nothing prints" : "won't print"}: <Strong>{f.what}</Strong>
        </div>
      ))}
      {snapshot.watertight === false && (
        <div class="text-bad">NOT watertight — {snapshot.non_manifold_edges} bad edges</div>
      )}
      {envelope && !envelope.fits && (
        <div class="text-bad">
          {envelope.given.map((d) => fmt(d)).join(" × ")} envelope — over by{" "}
          <Strong>{fmt(envelope.over_mm ?? 0)}</Strong> mm along {envelope.on}
        </div>
      )}
      {budget && !budget.fits && (
        <div class="text-bad">
          <Strong>{fmt(budget.measured)}</Strong> cm³ against a {fmt(budget.given)} cm³ budget
        </div>
      )}
      {onStubs(snapshot) && (
        <div class="text-bad">
          <StandsOn snapshot={snapshot} />
        </div>
      )}
      {snapshot.bodies > (snapshot.named_bodies?.length ?? 1) && (
        <div class="text-bad">
          <Strong>{snapshot.bodies}</Strong> separate bodies
          {snapshot.named_bodies && ` for ${snapshot.named_bodies.length} named`}
        </div>
      )}
      {snapshot.named_bodies?.filter(brokenBody).map((body) => (
        <BodyLine key={body.name} body={body} />
      ))}
      {snapshot.between_bodies?.filter(interfering).map((pair) => (
        <PairLine key={`${pair.a}/${pair.b}`} pair={pair} />
      ))}
      {!linked && S.lastTreatments.value.length > 0 && (
        <div class="text-bad">no final treatment curves are available</div>
      )}
      {dead > 0 && <div class="text-bad">{dead} unused nodes</div>}
    </>
  );
}

/** What each kind of print finding is called on the chevron's line. */
const FINDING_NAMES: Record<string, string> = {
  thin: "thin wall",
  overhang: "overhang",
  pieces: "loose pieces",
  floating: "off the bed",
  collision: "cut into a feature",
};

/** The flagged findings by kind, so the line says what to read without opening it. */
function flaggedKinds(findings: { kind: string }[]): string {
  const counts = new Map<string, number>();
  for (const f of findings) counts.set(f.kind, (counts.get(f.kind) ?? 0) + 1);
  return [...counts]
    .map(([kind, n]) => `${FINDING_NAMES[kind] ?? kind}${n > 1 ? ` ×${n}` : ""}`)
    .join(", ");
}

/** The chevron's line: how many bodies, and whether print_check found anything. */
function Summary({ snapshot }: { snapshot: Snapshot }) {
  const bodies = snapshot.named_bodies?.length ?? 0;
  const print = snapshot.print_check;
  const failed = print?.failed?.length ?? 0;
  const flagged = print?.flagged?.length ?? 0;
  return (
    <>
      {bodies > 1 ? (
        <>
          <Strong>{bodies}</Strong> bodies
        </>
      ) : (
        "details"
      )}
      {print &&
        (failed > 0 ? (
          <span class="text-bad"> · print fails</span>
        ) : flagged > 0 ? (
          <>
            {" · prints, flagged: "}
            <Strong>{flaggedKinds(print!.flagged ?? [])}</Strong>
          </>
        ) : (
          " · prints clean"
        ))}
    </>
  );
}

/** Behind the chevron: what is measured and right. */
function Details({ snapshot }: { snapshot: Snapshot }) {
  return (
    <>
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
            <Strong>{snapshot.faces}</Strong> faces · <Strong>{snapshot.topological_edges}</Strong> edges
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
      {snapshot.stands_on && !onStubs(snapshot) && (
        <div>
          <StandsOn snapshot={snapshot} />
        </div>
      )}
      {snapshot.named_bodies?.filter((body) => !brokenBody(body)).map((body) => (
        <BodyLine key={body.name} body={body} />
      ))}
      {snapshot.between_bodies?.filter((pair) => !interfering(pair)).map((pair) => (
        <PairLine key={`${pair.a}/${pair.b}`} pair={pair} />
      ))}
      {snapshot.brief?.envelope?.fits && (
        <div>{snapshot.brief.envelope.given.map((d) => fmt(d)).join(" × ")} envelope — fits</div>
      )}
      {snapshot.print_check?.flagged?.map((f) => (
        <div key={f.what}>print: {f.what}</div>
      ))}
      {snapshot.notes && (
        <div>
          {snapshot.notes.source}:{" "}
          {snapshot.notes.values.map((n) => `${n.label} ${noteValue(n.value)}`).join(" · ")}
          {snapshot.notes.dropped ? ` · ${snapshot.notes.dropped} more dropped` : ""}
        </div>
      )}
    </>
  );
}

function noteValue(value: number | string | number[]): string {
  if (typeof value === "number") return fmt(value);
  return Array.isArray(value) ? value.map(fmt).join(", ") : value;
}

function StandsOn({ snapshot }: { snapshot: Snapshot }) {
  const stands = snapshot.stands_on!;
  return (
    <>
      stands on <Strong>{fmt(stands.area_mm2)}</Strong> mm² in <Strong>{stands.patches}</Strong>{" "}
      {stands.patches === 1 ? "patch" : "patches"}
    </>
  );
}

function BodyLine({ body }: { body: Body }) {
  return (
    <div class={brokenBody(body) ? "text-bad" : undefined}>
      {body.name}
      {body.reference && " (reference)"}:{" "}
      {body.volume_mm3 !== undefined ? (
        <>
          <Strong>{fmt(body.volume_mm3)}</Strong> mm³
        </>
      ) : (
        "a surface"
      )}
      {body.pieces > 1 && ` in ${body.pieces} pieces`}
    </div>
  );
}

function PairLine({ pair }: { pair: Pair }) {
  return (
    <div class={interfering(pair) ? "text-bad" : undefined}>
      {pair.a} · {pair.b}: {pair.verdict}
      {pair.verdict !== "touching" && pair.clearance_mm !== undefined && (
        <>
          {" by "}
          <Strong>{fmt(pair.clearance_mm)}</Strong> mm
        </>
      )}
      {interfering(pair) && (
        <>
          {", "}
          <Strong>{fmt(pair.interference_mm3)}</Strong> mm³ shared
          {pair.depth_mm !== undefined && (
            <>
              {", "}
              <Strong>{fmt(pair.depth_mm)}</Strong> mm deep
            </>
          )}
        </>
      )}
      {pair.verdict === "touching" && pair.contact_mm2 !== undefined && (
        <>
          {" over "}
          <Strong>{fmt(pair.contact_mm2)}</Strong> mm²
          {` in ${pair.contact_patches ?? 0} ${pair.contact_patches === 1 ? "patch" : "patches"}`}
        </>
      )}
    </div>
  );
}

/** A measured number, picked out of the sentence around it. */
const Strong = ({ children }: { children: ComponentChildren }) => (
  <b class="text-ink font-medium">{children}</b>
);
