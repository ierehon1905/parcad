/**
 * CodeMirror, behind a ref.
 *
 * An island, and it has to be one. CodeMirror keeps its own view of the
 * document, its own undo history and its own decorations, and re-rendering the
 * text from a virtual DOM would fight all three. So this component owns exactly
 * one empty `<div>`, hands it to CodeMirror once, and never touches what is
 * inside it again — the editor writes to the signals in `state.ts` and the rest
 * of the window reads those.
 *
 * The `#editor` id and the `window.__editor` / `window.__undo` handles are a
 * contract with the desktop end-to-end suite, which cannot deliver a Cmd-Z
 * keystroke to WKWebView and drives the undo command directly instead. What
 * that suite asserts is not key delivery but history: an agent's edit must land
 * in the same undo history the user's own typing goes into.
 */

import { EditorView, basicSetup } from "codemirror";
import { tooltips } from "@codemirror/view";
import { undo } from "@codemirror/commands";
import { javascript } from "@codemirror/lang-javascript";
import { oneDark } from "@codemirror/theme-one-dark";
import { useEffect, useRef } from "preact/hooks";

import { insertFlashField, treatmentHoverField } from "../editor-marks";
import * as engine from "../engine";
import { intellisense } from "../intellisense/extensions";
import { cardTheme } from "../intellisense/theme";
import * as languageService from "../intellisense/service";
import { numberDial } from "../number-dial";
import { selectorLinter } from "../selector-lint";
import * as S from "../state";
import { onTreatmentChain, treatmentAtCursor } from "../source-link";
import { treatmentHover, type TreatmentHoverSource } from "../treatment-hover";
import type { TreatmentNode } from "../treatment-info";

/**
 * What the hover tooltip is allowed to know.
 *
 * Declared apart from the editor so the editor's construction does not depend
 * on an extension that reads the editor back.
 */
const hoverSource: TreatmentHoverSource = {
  // Only for a document that still matches the evaluated graph. Hovering
  // half-typed source must not report the last part's counts.
  treatmentAt: (pos) => {
    const editor = S.editorRef.current;
    if (!editor) return undefined;
    const source = editor.state.doc.toString();
    if (source !== S.lastSource.value) return undefined;
    const treatment = treatmentAtCursor(editor.state, source, S.lastTreatments.value, pos);
    return (
      treatment && {
        node: treatment.node,
        method: treatment.source?.method,
        onChain: onTreatmentChain(editor.state, source, pos),
      }
    );
  },
  nodeAt: (node) => S.lastGraph.value?.nodes[node] as TreatmentNode | undefined,
  callRange: (node) =>
    engine.treatmentRange(S.lastTreatments.value.find((t) => t.node === node)),
  resolve: engine.resolveTarget,
  info: (pos) => languageService.quickInfo(pos),
};

export function Editor() {
  const host = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!host.current) return;

    const view = new EditorView({
      // Empty until the project folder answers. The document is loaded from
      // disk rather than compiled in, so there is nothing to show
      // synchronously.
      doc: "",
      parent: host.current,
      extensions: [
        basicSetup,
        javascript(),
        oneDark,
        // After One Dark on purpose: it styles the tooltips too, and the later
        // theme wins.
        cardTheme,
        treatmentHoverField,
        insertFlashField,
        // Selector syntax is marked as you type. Everything else — whether the
        // selector resolves, and to how many edges — waits for the evaluation,
        // because only the kernel knows.
        selectorLinter,
        // Click a number to dial it with the arrow keys. Above the default
        // keymap, and engaged by the pointer only, so keyboard navigation
        // keeps Up and Down.
        numberDial,
        // Where a card may go. CodeMirror already flips a tooltip to the other
        // side when it does not fit and shrinks it to the room that is left —
        // it just assumes the whole window is the editor, and here the top of
        // it is the titlebar and the operation palette, which paint over a
        // card that runs up into them. Measured from the scroller, so the rows
        // are the bound: the error pane opening takes the space with it.
        tooltips({
          // Out of the editor and onto the document, so that nothing between
          // the two can trim a card: a hover that reaches over the viewport
          // was being cut at the editor pane's edge. CodeMirror's own remedy
          // for exactly this, and the reason `style.css` matches `.cm-tooltip`
          // twice rather than under `.cm-editor` — a tooltip is no longer
          // inside one.
          parent: document.body,
          tooltipSpace: (editor) => {
            const rows = editor.scrollDOM.getBoundingClientRect();
            // Wide horizontally on purpose: the editor pane is narrow, and a
            // signature broken over six lines to stay inside it is worse than
            // one that reaches over the viewport.
            return {
              top: rows.top,
              bottom: rows.bottom,
              left: 0,
              right: document.documentElement.clientWidth,
            };
          },
        }),
        treatmentHover(hoverSource),
        // Completion, signature help and type diagnostics, from a language
        // service that compiles `dsl.ts` itself. After the hover above, whose
        // tooltip it renders into rather than beside.
        intellisense(),
        EditorView.domEventHandlers({ blur: () => engine.saveOnBlur() }),
        EditorView.updateListener.of((v) => {
          if (v.docChanged) {
            // The titlebar's saved/unsaved mark is a fact about this keystroke,
            // so it cannot wait for the debounced evaluation.
            S.docSource.value = v.state.doc.toString();
            engine.forgetTreatmentPreview();
            engine.schedule();
          }
          if (v.selectionSet) void engine.previewTreatmentAtCursor();
        }),
      ],
    });

    S.editorRef.current = view;
    // Handles for the desktop end-to-end driver and for poking during
    // development, alongside `__viewport`.
    const globals = window as unknown as Record<string, unknown>;
    globals.__editor = view;
    globals.__undo = () => undo(view);

    void engine.start();
    engine.subscribeSession();
    // The compiler costs a few hundred milliseconds to start and nothing until
    // it is asked something, so it waits for the window to be done with the
    // first part rather than competing with it.
    const warm = requestIdleCallback(() => languageService.prewarm(), { timeout: 4000 });

    return () => {
      cancelIdleCallback(warm);
      view.destroy();
      S.editorRef.current = undefined;
    };
  }, []);

  return <div id="editor" ref={host} class="flex-1 min-h-0 overflow-hidden" />;
}
