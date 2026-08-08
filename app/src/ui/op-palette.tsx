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
 * The catalogue it draws is `ops.ts`; what pressing one writes is `snippet.ts`.
 */

import { useSignal } from "@preact/signals";
import { useEffect, useRef } from "preact/hooks";

import { type Op, type OpGroup, OP_GROUPS } from "../ops";
import { insertSnippet } from "../snippet";
import { Icon } from "./icons";
import { tip } from "./tooltip";

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
    const resize = () => (open.value = undefined);
    document.addEventListener("pointerdown", down, true);
    document.addEventListener("keydown", escape);
    window.addEventListener("resize", resize);
    return () => {
      document.removeEventListener("pointerdown", down, true);
      document.removeEventListener("keydown", escape);
      window.removeEventListener("resize", resize);
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
