/**
 * What a treatment call is currently doing, and the edits that follow from it.
 *
 * The editor can say a great deal about a `.fillet(…)` without evaluating
 * anything — its radius, its selector, whether it pins a count. It cannot say
 * how many edges that selector actually resolves to, or whether a tag would
 * select the same ones; only the kernel knows, and it answers per node through
 * `inspect_edge_target`. Both halves are assembled here so the tooltip stays
 * presentation and the edits stay testable without a DOM.
 *
 * Everything in this file is a pure function of the graph node and the resolved
 * target. Nothing here decides *when* to resolve — see `treatment-hover.ts`.
 */

/** The resolved target the kernel returns for one treatment node. */
export interface ResolvedTarget {
  node: number;
  edges: unknown[];
  vertices?: unknown[];
  /** Tags whose live edge set is exactly this target. */
  provenance?: string[];
}

/** The intent-graph node behind a treatment call. */
export interface TreatmentNode {
  op: string;
  radius?: number;
  distance?: number;
  selector?: unknown;
  vertices?: unknown;
  expect?: { count: number };
  recipe?: { continuity?: string; corner?: string };
}

/** One line of the tooltip: a label, a value, and how it should read. */
export interface InfoRow {
  label: string;
  value: string;
  tone?: "ok" | "warn";
}

/** A source edit the tooltip can offer as a button. */
export interface TreatmentAction {
  label: string;
  /** Why this edit is worth making, shown as the button's title. */
  detail: string;
  /** Replacement text, and the range within the treatment call to replace. */
  edit: SourceEdit;
}

export interface SourceEdit {
  from: number;
  to: number;
  insert: string;
}

/** How the selector was authored, as source text. */
export function selectorText(node: TreatmentNode): string | undefined {
  const selector = node.vertices ?? node.selector;
  if (selector === undefined) return undefined;
  return typeof selector === "string" ? `"${selector}"` : JSON.stringify(selector);
}

/** The count a treatment currently acts on: corners if it targets corners. */
export function resolvedCount(target: ResolvedTarget): {
  count: number;
  noun: "edge" | "corner";
} {
  const corners = target.vertices?.length ?? 0;
  return corners > 0
    ? { count: corners, noun: "corner" }
    : { count: target.edges.length, noun: "edge" };
}

function plural(count: number, noun: string) {
  return `${count} ${noun}${count === 1 ? "" : "s"}`;
}

/**
 * The tooltip body.
 *
 * `target` is absent until the kernel answers — while resolving, in a plain
 * browser, or when the treatment failed to evaluate. The rows that need no
 * geometry are still shown, rather than the tooltip being all or nothing.
 */
export function treatmentRows(
  node: TreatmentNode,
  target?: ResolvedTarget,
  pending = false,
): InfoRow[] {
  const rows: InfoRow[] = [];
  const size = node.op === "chamfer" ? node.distance : node.radius;
  if (size !== undefined) {
    rows.push({
      label: node.op === "chamfer" ? "distance" : "radius",
      value: `${size} mm`,
    });
  }

  const selector = selectorText(node);
  if (selector) {
    rows.push({ label: node.vertices !== undefined ? "corners" : "selector", value: selector });
  }

  if (node.recipe?.continuity === "curvature") {
    rows.push({ label: "continuity", value: "curvature (G2)" });
  }

  if (target) {
    const { count, noun } = resolvedCount(target);
    rows.push({ label: "resolves", value: plural(count, noun) });
    if (node.expect) {
      rows.push(
        node.expect.count === count
          ? { label: "expect", value: `${node.expect.count} — holds`, tone: "ok" }
          : {
              label: "expect",
              value: `${node.expect.count}, but ${plural(count, noun)} match`,
              tone: "warn",
            },
      );
    }
    if (target.provenance?.length) {
      rows.push({ label: "same as", value: target.provenance.join(", ") });
    }
  } else if (pending) {
    rows.push({ label: "resolves", value: "…" });
  }

  return rows;
}

/** The heading, e.g. `.fillet` — the authored method, not the graph op. */
export function treatmentTitle(node: TreatmentNode, method?: string): string {
  return `.${method ?? node.op}`;
}

/**
 * Edits worth offering for this treatment, given what it resolves to.
 *
 * Both of these turn a selector that happens to be right today into one that
 * says so — an `expect` makes a changed target an error rather than a silently
 * different part, and a provenance selector names the operation that made the
 * edges instead of where they currently sit.
 *
 * `call` is the treatment call's text and `callFrom` its document offset, so
 * the returned edits are absolute document ranges.
 */
export function treatmentActions(
  node: TreatmentNode,
  call: string,
  callFrom: number,
  target?: ResolvedTarget,
): TreatmentAction[] {
  if (!target) return [];
  const actions: TreatmentAction[] = [];
  const { count, noun } = resolvedCount(target);

  if (!node.expect) {
    // After `.edges(…)` / `.vertices(…)`, before the treatment call it feeds.
    const selection = /\.(edges|vertices)\s*\([^)]*\)/.exec(call);
    if (selection) {
      const at = callFrom + selection.index + selection[0].length;
      actions.push({
        label: `pin expect({ count: ${count} })`,
        detail:
          `This selector matches ${plural(count, noun)} today. Pinning the count turns a ` +
          "later edit that changes the target into a build error instead of a quietly different part.",
        edit: { from: at, to: at, insert: `.expect({ count: ${count} })` },
      });
    }
  }

  // Only offered when the tag selects exactly this set — the kernel checks
  // that, so accepting the edit cannot change which edges are treated.
  const tag = target.provenance?.[0];
  if (tag && typeof node.selector === "string") {
    const literal = new RegExp(`\\.edges\\s*\\(\\s*${quoteRegex(node.selector)}\\s*\\)`).exec(call);
    if (literal) {
      actions.push({
        label: `select by generatedBy: "${tag}"`,
        detail:
          `${tag} currently generates exactly these ${plural(count, noun)}. A directional ` +
          "selector follows whichever edge is furthest out; a provenance selector keeps naming " +
          "the feature after a dimension change moves that extreme somewhere else.",
        edit: {
          from: callFrom + literal.index,
          to: callFrom + literal.index + literal[0].length,
          insert: `.edges({ generatedBy: ${JSON.stringify(tag)} })`,
        },
      });
    }
  }

  return actions;
}

/** Match the selector as it may have been quoted in source. */
function quoteRegex(selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return `["'\`]${escaped}["'\`]`;
}
