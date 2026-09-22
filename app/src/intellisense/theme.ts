/**
 * How the floating cards look, as a CodeMirror theme.
 *
 * These are the one place in this app where a stylesheet is the wrong tool.
 * Everything else CodeMirror renders — a decoration's span, the editor
 * surface — can be styled from `style.css`, because nothing else is styling
 * it. The tooltips are different: One Dark already gives them a background and
 * no border, and a rule in our stylesheet ties with the theme's on specificity
 * and then loses on source order, because CodeMirror injects its styles after
 * ours. Answering that with a third copy of the class name is a fight this
 * would keep having.
 *
 * `EditorView.theme` is the API CodeMirror provides for exactly this, and the
 * values are still the design tokens — read as the custom properties Tailwind
 * emits for `@theme`, so there is no second palette here, only a second way of
 * reaching the first.
 *
 * Every selector names its class twice. Being listed after `oneDark` is not
 * enough and neither is `Prec.lowest`: both themes generate a scoped rule of
 * the same specificity, and the one that wins is whichever CodeMirror happens
 * to mount later — measured, One Dark's `.ͼo .cm-tooltip` landed at rule 190
 * and this file's at 99, so only the properties One Dark does not set were
 * getting through. `.cm-tooltip.cm-tooltip` is a class more specific and
 * settles it wherever the rule ends up.
 *
 * The measurements are Monaco's, read off `hoverWidget.css` and `hover.css`:
 * 8px radius, a shadow that is spread with no offset, 1.5em lines. Two values
 * are deliberately not theirs — the shadow's alpha and the border — because
 * both are measured against a surface much darker than Monaco's and would
 * otherwise not be visible at all.
 */

import { EditorView } from "@codemirror/view";

/** The edge of something floating, which is brighter than a line between panels. */
const EDGE = "color-mix(in srgb, var(--color-ink) 15%, transparent)";
const SHADOW = "0 0 12px rgb(0 0 0 / 0.5)";

export const cardTheme = EditorView.theme({
  ".cm-tooltip.cm-tooltip": {
    background: "var(--color-panel-2)",
    border: `1px solid ${EDGE}`,
    borderRadius: "8px",
    boxShadow: SHADOW,
    lineHeight: "1.5em",
    color: "var(--color-ink)",
  },

  /**
   * The two that carry prose, and so the two CodeMirror may have to cut short:
   * it shrinks a card to the room `tooltipSpace` leaves, and the rest has to
   * scroll rather than vanish. `auto` is also what keeps the corners honest,
   * because a section rule inside a card is drawn past the padding.
   *
   * Never the completion list. CodeMirror hangs the details panel off it as a
   * child and then places that panel entirely outside the list's own box, so
   * clipping the list deletes the panel — see docs/GOTCHAS.md.
   */
  ".cm-tooltip-hover.cm-tooltip-hover, .cm-completionInfo.cm-completionInfo": {
    overflow: "auto",
    overscrollBehavior: "contain",
  },

  /**
   * How wide this app's own cards may be. `max-content` is load-bearing: a
   * floating box with an automatic width is as wide as the space beside it, so
   * it changes when CodeMirror moves the card, and the card opens at its
   * proper size and then squashes. The cap is the smaller of a comfortable
   * measure and the window — a card may cover the viewport beside it, it may
   * not fall off the end of it.
   *
   * The details panel is absent again: CodeMirror sizes that one to the room
   * beside the list, and a width of our own there is only a way to be trimmed.
   */
  ".cm-tooltip-hover.cm-tooltip-hover, .cm-tooltip-signature.cm-tooltip-signature":
    {
      width: "max-content",
      maxWidth: "min(720px, calc(100vw - 1rem))",
    },

  ".cm-completionInfo.cm-completionInfo": {
    background: "var(--color-panel-2)",
    border: `1px solid ${EDGE}`,
    borderRadius: "8px",
    boxShadow: SHADOW,
    padding: "0",
    margin: "0 0 0 4px",
  },

  ".cm-tooltip-autocomplete.cm-tooltip-autocomplete > ul": {
    fontFamily: "var(--font-mono)",
    fontSize: "12.5px",
    maxHeight: "22rem",
  },

  ".cm-tooltip-autocomplete.cm-tooltip-autocomplete > ul > li": {
    padding: "0 8px",
    lineHeight: "22px",
  },

  /** The accent's deep shade, which is this app's "selected" everywhere else. */
  ".cm-tooltip-autocomplete.cm-tooltip-autocomplete > ul > li[aria-selected]": {
    background: "var(--color-accent-deep)",
    color: "var(--color-ink)",
  },

  /** The one column a reader scans down, so it is the accent and not a grey. */
  ".cm-completionIcon.cm-completionIcon": {
    color: "var(--color-accent)",
    opacity: "1",
    width: "1rem",
    marginRight: "0.25rem",
  },

  /** What the typed prefix matched, so a fuzzy hit shows why it is in the list. */
  ".cm-completionMatchedText.cm-completionMatchedText": {
    color: "var(--color-accent)",
    textDecoration: "none",
    fontWeight: "bold",
  },
});
