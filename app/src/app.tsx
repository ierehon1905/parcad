/**
 * The shell: three panes, the keys, and the two things that watch the host.
 *
 * Everything below this is either a component reading a signal or one of the
 * two imperative islands. The machinery — the evaluation cycle, the live
 * session, saving and exporting — is in `engine.ts` and is not a render; this
 * file only starts it and lays out what observes it.
 */

import { useEffect } from "preact/hooks";

import * as engine from "./engine";
import { watchThumbnails } from "./thumbnails";
import * as S from "./state";
import { Editor } from "./ui/editor";
import { TabCount, Tabs } from "./ui/components/Tabs";
import { OpPalette } from "./ui/op-palette";
import { problemCount, problemsOf, Problems } from "./ui/problems";
import { Sidebar } from "./ui/sidebar";
import { Splitter } from "./ui/splitter";
import { Titlebar } from "./ui/titlebar";
import { TooltipLayer } from "./ui/tooltip";
import { UpdateBar } from "./ui/update-bar";
import { ViewportPane } from "./ui/viewport-pane";

export function App() {
  useEffect(() => engine.watchMcp(), []);
  useEffect(() => engine.watchAgentLink(), []);
  useEffect(() => engine.watchKernel(), []);
  useEffect(() => engine.watchUpdates(), []);
  useEffect(watchThumbnails, []);
  useEffect(keys, []);

  return (
    <div class="flex flex-col h-full">
      <Titlebar />
      <UpdateBar />
      <main class="flex flex-1 min-h-0">
        <Sidebar />
        {/* Hidden, never unmounted: CodeMirror owns the document and the
            evaluation cycle starts with it, so a window showing only the part
            is still building that part from the source it holds. */}
        <section
          id="editor-pane"
          hidden={!S.codeVisible.value}
          class="flex flex-col w-[42%] min-w-[280px] min-h-0 bg-panel [&[hidden]]:hidden"
        >
          <EditorTabs />
          <div hidden={S.editorTab.value !== "code"} class="flex flex-col flex-1 min-h-0 [&[hidden]]:hidden">
            {/* Pressing an operation writes its call into the source at the
                cursor — it does not open a command, and there is nowhere else
                for a feature to live. See ui/op-palette.tsx. */}
            <OpPalette />
            <Editor />
          </div>
          {S.editorTab.value === "problems" && <Problems />}
          <ErrorPane />
        </section>
        {S.codeVisible.value && <Splitter />}
        <ViewportPane />
      </main>
      <TooltipLayer />
    </div>
  );
}

/** Code, and what is wrong with the part it builds; the count is how a glance knows whether to look. */
function EditorTabs() {
  const snapshot = S.snapshot.value;
  const count = snapshot ? problemCount(problemsOf(snapshot)) : undefined;
  const measuring = S.measuring.value.length > 0;
  return (
    <Tabs
      tabs={[
        { key: "code", label: "Code" },
        {
          key: "problems",
          label: (
            <>
              Problems
              {count && <TabCount n={count.n} tone={count.tone} />}
              {measuring && <span class="text-ink-dim">…</span>}
            </>
          ),
        },
      ]}
      current={S.editorTab.value}
      onChoose={(key) => {
        S.editorTab.value = key;
        // CodeMirror measured nothing while it was hidden.
        if (key === "code") requestAnimationFrame(() => S.editorRef.current?.requestMeasure());
      }}
    />
  );
}

/**
 * Why the last build failed, under the editor rather than over the part.
 *
 * The geometry stays on screen behind it: a broken edit mid-typing should not
 * blank the viewport, and the message here is what says the part is a moment
 * out of date.
 */
function ErrorPane() {
  const text = S.errorText.value;
  return (
    <div
      id="error"
      hidden={!text}
      class="flex-none max-h-[34%] overflow-auto px-3 py-2.5 border-t border-line
             bg-bad-bg text-bad-ink font-mono text-tiny whitespace-pre-wrap"
    >
      {text}
    </div>
  );
}

/**
 * Keys.
 *
 * ⌘S saves the part. It used to export STL, from when there was nothing on disk
 * to save and the only file the app could produce was a mesh; now that a part is
 * a project the conventional meaning is the right one, and exports moved to ⌘E.
 * ⇧ picks the exact format: STEP needs the B-rep kernel whatever the viewport
 * happens to be showing, because a mesh cannot be turned back into exact
 * surfaces after the fact.
 *
 * ⌘\ shows and hides the source, which is the key every editor with a side
 * panel uses for the same thing.
 *
 * In the capture phase, and on the physical key rather than the character it
 * types: a browser's own ⌘S is only suppressed by preventDefault on the way
 * down, and under a non-Latin layout `e.key` for the S key is "ы", not "s".
 */
function keys() {
  const onKey = async (e: KeyboardEvent) => {
    if (!(e.metaKey || e.ctrlKey)) return;
    const key = physicalKey(e);

    // ⌘A pressed anywhere but a field has nothing to mean in a tool window:
    // the browser's answer is to highlight the titlebar and the palette. Send
    // it to the source instead, which is the only thing here made of text.
    if (key === "a" && !editable(e.target)) {
      e.preventDefault();
      const view = S.editorRef.current;
      if (!view) return;
      engine.showCode(true);
      view.focus();
      view.dispatch({ selection: { anchor: 0, head: view.state.doc.length } });
      return;
    }

    if (key === "\\") {
      e.preventDefault();
      engine.showCode(!S.codeVisible.peek());
      return;
    }
    if (key === "o") {
      e.preventDefault();
      // A second ⌘O with the search already taking keys goes back to the part.
      if (S.browserOpen.peek() && document.activeElement?.closest("#parts")) engine.showPartTree();
      else engine.showParts();
      return;
    }
    if (key === "s") {
      e.preventDefault();
      await engine.saveOpenPart();
      return;
    }
    if (key !== "e") return;

    e.preventDefault();
    await engine.runExport(e.shiftKey ? "step" : "stl");
  };

  window.addEventListener("keydown", onKey, true);
  return () => window.removeEventListener("keydown", onKey, true);
}

/** Somewhere a ⌘A of its own belongs: a field, or the editor, which keeps one. */
function editable(target: EventTarget | null): boolean {
  const el = target instanceof Element ? target : null;
  return !!el?.closest("input, textarea, [contenteditable=''], [contenteditable='true'], .cm-editor");
}

/** The key under the finger, named as a US layout would: "KeyS" -> "s", "Backslash" -> "\\". */
function physicalKey(e: KeyboardEvent): string {
  const code = e.code;
  if (code.length === 4 && code.startsWith("Key")) return code[3].toLowerCase();
  if (code === "Backslash") return "\\";
  return e.key.toLowerCase();
}
