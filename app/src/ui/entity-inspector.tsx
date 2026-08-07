/**
 * What is under the pointer, and the selector that would name it in a script.
 *
 * The suggested selector is the useful half. A viewport `edge@7` is valid for
 * exactly one evaluation and is never a durable script reference; a directional
 * conjunction is re-resolved after every edit, which is what lets a fillet
 * survive a change that renumbers the topology. So the panel offers the second
 * and copies the second, and says plainly when no conjunction picks the entity
 * out — refusing beats handing over an ambiguous one.
 */

import { computed } from "@preact/signals";

import { directionLabel, fmt, suggestEdgeSelector, treatmentForEdge } from "../engine";
import { suggestVertexSelector } from "../entities";
import * as S from "../state";
import { Icon } from "./icons";
import { tip } from "./tooltip";

/**
 * Hover wins over selection, and which of the two it is gets said out loud.
 *
 * A click pins the entity and the panel then stays after the pointer leaves;
 * without the label a pinned panel is indistinguishable from a hover that has
 * not caught up yet.
 */
const shown = computed(() => {
  const vertex = S.hoveredVertex.value ?? S.selectedVertex.value;
  if (vertex) return { kind: "vertex" as const, vertex, hovering: !!S.hoveredVertex.value };
  const edge = S.hoveredEdge.value ?? S.selectedEdge.value;
  if (edge) return { kind: "edge" as const, edge, hovering: !!S.hoveredEdge.value };
  return undefined;
});

export function EntityInspector() {
  const target = shown.value;
  if (!target) return null;

  const vertex = target.kind === "vertex" ? target.vertex : undefined;
  const edge = target.kind === "edge" ? target.edge : undefined;

  const selector = vertex
    ? suggestVertexSelector(vertex, S.visibleVertices.value)
    : suggestEdgeSelector(edge!, S.visibleEdges.value);
  const treatment = edge && treatmentForEdge(edge);
  const call = vertex ? ".vertices" : ".edges";

  return (
    <div
      id="edge-inspector"
      class="min-w-[184px] px-2.5 py-2 rounded-lg border border-line
             bg-glass/86 backdrop-blur-lg text-ink-dim font-mono text-tiny"
    >
      <div class="flex items-center gap-1.5">
        <Icon name={vertex ? "vertices" : "edges"} class="size-4 shrink-0 text-ink-dim" />
        <span class="font-sans text-[10px] leading-[1.3] tracking-[0.08em] uppercase text-ink-dim">
          {target.kind} inspector{target.hovering ? "" : " · selected"}
        </span>
      </div>

      <div class="mt-1 text-ink">{vertex ? vertex.id : edge!.id}</div>
      <div>
        {vertex
          ? `(${vertex.point.map(fmt).join(", ")}) mm · ${vertex.degree} incident edge${vertex.degree === 1 ? "" : "s"}`
          : `${fmt(edge!.length_mm)} mm${edge!.direction ? ` · ${directionLabel(edge!.direction)}` : ""}`}
      </div>

      {treatment && (
        <div id="edge-origin" class="mt-[5px] text-gold">
          from .{treatment.source?.method ?? treatment.kind}(…) · click to lock code
        </div>
      )}

      <div class="mt-[5px] text-accent">
        {selector
          ? `${vertex ? "vertices" : "selector"}: ${selector}`
          : `no unique directional ${vertex ? "vertex " : ""}selector`}
      </div>

      <button
        type="button"
        disabled={!selector}
        class="flex items-center gap-1.5 mt-[7px] px-1.5 py-0.5 bg-panel-2 text-ink
               border border-line rounded cursor-pointer hover:border-accent
               disabled:cursor-default disabled:text-ink-dim disabled:opacity-65
               disabled:hover:border-line"
        {...tip({
          title: `Copy ${target.kind === "vertex" ? "a vertex" : "an edge"} selector`,
          code: selector ? `${call}("${selector}")` : undefined,
          text: selector
            ? `A selector is re-resolved on every evaluation, so it survives an edit that renumbers the topology. The viewport's own ${vertex ? "vertex" : "edge@…"} ID never does.`
            : vertex
              ? "No directional condition identifies this corner on its own. Change the model, or reach for a different corner."
              : "No directional conjunction picks this edge out on its own. Add a stronger condition, or select a group and let the treatment take all of them.",
        })}
        onClick={() => void copySelector(selector)}
      >
        <Icon name="copy" class="size-3.5 shrink-0" />
        <span>copy {vertex ? "vertex " : ""}selector</span>
      </button>
    </div>
  );
}

async function copySelector(selector: string | undefined) {
  if (!selector || !navigator.clipboard) return;
  try {
    await navigator.clipboard.writeText(selector);
    S.setStatus(`copied ${selector}`);
  } catch {
    S.setStatus("could not copy selector", "failed");
  }
}
