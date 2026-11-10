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
 * default. A key holder showed seventeen lines over the part, and nobody reads
 * furniture; a green line on every part is what makes the red one easy to miss.
 *
 * The chevron's line is an indicator, not a count: it names what to read and
 * in which body ("overhang: shelf1, shelf2"), because "prints, 1 flagged" made
 * the reader open the panel to learn whether it mattered. A plant shelf of 20
 * bodies listed 190 pairs here, "wall clear of pot by 1100 mm" among them, so
 * a pair of two references is never shown, clear pairs collapse to the three
 * tightest, and references to one line of names. The mesh's own numbers come
 * last: they describe how the part is drawn, not the part.
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
import type { PrintFinding } from "../state";
import { Glass } from "./components/Glass";

type Snapshot = NonNullable<typeof S.snapshot.value>;
type Body = NonNullable<Snapshot["named_bodies"]>[number];
type Pair = NonNullable<Snapshot["between_bodies"]>[number];

const OPEN_KEY = "parcad.report.open";

/** Interfering pairs listed above the chevron before the rest are counted. */
const INTERFERING_LISTED = 4;
/** Clear pairs listed behind it, tightest first. */
const CLOSEST_LISTED = 3;
/** Pairs, bodies or findings listed on one line before the rest are counted. */
const NAMES_LISTED = 6;

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

const interfering = (pair: Pair) => pair.verdict === "interfering";
const onStubs = (snapshot: Snapshot) => (snapshot.stands_on?.footprint_fraction ?? 1) < 0.1;

/** The bodies a part is measured against rather than made of. */
function referenceNames(snapshot: Snapshot): Set<string> {
  return new Set(snapshot.named_bodies?.filter((body) => body.reference).map((body) => body.name));
}

/** Every pair worth a line: two references say nothing about the part. */
function pairsOfThePart(snapshot: Snapshot): Pair[] {
  const references = referenceNames(snapshot);
  return (snapshot.between_bodies ?? []).filter((pair) => !(references.has(pair.a) && references.has(pair.b)));
}

/** Every red line: shown whether the details are open or not. */
function Wrong({ snapshot }: { snapshot: Snapshot }) {
  const dead = snapshot.unused_nodes ?? 0;
  const linked = S.linkedEdgeCount.value;
  const envelope = snapshot.brief?.envelope;
  const budget = snapshot.brief?.budget_cm3;
  const clashes = pairsOfThePart(snapshot).filter(interfering);

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
      {!snapshot.named_bodies && snapshot.bodies > 1 && (
        <div class="text-bad">
          <Strong>{snapshot.bodies}</Strong> separate bodies
        </div>
      )}
      {snapshot.named_bodies
        ?.filter((body) => !body.reference && body.watertight === false)
        .map((body) => (
          <div class="text-bad" key={body.name}>
            {body.name}: not watertight
          </div>
        ))}
      {clashes.slice(0, INTERFERING_LISTED).map((pair) => (
        <PairLine key={`${pair.a}/${pair.b}`} pair={pair} />
      ))}
      {clashes.length > INTERFERING_LISTED && (
        <div class="text-bad">
          and {clashes.length - INTERFERING_LISTED} more {clashes.length - INTERFERING_LISTED === 1 ? "pair interferes" : "pairs interfere"}
        </div>
      )}
      {!linked && S.lastTreatments.value.length > 0 && (
        <div class="text-bad">no final treatment curves are available</div>
      )}
      {dead > 0 && <div class="text-bad">{dead} unused nodes</div>}
    </>
  );
}

/** What each kind of print finding is called, on the chevron's line and behind it. */
const FINDING_NAMES: Record<PrintFinding["kind"], string> = {
  collision: "cut into a feature",
  thin: "thin wall",
  floating: "above the bed",
  pieces: "in pieces",
  overhang: "overhang",
};

/** The flagged findings by kind, each with the bodies it is in, so the line says what to read and where. */
function findingsByKind(findings: PrintFinding[], named: boolean): string {
  const kinds = Object.keys(FINDING_NAMES) as PrintFinding["kind"][];
  return kinds
    .map((kind) => {
      const of = findings.filter((f) => f.kind === kind);
      if (of.length === 0) return "";
      const bodies = named ? [...new Set(of.map((f) => f.body).filter((b): b is string => b !== undefined))] : [];
      return bodies.length > 0 ? `${FINDING_NAMES[kind]}: ${names(bodies)}` : `${FINDING_NAMES[kind]}${of.length > 1 ? ` ×${of.length}` : ""}`;
    })
    .filter(Boolean)
    .join(" · ");
}

/** A list of names, the first few written out and the rest counted. */
function names(list: string[]): string {
  const shown = list.slice(0, NAMES_LISTED).join(", ");
  return list.length > NAMES_LISTED ? `${shown} +${list.length - NAMES_LISTED}` : shown;
}

/** The chevron's line: how many bodies make the part, whether its checks hold, and what the print check wants read. */
function Summary({ snapshot }: { snapshot: Snapshot }) {
  const made = snapshot.named_bodies?.filter((body) => !body.reference).length ?? 0;
  const print = snapshot.print_check;
  const failed = print?.failed?.length ?? 0;
  const flagged = print?.flagged ?? [];
  const checks = snapshot.checks;
  return (
    <>
      {made > 1 ? (
        <>
          <Strong>{made}</Strong> bodies
        </>
      ) : (
        "details"
      )}
      {checks?.verdict === "passed" && (
        <>
          {" · "}
          <Strong>{checks.passed}</Strong> {checks.passed === 1 ? "check holds" : "checks hold"}
        </>
      )}
      {print &&
        (failed > 0 ? (
          <span class="text-bad"> · print fails</span>
        ) : flagged.length > 0 ? (
          <span class="text-note"> · {findingsByKind(flagged, snapshot.named_bodies !== undefined)}</span>
        ) : (
          " · prints clean"
        ))}
    </>
  );
}

/** One flagged finding in a line: its kind, where, and its own number. */
function FindingLine({ finding: f, pieces, named }: { finding: PrintFinding; pieces?: number; named: boolean }) {
  const where = named && f.body ? ` in ${f.body}` : "";
  switch (f.kind) {
    case "thin":
      return (
        <div class="text-note">
          {f.thin_kind === "feather" ? "feather" : "thin wall"} <Strong>{fmt(f.thickness_mm ?? 0)}</Strong> mm
          {f.between && ` between ${f.between[0]} and ${f.between[1]}`}
          {where}
        </div>
      );
    case "collision":
      return (
        <div class="text-note">
          {f.between ? `${f.between[0]} cuts into ${f.between[1]}` : "a cut into a feature"}
          {f.removed_mm3 !== undefined && (
            <>
              , <Strong>{fmt(f.removed_mm3)}</Strong> mm³ taken
            </>
          )}
          {where}
        </div>
      );
    case "overhang":
      return (
        <div class="text-note">
          overhang{where}
          {f.unsupported_mm2 !== undefined && (
            <>
              : <Strong>{fmt(f.unsupported_mm2)}</Strong> mm² faces down
            </>
          )}
        </div>
      );
    case "pieces":
      return (
        <div class="text-note">
          {(named && f.body) || "the part"} prints in <Strong>{pieces ?? f.pieces?.length ?? 0}</Strong> pieces
        </div>
      );
    case "floating":
      return <div class="text-note">a piece starts above the bed{where}</div>;
  }
}

/** Behind the chevron: what to read first, then what is measured and right, then how it is drawn. */
function Details({ snapshot }: { snapshot: Snapshot }) {
  const references = referenceNames(snapshot);
  const made = snapshot.named_bodies?.filter((body) => !body.reference) ?? [];
  const pairs = pairsOfThePart(snapshot);
  const touching = pairs.filter((pair) => pair.verdict === "touching");
  const touchingWithin = touching.filter((pair) => !references.has(pair.a) && !references.has(pair.b));
  const clear = pairs
    .filter((pair) => pair.verdict === "clear" && pair.clearance_mm !== undefined)
    .sort((p, q) => p.clearance_mm! - q.clearance_mm!);
  const flagged = snapshot.print_check?.flagged ?? [];

  return (
    <>
      {flagged.slice(0, NAMES_LISTED).map((f, i) => (
        <FindingLine
          key={`${f.kind}/${f.body ?? ""}/${i}`}
          finding={f}
          named={snapshot.named_bodies !== undefined}
          pieces={snapshot.named_bodies?.find((body) => body.name === f.body)?.pieces}
        />
      ))}
      {flagged.length > NAMES_LISTED && (
        <div class="text-note">and {flagged.length - NAMES_LISTED} more to read</div>
      )}
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
        {snapshot.brief?.envelope?.fits && (
          <> · fits the {snapshot.brief.envelope.given.map((d) => fmt(d)).join(" × ")} envelope</>
        )}
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
      {made.length <= 4 ? (
        made.map((body) => <BodyLine key={body.name} body={body} />)
      ) : (
        <div>bodies: {names(made.map((body) => body.name))}</div>
      )}
      {references.size > 0 && <div>reference: {names([...references])}</div>}
      {touchingWithin.length > 0 && (
        <div>
          touching: {names(touchingWithin.map((pair) => `${pair.a} · ${pair.b}`))}
          {touching.length > touchingWithin.length && ` · ${touching.length - touchingWithin.length} more against references`}
        </div>
      )}
      {clear.length > 0 && (
        <div>
          closest:{" "}
          {clear.slice(0, CLOSEST_LISTED).map((pair, i) => (
            <span key={`${pair.a}/${pair.b}`}>
              {i > 0 && ", "}
              {pair.a} · {pair.b} <Strong>{fmt(pair.clearance_mm!)}</Strong> mm
            </span>
          ))}
          {clear.length > CLOSEST_LISTED &&
            ` · ${clear.length - CLOSEST_LISTED} more ${clear.length - CLOSEST_LISTED === 1 ? "pair" : "pairs"} clear`}
        </div>
      )}
      {snapshot.notes && (
        <div>
          {snapshot.notes.source}:{" "}
          {snapshot.notes.values.map((n) => `${n.label} ${noteValue(n.value)}`).join(" · ")}
          {snapshot.notes.dropped ? ` · ${snapshot.notes.dropped} more dropped` : ""}
        </div>
      )}
      <div>
        mesh: <Strong>{snapshot.triangles.toLocaleString()}</Strong> triangles within{" "}
        <Strong>{snapshot.resolution_mm.toFixed(3)}</Strong> mm
        {snapshot.faces !== undefined && (
          <>
            {" · "}
            <Strong>{snapshot.faces}</Strong> faces · <Strong>{snapshot.topological_edges}</Strong> edges
          </>
        )}
      </div>
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
    <div>
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
  );
}

function PairLine({ pair }: { pair: Pair }) {
  return (
    <div class="text-bad">
      {pair.a} · {pair.b}: interfering, <Strong>{fmt(pair.interference_mm3)}</Strong> mm³ shared
      {pair.depth_mm !== undefined && (
        <>
          {", "}
          <Strong>{fmt(pair.depth_mm)}</Strong> mm deep
        </>
      )}
    </div>
  );
}

/** A measured number, picked out of the sentence around it; long ones grouped by thousands so they can be read aloud. */
const Strong = ({ children }: { children: ComponentChildren }) => (
  <b class="text-ink font-medium">{typeof children === "string" ? grouped(children) : children}</b>
);

/** `55933456` as `55 933 456`, with a narrow no-break space; four digits and fewer stay as they are. */
function grouped(text: string): string {
  return text.replace(/\d{5,}/g, (digits) => digits.replace(/\B(?=(\d{3})+$)/g, "\u202f"));
}
