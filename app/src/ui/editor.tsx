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
import { undo } from "@codemirror/commands";
import { javascript } from "@codemirror/lang-javascript";
import { oneDark } from "@codemirror/theme-one-dark";
import { useEffect, useRef } from "preact/hooks";

import { insertFlashField, treatmentHoverField } from "../editor-marks";
import * as engine from "../engine";
import { selectorLinter } from "../selector-lint";
import * as S from "../state";
import { treatmentAtCursor } from "../source-link";
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
    return treatment && { node: treatment.node, method: treatment.source?.method };
  },
  nodeAt: (node) => S.lastGraph.value?.nodes[node] as TreatmentNode | undefined,
  callRange: (node) =>
    engine.treatmentRange(S.lastTreatments.value.find((t) => t.node === node)),
  resolve: engine.resolveTarget,
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
        treatmentHoverField,
        insertFlashField,
        // Selector syntax is marked as you type. Everything else — whether the
        // selector resolves, and to how many edges — waits for the evaluation,
        // because only the kernel knows.
        selectorLinter,
        treatmentHover(hoverSource),
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

    return () => {
      view.destroy();
      S.editorRef.current = undefined;
    };
  }, []);

  return <div id="editor" ref={host} class="flex-1 min-h-0 overflow-hidden" />;
}
