/**
 * Hover a treatment call to see what it currently resolves to, and fix it.
 *
 * The source says `.edges(">Z and |X").fillet(2)`; the part says four edges are
 * rounded. Only the second is the thing you actually care about, and until now
 * it took a viewport hover to find out. This tooltip closes that gap in the
 * direction people read: from the code to the geometry.
 *
 * Resolving costs a kernel round trip per node, so the tooltip opens
 * immediately with what the graph already knows and fills in the measured rows
 * when the worker answers. A stale answer is worse than a slow one, so the
 * caller's cache is keyed to the evaluated graph and dropped when it changes.
 */

import { hoverTooltip, type EditorView, type Tooltip } from "@codemirror/view";
import type { Extension } from "@codemirror/state";
import { Fragment, render } from "preact";

import {
  treatmentActions,
  treatmentRows,
  treatmentTitle,
  type ResolvedTarget,
  type TreatmentNode,
} from "./treatment-info";

export interface TreatmentHoverSource {
  /** The treatment whose authored call contains this offset, if any. */
  treatmentAt(pos: number): { node: number; method?: string } | undefined;
  /** The intent-graph node for a treatment, when the document is still current. */
  nodeAt(node: number): TreatmentNode | undefined;
  /** The authored call's document range. */
  callRange(node: number): { from: number; to: number } | undefined;
  /**
   * Ask the kernel for the exact target.
   *
   * Optional so the tooltip has a defined answer when nothing can measure —
   * it then shows only what the graph states and offers no edits, rather than
   * claiming a count it cannot check.
   */
  resolve?(node: number): Promise<ResolvedTarget>;
}

export function treatmentHover(source: TreatmentHoverSource): Extension {
  return hoverTooltip(
    (view, pos): Tooltip | null => {
      const treatment = source.treatmentAt(pos);
      const node = treatment && source.nodeAt(treatment.node);
      const range = treatment && source.callRange(treatment.node);
      if (!treatment || !node || !range) return null;

      return {
        pos: range.from,
        end: range.to,
        above: true,
        create: () => {
          // CodeMirror positions a plain element and hands us the inside of it.
          // Preact renders into that element, so this tooltip is written the
          // same way as every other panel in the app rather than by hand.
          const dom = document.createElement("div");

          const draw = (target?: ResolvedTarget, pending = false) =>
            render(
              <TreatmentCard
                node={node}
                method={treatment.method}
                target={target}
                pending={pending}
                view={view}
                // Recomputed from the live document: the call may have moved
                // since the tooltip opened, and an edit applied at a stale
                // offset would land in the middle of something else.
                range={source.callRange(treatment.node)}
              />,
              dom,
            );

          draw(undefined, !!source.resolve);
          source
            .resolve?.(treatment.node)
            .then((target) => draw(target))
            // A treatment that cannot be resolved has already reported why in
            // the error pane. Fall back to the rows that need no geometry.
            .catch(() => draw(undefined));

          return { dom, destroy: () => render(null, dom) };
        },
      };
    },
    // Long enough not to fire while the pointer crosses the editor on its way
    // somewhere else.
    { hoverTime: 350 },
  );
}

/**
 * How the tooltip looks, and what it offers to write.
 *
 * The card is built here, so its appearance is here too — CodeMirror only
 * supplies the positioned shell around it. The one thing left in `style.css` is
 * what CodeMirror itself renders and names.
 */
function TreatmentCard({
  node,
  method,
  target,
  pending,
  view,
  range,
}: {
  node: TreatmentNode;
  method?: string;
  target?: ResolvedTarget;
  pending: boolean;
  view: EditorView;
  range?: { from: number; to: number };
}) {
  const call = range ? view.state.doc.sliceString(range.from, range.to) : undefined;
  const actions =
    call !== undefined && range ? treatmentActions(node, call, range.from, target) : [];

  return (
    <div class="bg-panel-2 text-ink font-mono text-small leading-normal px-2.5 py-2 max-w-[42ch]">
      <div class="text-accent mb-1">{treatmentTitle(node, method)}</div>
      {/* Short labels, long values: give the value column the slack. */}
      <dl class="grid m-0 gap-x-2.5 gap-y-px grid-cols-[max-content_1fr]">
        {treatmentRows(node, target, pending).map((row) => (
          <Fragment key={row.label}>
            <dt class="text-ink-dim">{row.label}</dt>
            {/* The data says ok or warn; only this file decides the colour. */}
            <dd class={`m-0 [overflow-wrap:anywhere] ${row.tone ? TONE[row.tone] : ""}`}>
              {row.value}
            </dd>
          </Fragment>
        ))}
      </dl>
      {actions.map((action) => (
        <button
          key={action.label}
          class="block w-full mt-1.5 px-1.5 py-1 text-left text-ink bg-panel border border-line
                 rounded-xs cursor-pointer hover:border-accent hover:text-accent"
          title={action.detail}
          onClick={() => {
            view.dispatch({ changes: action.edit });
            view.focus();
          }}
        >
          {action.label}
        </button>
      ))}
    </div>
  );
}

/** The data says ok or warn; only this file decides what colour that is. */
const TONE = { ok: "text-good", warn: "text-bad" } as const;
