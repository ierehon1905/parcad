/**
 * The first part, built before the page was deployed.
 *
 * A visitor waits twice on a first visit: for 6 MB of kernel, and then for the
 * seed part to build in it — seconds, on geometry nobody has edited yet. The
 * site ships that one evaluation (`playground/prebuild.sh` records it from the
 * same kernel), so the part is on screen while the kernel is still arriving.
 *
 * It is offered exactly once, for the part it was recorded from and only while
 * its text is unedited: another part, a changed character, or a second
 * evaluation all go to the kernel in the tab.
 * And it is never the last word — `rebuild` runs the same graph there as soon
 * as the kernel is up, and what it measures replaces what was shipped. The page
 * says so while the stored build is what is on screen.
 */

import { call } from "./kernel";

declare const __PARCAD_FIRST_PART__: { url: string; part: string; script: string } | undefined;

const shipped = typeof __PARCAD_FIRST_PART__ === "undefined" ? undefined : __PARCAD_FIRST_PART__;
let offered = false;
let rebuilding: Promise<unknown> | undefined;

const matches = (part: string | undefined, source: string) =>
  !!shipped && part === shipped.part && source === shipped.script;

/** The shipped evaluation, if this is the part it was recorded from, unedited. */
export async function take(part: string | undefined, source: string): Promise<unknown | undefined> {
  if (!shipped || offered || !matches(part, source)) return undefined;
  offered = true;
  const response = await fetch(new URL(shipped.url, document.baseURI)).catch(() => undefined);
  if (!response?.ok) return undefined;
  const evaluated = (await response.json().catch(() => undefined)) as Record<string, unknown> | undefined;
  // Marked, because what the viewport draws has to say where it was measured.
  return evaluated && { ...evaluated, shipped: true };
}

/** Build that same graph in this tab, to replace what was shipped with it. */
export function rebuild(graph: unknown): Promise<unknown> {
  rebuilding = call({ op: "evaluate", graph }).then((reply) => {
    if (!("json" in reply)) throw new Error("the kernel answered the rebuild with bytes, not an evaluation");
    return reply.json;
  });
  return rebuilding;
}

/**
 * The rebuild already under way for this part, for a second request for it.
 *
 * The editor evaluates again as soon as it has mounted, which would otherwise
 * queue a second build of the same unedited script behind the first.
 */
export function inFlight(part: string | undefined, source: string): Promise<unknown> | undefined {
  return matches(part, source) ? rebuilding : undefined;
}
