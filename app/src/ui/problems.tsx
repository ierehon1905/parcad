/**
 * What is wrong with the part, then what the kernel measured: the editor
 * pane's second tab.
 *
 * It used to float over the viewport's corner. Over the part is the one place
 * a report must not be — it covered the dial it was describing — and nobody
 * read it there; a tab beside the code is a place of its own, and the count
 * on the tab and the dot on each body in the tree say whether to open it.
 *
 * Every value comes out of the `EvaluationSnapshot` the host built. This
 * component selects and labels; it never derives. Lessons it keeps:
 *
 * - The z range sits beside the size: a stand whose pegs were on the wrong face
 *   measured a plausible 45 mm tall and stood 5.5 mm below its own base.
 * - What the part stands on is a problem when it is under a tenth of the
 *   footprint, which is what eighteen stubs look like and a slab never does.
 * - A pair of two references says nothing about the part, and clear pairs
 *   collapse to the three tightest: a plant shelf of 20 bodies listed 190.
 * - The mesh's own numbers come last: they describe how the part is drawn.
 */

import type { ComponentChildren } from "preact";
import * as engine from "../engine";
import { fmt } from "../engine";
import * as S from "../state";
import type { PrintFinding } from "../state";

type Snapshot = NonNullable<typeof S.snapshot.value>;
type Body = NonNullable<Snapshot["named_bodies"]>[number];
type Pair = NonNullable<Snapshot["between_bodies"]>[number];

/** Clear pairs listed under Measured, tightest first. */
const CLOSEST_LISTED = 3;
/** Names listed on one line before the rest are counted. */
const NAMES_LISTED = 6;

/** One thing to read about the part, and the bodies it is about. */
export interface Problem {
  key: string;
  /** `bad`: the part fails as written. `note`: it prints, and this should be read. */
  tone: "bad" | "note";
  /** What pointing at it singles out in the view. */
  bodies: string[];
  text: ComponentChildren;
}

/** Everything wrong with the part, failures first. */
export function problemsOf(snapshot: Snapshot): Problem[] {
  return [...failures(snapshot), ...findings(snapshot)];
}

/** How many problems there are of the worse kind present: what the tab says. */
export function problemCount(problems: Problem[]): { n: number; tone: Problem["tone"] } | undefined {
  const bad = problems.filter((p) => p.tone === "bad").length;
  if (bad) return { n: bad, tone: "bad" };
  return problems.length ? { n: problems.length, tone: "note" } : undefined;
}

export function Problems() {
  const snapshot = S.snapshot.value;
  if (!snapshot) {
    return <p class="m-0 px-4 py-4 text-ink-dim">Nothing is built yet.</p>;
  }
  const problems = problemsOf(snapshot);
  return (
    <div class="flex-1 min-h-0 overflow-auto py-2">
      {problems.length === 0 ? (
        <p class="m-0 px-4 py-2 text-good">Nothing is wrong with this part.</p>
      ) : (
        <ul class="m-0 p-0 list-none">
          {problems.map((problem) => (
            <ProblemRow key={problem.key} problem={problem} />
          ))}
        </ul>
      )}
      <Measured snapshot={snapshot} />
    </div>
  );
}

function ProblemRow({ problem }: { problem: Problem }) {
  return (
    <li
      class="flex items-baseline gap-2.5 px-4 py-1.5 hover:bg-panel-2"
      onMouseEnter={() => engine.focusBodies(problem.bodies)}
      onMouseLeave={() => engine.focusBodies([])}
    >
      <span class={`shrink-0 size-2 rounded-full translate-y-[-1px] ${problem.tone === "bad" ? "bg-bad" : "bg-note"}`} />
      <span class="text-ink">{problem.text}</span>
    </li>
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

/** What fails as written. */
function failures(snapshot: Snapshot): Problem[] {
  const out: Problem[] = [];
  const bad = (key: string, bodies: (string | undefined)[], text: ComponentChildren) =>
    out.push({ key, tone: "bad", bodies: bodies.filter((b): b is string => !!b), text });
  const envelope = snapshot.brief?.envelope;
  const budget = snapshot.brief?.budget_cm3;
  const dead = snapshot.unused_nodes ?? 0;

  snapshot.checks?.failed?.forEach((f) =>
    bad(`check/${f.check}`, [], (
      <>
        Check fails: <Code>{f.check}</Code>
        {f.measured_mm !== undefined && (
          <>
            , measured <Strong>{fmt(f.measured_mm)}</Strong> mm
          </>
        )}
        {f.measured_mm3 !== undefined && (
          <>
            , measured <Strong>{fmt(f.measured_mm3)}</Strong> mm³
          </>
        )}
      </>
    )),
  );
  snapshot.print_check?.failed?.forEach((f, i) =>
    bad(`print/${i}`, [f.body], (
      <>
        {f.kind === "thin" ? "Nothing prints" : "Won't print"}: {f.what}
      </>
    )),
  );
  if (snapshot.watertight === false) {
    bad("watertight", [], (
      <>
        Not watertight: <Strong>{snapshot.non_manifold_edges}</Strong> bad edges
      </>
    ));
  }
  if (envelope && !envelope.fits) {
    bad("envelope", [], (
      <>
        Over the {envelope.given.map((d) => fmt(d)).join(" × ")} envelope by <Strong>{fmt(envelope.over_mm ?? 0)}</Strong> mm
        along {envelope.on}
      </>
    ));
  }
  if (budget && !budget.fits) {
    bad("budget", [], (
      <>
        <Strong>{fmt(budget.measured)}</Strong> cm³ against a {fmt(budget.given)} cm³ budget
      </>
    ));
  }
  if (onStubs(snapshot)) {
    bad("stubs", [], (
      <>
        Stands on stubs: <StandsOn snapshot={snapshot} />
      </>
    ));
  }
  if (!snapshot.named_bodies && snapshot.bodies > 1) {
    bad("separate", [], (
      <>
        Falls apart into <Strong>{snapshot.bodies}</Strong> separate bodies
      </>
    ));
  }
  snapshot.named_bodies
    ?.filter((body) => !body.reference && body.watertight === false)
    .forEach((body) => bad(`watertight/${body.name}`, [body.name], <>{body.name} is not watertight</>));
  pairsOfThePart(snapshot)
    .filter(interfering)
    .forEach((pair) =>
      bad(`clash/${pair.a}/${pair.b}`, [pair.a, pair.b], (
        <>
          {pair.a} and {pair.b} overlap by <Strong>{fmt(pair.interference_mm3)}</Strong> mm³
          {pair.depth_mm !== undefined && (
            <>
              , <Strong>{fmt(pair.depth_mm)}</Strong> mm deep
            </>
          )}
        </>
      )),
    );
  if (!S.linkedEdgeCount.value && S.lastTreatments.value.length > 0) {
    bad("curves", [], <>No final treatment curves are available</>);
  }
  if (dead > 0) {
    bad("unused", [], (
      <>
        <Strong>{dead}</Strong> {dead === 1 ? "shape" : "shapes"} the part never uses
      </>
    ));
  }
  return out;
}

/** What prints but should be read. */
function findings(snapshot: Snapshot): Problem[] {
  const named = snapshot.named_bodies !== undefined;
  return (snapshot.print_check?.flagged ?? []).map((f, i) => ({
    key: `finding/${f.kind}/${i}`,
    tone: "note",
    bodies: [...(named && f.body ? [f.body] : []), ...(f.between ?? [])],
    text: (
      <FindingText
        finding={f}
        named={named}
        pieces={snapshot.named_bodies?.find((body) => body.name === f.body)?.pieces}
      />
    ),
  }));
}

/** One flagged finding: its kind, where, and its own number. */
function FindingText({ finding: f, pieces, named }: { finding: PrintFinding; pieces?: number; named: boolean }) {
  const where = named && f.body ? ` in ${f.body}` : "";
  switch (f.kind) {
    case "thin":
      return (
        <>
          {f.thin_kind === "feather" ? "Feather edge" : "Thin wall"} <Strong>{fmt(f.thickness_mm ?? 0)}</Strong> mm
          {f.between && ` between ${f.between[0]} and ${f.between[1]}`}
          {where}
        </>
      );
    case "collision":
      return (
        <>
          {f.between ? `${f.between[0]} cuts into ${f.between[1]}` : "A cut takes from a feature"}
          {f.removed_mm3 !== undefined && (
            <>
              , <Strong>{fmt(f.removed_mm3)}</Strong> mm³ taken
            </>
          )}
          {where}
        </>
      );
    case "overhang":
      return (
        <>
          Overhang{where}
          {f.unsupported_mm2 !== undefined && (
            <>
              : <Strong>{fmt(f.unsupported_mm2)}</Strong> mm² faces down
            </>
          )}
        </>
      );
    case "pieces":
      return (
        <>
          {(named && f.body) || "The part"} prints in <Strong>{pieces ?? f.pieces?.length ?? 0}</Strong> pieces
        </>
      );
    case "floating":
      return <>A piece starts above the bed{where}</>;
  }
}

/** What is measured and right, then how it is drawn. */
function Measured({ snapshot }: { snapshot: Snapshot }) {
  const [sx, sy, sz] = snapshot.size;
  const references = referenceNames(snapshot);
  const made = snapshot.named_bodies?.filter((body) => !body.reference) ?? [];
  const pairs = pairsOfThePart(snapshot);
  const touching = pairs.filter((pair) => pair.verdict === "touching");
  const touchingWithin = touching.filter((pair) => !references.has(pair.a) && !references.has(pair.b));
  const clear = pairs
    .filter((pair) => pair.verdict === "clear" && pair.clearance_mm !== undefined)
    .sort((p, q) => p.clearance_mm! - q.clearance_mm!);
  const checks = snapshot.checks;
  const print = snapshot.print_check;

  return (
    <section class="mt-3 pt-3 border-t border-line">
      <h3 class="m-0 px-4 pb-1.5 text-tiny font-medium uppercase tracking-[0.06em] text-ink-dim">Measured</h3>
      <dl class="m-0 grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1 px-4 text-ink-dim">
        <Row label="Size">
          <Strong>
            {fmt(sx)} × {fmt(sy)} × {fmt(sz)}
          </Strong>{" "}
          mm
        </Row>
        <Row label="Height">
          z <Strong>{fmt(snapshot.bounds_min[2])}</Strong> to <Strong>{fmt(snapshot.bounds_max[2])}</Strong>
        </Row>
        {snapshot.volume_mm3 !== undefined ? (
          <Row label="Volume">
            <Strong>{fmt(snapshot.volume_mm3)}</Strong> mm³
            {snapshot.brief?.envelope?.fits && (
              <> · fits the {snapshot.brief.envelope.given.map((d) => fmt(d)).join(" × ")} envelope</>
            )}
          </Row>
        ) : (
          <Row label="Surface">
            <Strong>{fmt(snapshot.area_mm2)}</Strong> mm²
          </Row>
        )}
        {snapshot.surface && (
          <Row label="Shell">
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
          </Row>
        )}
        {snapshot.thickened_mm && (
          <Row label="Thickened">
            <Strong>{fmt(snapshot.thickened_mm.min)}</Strong>
            {snapshot.thickened_mm.max !== snapshot.thickened_mm.min && (
              <>
                {" to "}
                <Strong>{fmt(snapshot.thickened_mm.max)}</Strong>
              </>
            )}{" "}
            mm
          </Row>
        )}
        {snapshot.stands_on && !onStubs(snapshot) && (
          <Row label="Stands on">
            <StandsOn snapshot={snapshot} />
          </Row>
        )}
        {checks?.verdict === "passed" && (
          <Row label="Checks">
            <Strong>{checks.passed}</Strong> {checks.passed === 1 ? "holds" : "hold"}
          </Row>
        )}
        {print?.verdict === "passed" && <Row label="Print">clean</Row>}
        {made.length > 0 && (
          <Row label="Bodies">
            {made.length <= NAMES_LISTED
              ? made.map((body, i) => (
                  <span key={body.name}>
                    {i > 0 && " · "}
                    <BodyVolume body={body} />
                  </span>
                ))
              : names(made.map((body) => body.name))}
          </Row>
        )}
        {references.size > 0 && <Row label="Reference">{names([...references])}</Row>}
        {touchingWithin.length > 0 && (
          <Row label="Touching">
            {names(touchingWithin.map((pair) => `${pair.a} · ${pair.b}`))}
            {touching.length > touchingWithin.length &&
              ` · ${touching.length - touchingWithin.length} more against references`}
          </Row>
        )}
        {clear.length > 0 && (
          <Row label="Closest">
            {clear.slice(0, CLOSEST_LISTED).map((pair, i) => (
              <span key={`${pair.a}/${pair.b}`}>
                {i > 0 && ", "}
                {pair.a} · {pair.b} <Strong>{fmt(pair.clearance_mm!)}</Strong> mm
              </span>
            ))}
            {clear.length > CLOSEST_LISTED &&
              ` · ${clear.length - CLOSEST_LISTED} more ${clear.length - CLOSEST_LISTED === 1 ? "pair" : "pairs"} clear`}
          </Row>
        )}
        {snapshot.notes && (
          <Row label="Noted">
            {snapshot.notes.values.map((n) => `${n.label} ${noteValue(n.value)}`).join(" · ")}
            {snapshot.notes.dropped ? ` · ${snapshot.notes.dropped} more dropped` : ""}
            <span class="block text-tiny">{snapshot.notes.source}</span>
          </Row>
        )}
        <Row label="Mesh">
          <Strong>{snapshot.triangles.toLocaleString()}</Strong> triangles within{" "}
          <Strong>{snapshot.resolution_mm.toFixed(3)}</Strong> mm
          {snapshot.faces !== undefined && (
            <>
              {" · "}
              <Strong>{snapshot.faces}</Strong> faces · <Strong>{snapshot.topological_edges}</Strong> edges
            </>
          )}
        </Row>
      </dl>
    </section>
  );
}

function Row({ label, children }: { label: string; children: ComponentChildren }) {
  return (
    <>
      <dt class="text-ink-dim">{label}</dt>
      <dd class="m-0">{children}</dd>
    </>
  );
}

/** A list of names, the first few written out and the rest counted. */
function names(list: string[]): string {
  const shown = list.slice(0, NAMES_LISTED).join(", ");
  return list.length > NAMES_LISTED ? `${shown} +${list.length - NAMES_LISTED}` : shown;
}

function noteValue(value: number | string | number[]): string {
  if (typeof value === "number") return fmt(value);
  return Array.isArray(value) ? value.map(fmt).join(", ") : value;
}

function StandsOn({ snapshot }: { snapshot: Snapshot }) {
  const stands = snapshot.stands_on!;
  return (
    <>
      <Strong>{fmt(stands.area_mm2)}</Strong> mm² in <Strong>{stands.patches}</Strong>{" "}
      {stands.patches === 1 ? "patch" : "patches"}
    </>
  );
}

function BodyVolume({ body }: { body: Body }) {
  return (
    <>
      {body.name}{" "}
      {body.volume_mm3 !== undefined ? (
        <>
          <Strong>{fmt(body.volume_mm3)}</Strong> mm³
        </>
      ) : (
        "a surface"
      )}
    </>
  );
}

/** What the part's own script calls something: a check's label. */
const Code = ({ children }: { children: ComponentChildren }) => <code class="font-mono text-small">{children}</code>;

/** A measured number, picked out of the sentence around it; long ones grouped by thousands so they can be read aloud. */
const Strong = ({ children }: { children: ComponentChildren }) => (
  <b class="text-ink font-medium tabular-nums">{typeof children === "string" ? grouped(children) : children}</b>
);

/** `55933456` as `55 933 456`, with a narrow no-break space; four digits and fewer stay as they are. */
function grouped(text: string): string {
  return text.replace(/\d{5,}/g, (digits) => digits.replace(/\B(?=(\d{3})+$)/g, " "));
}
