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
import * as S from "./state";
import { Editor } from "./ui/editor";
import { OpPalette } from "./ui/op-palette";
import { ProjectBrowser } from "./ui/project-browser";
import { Splitter } from "./ui/splitter";
import { Titlebar } from "./ui/titlebar";
import { TooltipLayer } from "./ui/tooltip";
import { ViewportPane } from "./ui/viewport-pane";

export function App() {
  useEffect(() => engine.watchMcp(), []);
  useEffect(keys, []);

  return (
    <div class="flex flex-col h-full">
      <Titlebar />
      <main class="flex flex-1 min-h-0">
        <section
          id="editor-pane"
          class="flex flex-col w-[42%] min-w-[280px] min-h-0 bg-panel"
        >
          {/* Pressing an operation writes its call into the source at the
              cursor — it does not open a command, and there is nowhere else for
              a feature to live. See ui/op-palette.tsx. */}
          <OpPalette />
          <Editor />
          <ErrorPane />
        </section>
        <Splitter />
        <ViewportPane />
      </main>
      <ProjectBrowser />
      <TooltipLayer />
    </div>
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
 */
function keys() {
  const onKey = async (e: KeyboardEvent) => {
    if (!(e.metaKey || e.ctrlKey)) return;
    const key = e.key.toLowerCase();

    if (key === "o") {
      e.preventDefault();
      S.browserOpen.value = true;
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

  window.addEventListener("keydown", onKey);
  return () => window.removeEventListener("keydown", onKey);
}
