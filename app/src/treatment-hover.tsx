/**
 * The editor's hover: what a name means, and what it did.
 *
 * Two questions meet at the same pointer. TypeScript answers the first — the
 * signature, the type, the doc comment `read_docs` serves and its worked
 * example — for every name in the document. The kernel answers the second, for
 * a treatment call only: the source says `.edges(">Z and |X").fillet(2)`, the
 * part says four edges are rounded, and only the second is the thing you
 * actually care about. One tooltip carries both, in that order, because that is
 * the order they are read in: what does this take, and what did it do here.
 *
 * Resolving costs a kernel round trip per node, so the tooltip opens
 * immediately with what the graph already knows and fills in the measured rows
 * when the worker answers. A stale answer is worse than a slow one, so the
 * caller's cache is keyed to the evaluated graph and dropped when it changes.
 *
 * It is anchored to the name under the pointer, not to the start of the call. A
 * treatment chain runs over several lines, and a card pinned to the top of it
 * opens nowhere near what the pointer is on.
 */

import { forEachDiagnostic } from "@codemirror/lint";
import { hoverTooltip, type EditorView, type Tooltip } from "@codemirror/view";
import type { Extension } from "@codemirror/state";
import { Fragment, render } from "preact";

import type { Info } from "./intellisense/analyzer";
import { Card, InfoCard, Rule } from "./intellisense/card";
import { hoverCardOpen } from "./intellisense/extensions";
import {
  treatmentActions,
  treatmentRows,
  treatmentTitle,
  type ResolvedTarget,
  type TreatmentNode,
} from "./treatment-info";

export interface TreatmentHoverSource {
  /**
   * The treatment whose authored call contains this offset, if any, and
   * whether the offset is on the chain's own method names rather than inside
   * one of their arguments.
   */
  treatmentAt(pos: number): { node: number; method?: string; onChain: boolean } | undefined;
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
  /**
   * What TypeScript knows at this offset, and the span it knows it about.
   *
   * Optional for the same reason: the language service starts on demand, and
   * until it has, a hover still says everything the graph knows.
   */
  info?(pos: number): Promise<(Info & { from: number; to: number }) | undefined>;
}

export function treatmentHover(source: TreatmentHoverSource): Extension {
  return hoverTooltip(
    async (view, pos): Promise<Tooltip | null> => {
      const treatment = source.treatmentAt(pos);
      const node = treatment && source.nodeAt(treatment.node);
      const range = treatment && source.callRange(treatment.node);
      const measured = !!(treatment && node && range);
      if (!measured && !source.info) return null;
      // A name the type checker has already complained about gets one tooltip,
      // not two. The complaint names the fix and the type under it is `any`,
      // which is the checker saying it gave up rather than anything to read.
      if (!measured && marked(view, pos)) return null;

      // Waited for rather than filled in later, which is the whole of the fix
      // for a card that appeared below the line and then jumped above it: the
      // signature and its prose are most of the height, so a card opened
      // without them is measured at the wrong size and placed on the wrong
      // side. CodeMirror drops the answer if the pointer has moved on.
      const info = await source.info?.(pos).catch(() => undefined);
      if (!measured && !info) return null;

      // The rows are about the treatment. They belong under a signature only
      // when the signature is the treatment's own — hovering `curve` inside
      // its selector asks about `EdgeQuery.curve`, and answering with what the
      // fillet rounded is answering a question nobody asked. When TypeScript
      // has nothing to say the card has no other subject, so they stay.
      const rows = measured && (treatment!.onChain || !info);
      if (!rows && !info) return null;

      // Anchored to the name itself — TypeScript's own span for it, or the
      // word under the pointer — so that a tooltip opened on a five-line call
      // chain still opens on the line the pointer is on.
      const word = view.state.wordAt(pos);

      return {
        pos: info?.from ?? word?.from ?? pos,
        end: info?.to ?? word?.to ?? pos,
        above: true,
        create: () => {
          hoverCardOpen(view, true);
          // CodeMirror positions a plain element and hands us the inside of it.
          // Preact renders into that element, so this tooltip is written the
          // same way as every other panel in the app rather than by hand.
          const dom = document.createElement("div");

          let target: ResolvedTarget | undefined;
          let pending = rows && !!source.resolve;

          const draw = () =>
            render(
              <HoverCard
                info={info}
                node={rows ? node : undefined}
                method={treatment?.method}
                target={target}
                pending={pending}
                view={view}
                // Recomputed from the live document: the call may have moved
                // since the tooltip opened, and an edit applied at a stale
                // offset would land in the middle of something else.
                range={treatment && source.callRange(treatment.node)}
              />,
              dom,
            );

          draw();
          if (rows) {
            source
              .resolve?.(treatment!.node)
              .then((answer) => {
                target = answer;
                pending = false;
                draw();
              })
              // A treatment that cannot be resolved has already reported why in
              // the error pane. Fall back to the rows that need no geometry.
              .catch(() => {
                pending = false;
                draw();
              });
          }

          return {
            dom,
            destroy: () => {
              hoverCardOpen(view, false);
              render(null, dom);
            },
          };
        },
      };
    },
    // Long enough not to fire while the pointer crosses the editor on its way
    // somewhere else.
    { hoverTime: 350 },
  );
}

/** Whether a diagnostic already covers this offset. */
function marked(view: EditorView, pos: number): boolean {
  let found = false;
  forEachDiagnostic(view.state, (_diagnostic, from, to) => {
    if (from <= pos && pos <= to) found = true;
  });
  return found;
}

/**
 * How the tooltip looks, and what it offers to write.
 *
 * The card is built here, so its appearance is here too — CodeMirror only
 * supplies the positioned shell around it. The one thing left in `style.css` is
 * what CodeMirror itself renders and names.
 */
function HoverCard({
  info,
  node,
  method,
  target,
  pending,
  view,
  range,
}: {
  info?: Info;
  node?: TreatmentNode;
  method?: string;
  target?: ResolvedTarget;
  pending: boolean;
  view: EditorView;
  range?: { from: number; to: number };
}) {
  const call = node && range ? view.state.doc.sliceString(range.from, range.to) : undefined;
  const actions =
    node && call !== undefined && range ? treatmentActions(node, call, range.from, target) : [];

  if (!info && !node) return null;

  return (
    <Card>
      {info && <InfoCard info={info} />}
      {node && (
        <>
          {/* The rule between what the language says and what this part did. */}
          {info && <Rule />}
          <div class={info ? "mt-2" : ""}>
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
        </>
      )}
    </Card>
  );
}

/** The data says ok or warn; only this file decides what colour that is. */
const TONE = { ok: "text-good", warn: "text-bad" } as const;
