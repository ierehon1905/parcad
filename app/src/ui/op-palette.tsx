/**
 * The operation palette: every name in the DSL, drawn, grouped and insertable.
 *
 * A CAD toolbar normally *starts a command* — click Fillet in Fusion and a
 * dialog opens, you pick edges in the viewport, you press OK and a feature
 * appears in a tree you cannot otherwise edit. None of that shape fits here,
 * and copying it would break the rule the rest of this app is built on:
 * `part.js` is the only authoritative file. A second authoring path that wrote
 * features somewhere else would be a model the agent on the MCP endpoint cannot
 * read and the user cannot diff.
 *
 * So this toolbar does the one thing that is consistent with that. **Pressing
 * an icon writes the call into the source**, at the cursor, with its first
 * argument selected so the next keystroke replaces it. Nothing here can produce
 * a part the script does not say.
 *
 * Which turns the palette into something more useful than a shortcut: it is the
 * shared vocabulary made visible. Every entry shows the real signature, because
 * that signature is what an agent working on the same part types, and what the
 * user will see it type. A person who learns this palette has learned the
 * language the model is already speaking — the two halves of the project
 * pointed at one surface.
 *
 * Only honest facts go in a detail line: which axis a primitive runs along,
 * that `scale` refuses to be non-uniform, that `loft` and `sweep` exist in the
 * exact kernel and nowhere else. A palette that promised an operation the
 * backend then refused would be the confident wrong answer this project spends
 * most of its effort not giving.
 */

import { useSignal } from "@preact/signals";
import { useEffect, useRef } from "preact/hooks";

import { flashInsert } from "../editor-marks";
import * as S from "../state";
import { Icon, type IconName } from "./icons";
import { tip } from "./tooltip";

export interface Op {
  /** The name as it is written in a script. */
  name: string;
  icon: IconName;
  /** The real signature, parameter names and all. */
  signature: string;
  /** What it does, and anything true a caller would otherwise find out late. */
  detail: string;
  /** What pressing it writes. The first argument is left selected. */
  snippet: string;
}

export interface OpGroup {
  id: string;
  label: string;
  icon: IconName;
  /** Why this group exists, shown above its ops. */
  blurb: string;
  ops: Op[];
}

/**
 * A method on a shape is written with the leading dot exactly as it is
 * inserted, so the palette never shows a form the script cannot contain.
 */
export const OP_GROUPS: OpGroup[] = [
  {
    id: "solid",
    label: "Solid",
    icon: "box",
    blurb: "Primitives. Every one is centred on the origin — place it with .at().",
    ops: [
      {
        name: "box",
        icon: "box",
        signature: "box(x, y, z)",
        detail: "A block with those full extents in millimetres.",
        snippet: "box(30, 20, 10)",
      },
      {
        name: "cylinder",
        icon: "cylinder",
        signature: "cylinder(radius, height)",
        detail: "Along Z. Height is the full height, not the half.",
        snippet: "cylinder(6, 20)",
      },
      {
        name: "sphere",
        icon: "sphere",
        signature: "sphere(radius)",
        detail: "Exact in both backends.",
        snippet: "sphere(8)",
      },
      {
        name: "cone",
        icon: "cone",
        signature: "cone(bottomRadius, topRadius, height)",
        detail:
          "Along Z. A zero radius gives a point; equal radii give a cylinder the long way round.",
        snippet: "cone(10, 4, 20)",
      },
      {
        name: "torus",
        icon: "torus",
        signature: "torus(major, minor, { sweep })",
        detail:
          "Minor must stay under major. sweep is degrees anticlockwise from +X, 360 by default.",
        snippet: "torus(20, 4)",
      },
      {
        name: "ngon",
        icon: "ngon",
        signature: "ngon(sides, size, height, { across, draft })",
        detail:
          'size is across the corners unless you pass { across: "flats" } — which is how hex bar and every spanner are specified.',
        snippet: "ngon(6, 12, 8)",
      },
    ],
  },
  {
    id: "profile",
    label: "Profile",
    icon: "extrude",
    blurb: "A shape you draw rather than pick. Outlines are convex; union two of them for an L.",
    ops: [
      {
        name: "extrude",
        icon: "extrude",
        signature: "extrude(outline, height, { draft })",
        detail: "A closed convex [x, y] outline given a thickness along Z, centred in Z.",
        snippet: "extrude([[-10, -5], [10, -5], [10, 5], [-10, 5]], 3)",
      },
      {
        name: "revolve",
        icon: "revolve",
        signature: "revolve(section)",
        detail:
          "A convex [radius, z] section spun a full turn about Z. Radii must be at or above zero.",
        snippet: "revolve([[0, -5], [8, -5], [8, 5], [0, 5]])",
      },
      {
        name: "pipe",
        icon: "pipe",
        signature: "pipe(path, diameter, { bend })",
        detail:
          "A round tube along straight runs joined by tangent arcs. Exact in both backends — keep a round section here rather than in sweep().",
        snippet: "pipe([[0, 0, 0], [40, 0, 0], [40, 30, 0]], 8, { bend: 10 })",
      },
      {
        name: "sweep",
        icon: "sweep",
        signature: "sweep(profile, path, { bend })",
        detail:
          "An authored section along the same run-and-bend path. B-rep only: the implicit backend refuses it by name.",
        snippet:
          "sweep([[-3, -3], [3, -3], [3, 3], [-3, 3]], [[0, 0, 0], [40, 0, 0], [40, 25, 0]], { bend: 8 })",
      },
      {
        name: "loft",
        icon: "loft",
        signature: "loft(sections, { smooth })",
        detail:
          "Sections must rise strictly in z and carry the same number of outline points, because walls pair vertices by index. B-rep only.",
        snippet:
          "loft([\n" +
          "  { z: 0, outline: [[-12, -12], [12, -12], [12, 12], [-12, 12]] },\n" +
          "  { z: 20, outline: [[-6, -6], [6, -6], [6, 6], [-6, 6]] },\n" +
          "])",
      },
    ],
  },
  {
    id: "combine",
    label: "Combine",
    icon: "union",
    blurb: "Booleans. Each takes any number of shapes, and an options object last.",
    ops: [
      {
        name: "union",
        icon: "union",
        signature: ".union(...shapes, { blend })",
        detail: "Fuse. Also the free function union(a, b) when there is no obvious base.",
        snippet: ".union(other)",
      },
      {
        name: "cut",
        icon: "cut",
        signature: ".cut(...tools, { blend })",
        detail: "Subtract each tool from this shape.",
        snippet: ".cut(tool)",
      },
      {
        name: "intersect",
        icon: "intersect",
        signature: ".intersect(...shapes)",
        detail:
          "Keep only what every shape covers. A blended intersection is refused rather than approximated.",
        snippet: ".intersect(other)",
      },
      {
        name: "blend",
        icon: "blend",
        signature: ".union(other, { blend: radius })",
        detail:
          "A rounded transition where the two solids meet. It pushes the surface out by up to blend/4 near the seam, so blended coincident faces come out slightly over nominal.",
        snippet: ".union(other, { blend: 3 })",
      },
    ],
  },
  {
    id: "edge",
    label: "Edge",
    icon: "fillet",
    blurb:
      "Treatments, and the selectors that aim them. >Z is furthest in +Z, |X is parallel to X; join terms with and.",
    ops: [
      {
        name: "edges",
        icon: "edges",
        signature: ".edges(selector)",
        detail:
          "Select logical edges for a treatment. Re-resolved after every evaluation, never a stored edge number.",
        snippet: '.edges(">Z and |X")',
      },
      {
        name: "vertices",
        icon: "vertices",
        signature: ".vertices(selector)",
        detail: "Select corners; the exact backend expands each to its incident edges.",
        snippet: '.vertices(">X and >Y and >Z")',
      },
      {
        name: "fillet",
        icon: "fillet",
        signature: ".fillet(radius, selector)",
        detail:
          "A circular round. Hover the call afterwards to see how many edges it actually caught.",
        snippet: '.fillet(2, ">Z")',
      },
      {
        name: "chamfer",
        icon: "chamfer",
        signature: ".chamfer(distance, selector)",
        detail: "A flat bevel, distance measured back along each face.",
        snippet: '.chamfer(1, ">Z")',
      },
      {
        name: "smooth",
        icon: "smooth",
        signature: ".edges(selector).smooth(radius)",
        detail:
          "A fillet with curvature continuity instead of tangency — no crease in the reflection where the blend meets the wall.",
        snippet: '.edges(">Z").smooth(3)',
      },
      {
        name: "shell",
        icon: "shell",
        signature: ".shell(thickness)",
        detail: "Hollow this out, leaving a wall of that thickness inside the current surface.",
        snippet: ".shell(2)",
      },
      {
        name: "offset",
        icon: "offset",
        signature: ".offset(distance)",
        detail:
          "Grow or shrink the whole surface. A general offset on the exact kernel is refused rather than approximated.",
        snippet: ".offset(1)",
      },
    ],
  },
  {
    id: "place",
    label: "Place",
    icon: "translate",
    blurb: "Where a primitive ends up, and what it is called afterwards.",
    ops: [
      {
        name: "at",
        icon: "translate",
        signature: ".at(x, y, z)",
        detail:
          "Move to a position. The usual way to place a primitive, which is born on the origin.",
        snippet: ".at(10, 0, 0)",
      },
      {
        name: "translate",
        icon: "translate",
        signature: ".translate(x, y, z)",
        detail:
          "Move by an offset. Same operation as .at(); the name says whether the numbers are a place or a step.",
        snippet: ".translate(10, 0, 0)",
      },
      {
        name: "rotate",
        icon: "rotate",
        signature: ".rotate(axis, degrees)",
        detail:
          'About an axis through the origin. Anticlockwise looking down the axis; "x", "y", "z" or a vector.',
        snippet: '.rotate("z", 45)',
      },
      {
        name: "mirror",
        icon: "mirror",
        signature: ".mirror(axis)",
        detail: "Reflect through the plane normal to that axis, at the origin.",
        snippet: '.mirror("x")',
      },
      {
        name: "scale",
        icon: "scale",
        signature: ".scale(factor)",
        detail:
          "Uniform only. A non-uniform scale is refused, because it turns a cylinder into something the kernel has no exact surface for.",
        snippet: ".scale(2)",
      },
      {
        name: "tag",
        icon: "tag",
        signature: ".tag(name)",
        detail:
          "Name this shape's surfaces. Tags come back in the report, colour the render, and are how an agent asks about one region rather than the whole part.",
        snippet: '.tag("mount-face")',
      },
    ],
  },
  {
    id: "pattern",
    label: "Pattern",
    icon: "grid",
    blurb:
      "Point sets, and the two ways to use one. grid() and polar() return points; repeat() and around() place shapes.",
    ops: [
      {
        name: "repeat",
        icon: "repeat",
        signature: "repeat(shape, points)",
        detail: "One copy of the shape at every [x, y] or [x, y, z], unioned.",
        snippet: "repeat(hole, grid(3, 2, 20, 15))",
      },
      {
        name: "grid",
        icon: "grid",
        signature: "grid(cols, rows, dx, dy)",
        detail: "A centred rectangular point set with that spacing.",
        snippet: "grid(3, 2, 20, 15)",
      },
      {
        name: "polar",
        icon: "polar",
        signature: "polar(count, radius, { start, straddle })",
        detail:
          "A bolt circle. straddle turns it half a step so no hole lands on a centreline — the flange convention, named rather than written as arithmetic.",
        snippet: "polar(6, 30, { straddle: true })",
      },
      {
        name: "around",
        icon: "around",
        signature: "around(shape, count, axis)",
        detail:
          "Spin a whole shape about an axis and union the copies. For a feature that is not itself rotationally symmetric.",
        snippet: 'around(slot, 4, "z")',
      },
    ],
  },
  {
    id: "hole",
    label: "Hole",
    icon: "hole",
    blurb:
      "Fastener sizes come from METRIC_FASTENERS. A drill diameter should never be a literal in a part.",
    ops: [
      {
        name: "holeFor",
        icon: "hole",
        signature: "holeFor(thread, depth, { fit, tapped, through })",
        detail:
          "A cutter sized from the thread designation: clearance by default, tap drill with { tapped: true }. Cut it, do not union it.",
        snippet: 'holeFor("M5", 12, { through: true })',
      },
      {
        name: "counterbore",
        icon: "counterbore",
        signature: "counterbore(thread)",
        detail: "The { diameter, depth } a socket head needs. A lookup, not a shape.",
        snippet: 'counterbore("M5")',
      },
      {
        name: "countersink",
        icon: "countersink",
        signature: "countersink(head, includedAngle)",
        detail:
          "The frustum a countersink drill leaves, positioned with its wide end at z = 0. 90° is ISO metric; pass 82 for imperial.",
        snippet: 'countersink("M5")',
      },
      {
        name: "clearance",
        icon: "hole",
        signature: "clearance(thread, fit)",
        detail: 'The clearance drill diameter — "close", "normal" or "free".',
        snippet: 'clearance("M5")',
      },
      {
        name: "tapDrill",
        icon: "thread",
        signature: "tapDrill(thread)",
        detail: "The tapping drill diameter for a metric coarse thread.",
        snippet: 'tapDrill("M5")',
      },
    ],
  },
];

const GROUP_BUTTON =
  "flex items-center gap-1.5 px-2 py-1 rounded-md border cursor-pointer " +
  "text-ink-dim border-transparent hover:text-ink hover:border-line";
/** The one currently showing its ops. */
const GROUP_BUTTON_OPEN =
  "flex items-center gap-1.5 px-2 py-1 rounded-md border cursor-pointer " +
  "text-accent bg-accent-deep/45 border-accent-edge";

export function OpPalette() {
  const open = useSignal<string | undefined>(undefined);
  const bar = useRef<HTMLDivElement>(null);

  // Outside click and Escape. On the capture phase so a press on another
  // group's button closes this one before that button's own handler runs.
  useEffect(() => {
    const down = (event: PointerEvent) => {
      const target = event.target as Node | null;
      if (target && !bar.current?.contains(target) && !inFlyout(target)) open.value = undefined;
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") open.value = undefined;
    };
    document.addEventListener("pointerdown", down, true);
    document.addEventListener("keydown", escape);
    window.addEventListener("resize", () => (open.value = undefined));
    return () => {
      document.removeEventListener("pointerdown", down, true);
      document.removeEventListener("keydown", escape);
    };
  }, []);

  return (
    <div
      ref={bar}
      class="relative flex flex-none items-center gap-0.5 px-2 py-1.5 border-b border-line bg-panel-2/60 overflow-x-auto"
      // The bar scrolls sideways in a narrow window. A flyout pinned to a
      // button that has just slid out from under it would point at nothing.
      onScroll={() => (open.value = undefined)}
    >
      {OP_GROUPS.map((group) => (
        <div key={group.id} class="relative">
          <button
            type="button"
            class={open.value === group.id ? GROUP_BUTTON_OPEN : GROUP_BUTTON}
            {...tip({ title: group.label, text: group.blurb })}
            onClick={() => (open.value = open.value === group.id ? undefined : group.id)}
          >
            <Icon name={group.icon} class="size-5 shrink-0" />
            <span class="text-small">{group.label}</span>
          </button>
          {open.value === group.id && (
            <Flyout
              group={group}
              onChoose={(op) => {
                open.value = undefined;
                insertSnippet(op.snippet);
              }}
            />
          )}
        </div>
      ))}
    </div>
  );
}

/** Whether a node is inside an open flyout, which is not inside the bar. */
function inFlyout(node: Node): boolean {
  return !!(node as Element).closest?.("[data-flyout]");
}

/**
 * One group's ops, as a panel of rows.
 *
 * Positioned `fixed` rather than `absolute`, which is not a detail: the bar
 * above scrolls sideways, so it computes to `overflow: auto` on *both* axes,
 * and an absolutely-positioned child was clipped to the bar's own 41 px height.
 * The panel laid out correctly and reported a sensible rectangle — it simply
 * was not drawn, which is why this needed a screenshot to catch rather than a
 * query. `fixed` escapes the clip; the coordinates come from the button.
 */
function Flyout({ group, onChoose }: { group: OpGroup; onChoose: (op: Op) => void }) {
  return (
    <div
      data-flyout
      class="fixed z-40 w-[min(430px,86vw)] max-h-[62vh] overflow-auto p-1.5
             rounded-xl border border-line bg-glass/95 backdrop-blur-lg
             shadow-[0_18px_44px_rgb(0_0_0/0.55)]"
      ref={(element) => {
        if (!element) return;
        // The parent is the group's own wrapper, which is exactly the button's
        // box — so no ref has to be threaded through the map above.
        const anchor = element.parentElement!.getBoundingClientRect();
        const box = element.getBoundingClientRect();
        element.style.top = `${Math.round(anchor.bottom + 6)}px`;
        element.style.left = `${Math.round(
          Math.min(Math.max(anchor.left, 6), window.innerWidth - box.width - 6),
        )}px`;
      }}
    >
      <p class="m-0 px-2 pt-1 pb-2 text-ink-dim text-tiny leading-[1.5]">{group.blurb}</p>
      {group.ops.map((op) => (
        <button
          key={op.name}
          type="button"
          class="flex w-full items-start gap-2.5 px-2 py-1.5 text-left rounded-lg cursor-pointer
                 border border-transparent text-ink-dim hover:border-accent-edge hover:bg-accent-deep/35"
          // The row already carries the signature and the sentence; the tooltip's
          // job is the one thing the row does not show — what pressing it will
          // actually write, which for `loft` is four lines.
          {...tip({
            title: `insert ${op.name}`,
            code: op.snippet,
            text: "Written at the cursor, with the first argument selected.",
          })}
          onClick={() => onChoose(op)}
        >
          <Icon name={op.icon} class="size-7 shrink-0 text-ink" />
          <span class="min-w-0 flex-1">
            <span class="block font-mono text-small text-ink [overflow-wrap:anywhere]">
              {op.signature}
            </span>
            <span class="block text-tiny leading-[1.45] mt-0.5">{op.detail}</span>
          </span>
        </button>
      ))}
    </div>
  );
}

/**
 * Write a call from the palette into the part.
 *
 * Two things make the difference between a palette worth pressing and one you
 * press once. Continuation lines are re-indented to the line the cursor is on,
 * so a `loft` dropped inside a function does not land against the left margin.
 * And the first argument is left *selected* rather than the caret being parked
 * after the call, because every one of these snippets carries a plausible
 * number that is not yours: the next keystroke should replace 30, not append
 * to it.
 */
export function insertSnippet(snippet: string) {
  const editor = S.editor();
  const range = editor.state.selection.main;
  const line = editor.state.doc.lineAt(range.from);
  const indent = /^[ \t]*/.exec(line.text)![0];
  const text = snippet.split("\n").join(`\n${indent}`);

  const opening = text.indexOf("(");
  const start = opening + 1;
  const end = opening < 0 ? -1 : firstArgumentEnd(text, start);
  const selected = end > start;

  editor.dispatch({
    changes: { from: range.from, to: range.to, insert: text },
    selection: selected
      ? { anchor: range.from + start, head: range.from + end }
      : { anchor: range.from + text.length },
    scrollIntoView: true,
    // In the same transaction, in post-change coordinates: the call highlights
    // itself as it lands rather than a frame later.
    effects: flashInsert.of({ from: range.from, to: range.from + text.length }),
  });
  editor.focus();

  // The fade is CSS; this only takes the decoration away once it has finished,
  // so the mark does not outlive the animation and re-appear on a later
  // re-render. A second insertion cancels the first — the newest write is the
  // one worth pointing at.
  window.clearTimeout(flashTimer);
  flashTimer = window.setTimeout(() => {
    S.editorRef.current?.dispatch({ effects: flashInsert.of(undefined) });
  }, FLASH_MS);
}

/** Long enough to catch the eye across the pane, short enough not to linger. */
const FLASH_MS = 900;
let flashTimer: number | undefined;

/**
 * Where the first argument of a call ends.
 *
 * A scan rather than a split on commas, because half these snippets have a
 * comma inside the first argument — `extrude([[-10, -5], …], 3)` — and a
 * selection that stopped at the first one would hand back a broken expression.
 * Nesting and string literals are all this has to understand; the snippets are
 * written in this file and none of them contains a regex or a comment.
 */
export function firstArgumentEnd(text: string, start: number): number {
  let depth = 0;
  let quote = "";
  for (let i = start; i < text.length; i++) {
    const c = text[i];
    if (quote) {
      if (c === "\\") i++;
      else if (c === quote) quote = "";
      continue;
    }
    if (c === '"' || c === "'" || c === "`") quote = c;
    else if (c === "(" || c === "[" || c === "{") depth++;
    else if (c === ")" || c === "]" || c === "}") {
      if (depth === 0) return i;
      depth--;
    } else if (c === "," && depth === 0) return i;
  }
  return start;
}
