/**
 * One tooltip, for every control in the chrome.
 *
 * This exists because `title=""` cannot do the job this app needs doing. A
 * native tooltip waits about a second, cannot be styled, collapses newlines
 * differently per platform, and — the part that actually matters here — renders
 * a monospace signature in the same proportional face as the prose around it.
 * Half the useful content in this UI *is* a signature: `fillet(radius,
 * selector)` is the thing a person is trying to read off the palette, and it
 * has to look like code.
 *
 * Delegated rather than a component per control, driven by data attributes:
 *
 *   data-tip        the sentence — what this control does
 *   data-tip-title  an optional heading above it, usually the operation's name
 *   data-tip-code   a signature or snippet, set in the mono face
 *   data-tip-key    the keyboard shortcut, if it has one
 *
 * Attributes rather than a `<Tooltip>` wrapper because a wrapper would put an
 * element between a flex container and its child in a dozen places, and because
 * the useful ones are *measurements* — the agent chip's tooltip is four seconds
 * old — which means they change on a timer rather than on a render. One
 * listener on the document beats a subscription per control.
 */

import { signal } from "@preact/signals";
import { useEffect } from "preact/hooks";

import { Glass } from "./components/Glass";

/** Long enough not to fire while the pointer crosses a toolbar. */
const DELAY_MS = 300;
/** Once one tip is up, moving to a neighbour should not wait again. */
const CHAIN_MS = 90;
/** Clear of the control, and of the pointer. */
const GAP = 8;

/**
 * What counts as a tipped element.
 *
 * `data-tip-title` is in here so a control whose tooltip is only a name and a
 * signature — most of the operation palette — is still found.
 */
const SELECTOR = "[data-tip],[data-tip-title]";

interface Tip {
  title?: string;
  code?: string;
  key?: string;
  text?: string;
  at: { top: number; left: number; bottom: number; width: number };
}

const shown = signal<Tip | undefined>(undefined);

/**
 * The panel.
 *
 * Rendered once at the root and positioned from the signal, rather than being
 * mounted next to whatever is hovered: a tooltip anchored inside the toolbar
 * would be clipped by the toolbar's own horizontal scroll.
 */
export function TooltipLayer() {
  useEffect(delegate, []);

  const tip = shown.value;
  if (!tip) return null;

  return (
    <Glass
      variant="tip"
      layout="fixed z-50 max-w-[34ch] px-2.5 py-2 pointer-events-none
              text-ink-dim text-tiny leading-[1.5]"
      ref={(element) => {
        if (element) place(element, tip);
      }}
    >
      {(tip.title || tip.key) && (
        <div class="flex items-baseline gap-2.5 text-ink font-medium">
          <span>{tip.title ?? ""}</span>
          {tip.key && <span class="ml-auto font-mono text-ink-dim text-[10.5px]">{tip.key}</span>}
        </div>
      )}
      {tip.code && (
        <div class={`font-mono text-accent [overflow-wrap:anywhere] ${tip.title ? "mt-1" : ""}`}>
          {tip.code}
        </div>
      )}
      {/* Newlines are paragraph breaks, so a longer explanation can have shape. */}
      {tip.text && (
        <div class={tip.title || tip.code ? "mt-1 whitespace-pre-line" : "whitespace-pre-line"}>
          {tip.text}
        </div>
      )}
    </Glass>
  );
}

/**
 * Below the control, unless there is no room — then above.
 *
 * Clamped to the window on both axes rather than flipped horizontally: a tip on
 * the last button of a toolbar should stay put and shift left, not jump to the
 * other side of the control it is describing.
 */
function place(element: HTMLElement, tip: Tip) {
  const box = element.getBoundingClientRect();
  const below = tip.at.bottom + GAP;
  const above = tip.at.top - GAP - box.height;
  const top = below + box.height <= window.innerHeight - 4 || above < 4 ? below : above;

  const wanted = tip.at.left + tip.at.width / 2 - box.width / 2;
  const left = Math.min(Math.max(wanted, 6), window.innerWidth - box.width - 6);

  element.style.top = `${Math.round(top)}px`;
  element.style.left = `${Math.round(left)}px`;
}

/** Watch the document for tipped elements. Returns its own teardown. */
function delegate() {
  let timer: number | undefined;
  let current: HTMLElement | undefined;
  /** When the last tip was hidden, so a hop to a neighbour skips the wait. */
  let last = 0;

  const hide = () => {
    window.clearTimeout(timer);
    current = undefined;
    if (shown.value) {
      shown.value = undefined;
      last = performance.now();
    }
  };

  const over = (event: PointerEvent) => {
    const target = (event.target as Element | null)?.closest?.(SELECTOR) as HTMLElement | null;
    if (!target || target === current) return;
    hide();
    current = target;
    timer = window.setTimeout(
      () => {
        if (current !== target || !target.isConnected) return;
        const box = target.getBoundingClientRect();
        shown.value = {
          title: target.dataset.tipTitle,
          code: target.dataset.tipCode,
          key: target.dataset.tipKey,
          text: target.dataset.tip,
          at: { top: box.top, left: box.left, bottom: box.bottom, width: box.width },
        };
        last = performance.now();
      },
      performance.now() - last < CHAIN_MS ? 0 : DELAY_MS,
    );
  };

  const out = (event: PointerEvent) => {
    // Only when the pointer has left the tipped element entirely; moving
    // between an icon and the label beside it stays inside it.
    const to = (event.relatedTarget as Element | null)?.closest?.(SELECTOR);
    if (to !== current) hide();
  };

  const escape = (event: KeyboardEvent) => {
    if (event.key === "Escape") hide();
  };

  document.addEventListener("pointerover", over);
  document.addEventListener("pointerout", out);
  // A tooltip that outlives the thing it describes is a lie about the screen.
  // Clicking, scrolling and leaving the window all end it.
  document.addEventListener("pointerdown", hide, true);
  document.addEventListener("scroll", hide, true);
  document.addEventListener("keydown", escape);
  window.addEventListener("blur", hide);

  return () => {
    hide();
    document.removeEventListener("pointerover", over);
    document.removeEventListener("pointerout", out);
    document.removeEventListener("pointerdown", hide, true);
    document.removeEventListener("scroll", hide, true);
    document.removeEventListener("keydown", escape);
    window.removeEventListener("blur", hide);
  };
}

/** The attributes that give an element a tooltip, spread onto it. */
export function tip(content: { title?: string; code?: string; key?: string; text?: string }) {
  return {
    "data-tip-title": content.title,
    "data-tip-code": content.code,
    "data-tip-key": content.key,
    "data-tip": content.text,
  };
}
