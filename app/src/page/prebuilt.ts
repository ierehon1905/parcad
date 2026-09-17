/**
 * The first part, built before the page was deployed.
 *
 * A visitor waits twice on a first visit: for 6 MB of kernel, and then for the
 * seed part to build in it — seconds, on geometry nobody has edited yet. The
 * site ships that one evaluation (`web/prebuild.sh` records it from the
 * same kernel), so the part is on screen while the kernel is still arriving.
 *
 * It is offered exactly once, for the part it was recorded from and only while
 * its text is unedited: another part, a changed character, or a second
 * evaluation all go to the kernel in the tab.
 * And it is never the last word — `rebuild` runs the same graph there as soon
 * as the kernel is up, and what it measures replaces what was shipped. The page
 * says so while the stored build is what is on screen.
 */

import { DRACOLoader } from "three/examples/jsm/loaders/DRACOLoader.js";

import { evaluate } from "./host";

declare const __PARCAD_FIRST_PART__:
  | { url: string; mesh: string; decoder: string; part: string; script: string }
  | undefined;

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
  if (!evaluated) return undefined;
  const mesh = await triangles(shipped.mesh, shipped.decoder).catch(() => undefined);
  if (!mesh) return undefined;
  // Marked, because what the viewport draws has to say where it was measured.
  return { ...evaluated, ...mesh, shipped: true };
}

/**
 * The mesh, out of the Draco file beside the evaluation.
 *
 * Only the triangles travel that way, and only as far as the first rebuild:
 * Draco quantises positions to 14 bits of the part's own extent, 0.006 mm here,
 * where every number the page *reports* comes from the snapshot in the JSON.
 * web/encode-draco.ts writes it.
 */
async function triangles(url: string, decoderPath: string) {
  const response = await fetch(new URL(url, document.baseURI));
  if (!response.ok) throw new Error(`the shipped mesh is not there: ${response.status}`);
  const encoded = await response.arrayBuffer();
  const loader = new DRACOLoader().setDecoderPath(new URL(decoderPath, document.baseURI).href);
  const geometry = await new Promise<import("three").BufferGeometry>((resolve, reject) =>
    loader.parse(encoded, resolve, reject),
  );
  loader.dispose();
  return {
    positions: geometry.getAttribute("position").array as Float32Array,
    normals: geometry.getAttribute("normal").array as Float32Array,
    indices: geometry.getIndex()!.array as Uint32Array,
  };
}

/** Build that same graph in this tab, to replace what was shipped with it. */
export function rebuild(graph: unknown): Promise<unknown> {
  rebuilding = evaluate(graph);
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
