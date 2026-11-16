/**
 * Which line made this face, and what this line made: the viewport and the
 * editor pointing at each other.
 *
 * - ⌘ held over the part: the face's feature is lit and the editor follows to
 *   the line that made it; let go without clicking and it scrolls back.
 * - ⌘-click: the face is pinned, the caret goes to that line, and its story is
 *   numbered in the gutter; ⌘-click the same feature again for the next step.
 *   Esc lets go.
 * - The pointer over a line of code: what that line made is lit, and what it
 *   only shaped faintly.
 *
 * Nothing floats over the part: the answer is the line itself. Machinery, like
 * `engine.ts`; the marks are `made-by-marks.ts`, the drawing `Viewport.setLitFaces`.
 */

import { computed, effect } from "@preact/signals";
import { EditorView } from "@codemirror/view";

import { flashInsert } from "./editor-marks";
import { showCode } from "./engine";
import { setMadeBy, type MadeByMarks } from "./made-by-marks";
import { Provenance } from "./provenance";
import * as S from "./state";

/** The last build's answer; none for a kernel that reports no `made_by`. */
const provenance = computed(() => {
  const faces = S.lastFaces.value;
  if (!faces?.some((face) => face.made_by?.length)) return undefined;
  return new Provenance(S.lastNodeLines.value, faces.map((face) => face.made_by ?? []));
});

/** Whether the editor still holds the source that was built: lines name lines of that build. */
function fresh(): boolean {
  const view = S.editorRef.current;
  return !!view && view.state.doc.length === S.lastSource.value.length && view.state.doc.toString() === S.lastSource.value;
}

let command = false;
let pointed: number | undefined;
let pinned: { face: number; level: number } | undefined;
let hoveredLine: number | undefined;
/** Where the editor was when ⌘ went down, to go back to if nothing was clicked. */
let followFrom: number | undefined;
let followed: number | undefined;
let committed = false;

/** The face under the pointer, from the viewport, and whether ⌘ is held with it. */
export function point(face: number | undefined, held: boolean) {
  pointed = face;
  hold(held);
  refresh();
}

/** ⌘ (Ctrl elsewhere) went down or up, wherever the pointer is. */
export function hold(held: boolean) {
  if (held === command) return;
  command = held;
  const view = S.editorRef.current;
  if (held) {
    followFrom = view?.scrollDOM.scrollTop;
    pointed ??= S.viewportRef.current?.pointed();
  } else {
    if (!committed && followed !== undefined && followFrom !== undefined) view?.scrollDOM.scrollTo({ top: followFrom, behavior: "smooth" });
    followFrom = undefined;
    followed = undefined;
    committed = false;
  }
  refresh();
}

/** A ⌘-click on the part: pin the face, or step up its story if it is the same feature. */
export function commandClick(face: number | undefined): boolean {
  const p = provenance.value;
  if (face === undefined || !p || !fresh()) return false;
  const rows = p.rowsOf(face);
  if (!rows.length) return false;
  const same = pinned && p.featureFaces(pinned.face).has(face);
  pinned = { face, level: same ? (pinned!.level + 1) % rows.length : 0 };
  committed = true;
  goTo(rows[pinned.level].line);
  refresh();
  return true;
}

/** The pointer over a line of the source, or off it. */
export function hoverLine(line: number | undefined) {
  if (line === hoveredLine) return;
  hoveredLine = line;
  refresh();
}

/** The source changed: its lines are no longer the build's, and nothing is pointed at in it until the pointer moves. */
export function edited() {
  hoveredLine = undefined;
  refresh();
}

/** Let go of the pinned face. */
export function release() {
  if (!pinned) return;
  pinned = undefined;
  refresh();
}

/** The caret to a line, the line in the middle of the editor, flashed so the eye finds it. */
function goTo(line: number) {
  const view = S.editorRef.current;
  if (!view || line > view.state.doc.lines) return;
  const { from, to } = view.state.doc.line(line);
  showCode(true);
  S.editorTab.value = "code";
  view.dispatch({
    selection: { anchor: from },
    effects: [EditorView.scrollIntoView(from, { y: "center" }), flashInsert.of({ from, to })],
  });
  view.focus();
}

/** Scroll the editor so a line sits in the middle, without moving the caret. */
function follow(line: number) {
  const view = S.editorRef.current;
  if (!view || line > view.state.doc.lines) return;
  const block = view.lineBlockAt(view.state.doc.line(line).from);
  const top = block.top + view.documentTop - view.scrollDOM.getBoundingClientRect().top + view.scrollDOM.scrollTop;
  view.scrollDOM.scrollTo({ top: top + block.height / 2 - view.scrollDOM.clientHeight / 2, behavior: "smooth" });
}

let shown: { marks: MadeByMarks; made: number[]; shaped: number[] } | undefined;

/** Light what the state says, in both panes, and nothing else. */
function refresh() {
  const p = provenance.value;
  let made = new Set<number>();
  let shaped = new Set<number>();
  const marks: MadeByMarks = { steps: [], current: 0 };
  if (p && fresh()) {
    // Over the pinned feature itself, ⌘ is about to step its story, so the story stays on show.
    const onPinned = pinned !== undefined && pointed !== undefined && p.featureFaces(pinned.face).has(pointed);
    if (command && pointed !== undefined && !onPinned) {
      const first = p.rowsOf(pointed)[0];
      if (first) {
        made = p.featureFaces(pointed);
        marks.lit = first.line;
        if (first.line !== followed) {
          followed = first.line;
          follow(first.line);
        }
      }
    } else if (hoveredLine !== undefined) {
      ({ made, shaped } = p.facesOfLine(hoveredLine));
      if (made.size || shaped.size) marks.lit = hoveredLine;
    } else if (pinned) {
      const rows = p.rowsOf(pinned.face);
      const level = Math.min(pinned.level, rows.length - 1);
      made = level === 0 ? p.featureFaces(pinned.face) : p.facesOfRow(rows[level]);
      marks.lit = rows[level]?.line;
    }
    if (pinned) {
      marks.steps = p.rowsOf(pinned.face).map((row) => row.line);
      marks.current = Math.min(pinned.level, marks.steps.length - 1);
    }
  }
  const next = { marks, made: [...made].sort((a, b) => a - b), shaped: [...shaped].sort((a, b) => a - b) };
  if (shown && same(shown, next)) return;
  shown = next;
  S.viewportRef.current?.setLitFaces(next.made, next.shaped);
  S.editorRef.current?.dispatch({ effects: setMadeBy.of(marks) });
}

function same(a: NonNullable<typeof shown>, b: NonNullable<typeof shown>) {
  return (
    a.marks.lit === b.marks.lit &&
    a.marks.current === b.marks.current &&
    a.marks.steps.join() === b.marks.steps.join() &&
    a.made.join() === b.made.join() &&
    a.shaped.join() === b.shaped.join()
  );
}

/**
 * Start listening: ⌘ anywhere in the window, Esc, and every new build, whose
 * faces are numbered afresh. Returns the way to stop.
 */
export function watch(): () => void {
  const key = (event: KeyboardEvent) => {
    if (event.key === "Meta" || event.key === "Control") hold(event.type === "keydown");
    else if (event.type === "keydown" && event.key === "Escape") release();
  };
  const blur = () => hold(false);
  window.addEventListener("keydown", key);
  window.addEventListener("keyup", key);
  window.addEventListener("blur", blur);
  const rebuilt = effect(() => {
    void S.lastFaces.value;
    pinned = undefined;
    pointed = undefined;
    shown = undefined;
    refresh();
  });
  return () => {
    window.removeEventListener("keydown", key);
    window.removeEventListener("keyup", key);
    window.removeEventListener("blur", blur);
    rebuilt();
  };
}
