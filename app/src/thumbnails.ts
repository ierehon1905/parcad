/**
 * Draw every part's picker thumbnail that is missing or stale, in the background.
 *
 * The host says which are stale — each `preview.png` carries the digest of the
 * script it shows — and this window draws them with its own scene, one part at
 * a time and only while nobody is typing or building. That is the whole cost
 * bound: a folder of a thousand stale parts is a thousand idle builds in a
 * row, never two at once. A part that fails to build or draw is not retried
 * until its script changes.
 */

import { effect, signal } from "@preact/signals";
import * as backend from "./backend";
import type { ProjectPart } from "./backend";
import { boundsOf, geometryOf, quietForMs } from "./engine";
import * as S from "./state";
import type { Evaluated } from "./state";
import { drawThumbnail, THUMBNAIL_LOOK } from "./viewport";

/** Quiet this long before a draw starts: a pause in typing, not the end of it. */
const IDLE_MS = 1500;

/** Digests drawn since the listing was read, by path, so a card can show one before the next listing. */
export const drawn = signal<ReadonlyMap<string, string>>(new Map());

/** `path@digest` for the parts that did not build or draw, which wait for an edit. */
const failed = new Set<string>();
let drawing = false;
let timer: number | undefined;

/** What the card's picture on disk is of, to tell when to fetch it again; undefined when there is none. */
export function pictureKey(part: ProjectPart): string | undefined {
  const fresh = drawn.value.get(part.path);
  if (fresh) return `${fresh}/${THUMBNAIL_LOOK}`;
  return part.thumbnail ? `${part.thumbnail.source}/${part.thumbnail.look}` : undefined;
}

function isStale(part: ProjectPart): boolean {
  if (!part.bundle || failed.has(`${part.path}@${part.source}`)) return false;
  if (drawn.peek().get(part.path) === part.source) return false;
  return part.thumbnail?.source !== part.source || part.thumbnail?.look !== THUMBNAIL_LOOK;
}

/** The open part first: it is the one just edited, and the one the picker opens on. */
function nextStale(): ProjectPart | undefined {
  const parts = S.projects.peek()?.parts ?? [];
  const open = parts.find((part) => part.path === S.openPath.peek());
  return open && isStale(open) ? open : parts.find(isStale);
}

function later(ms: number) {
  window.clearTimeout(timer);
  timer = window.setTimeout(step, ms);
}

async function step() {
  if (drawing) return;
  const part = nextStale();
  if (!part) return;
  const quiet = quietForMs();
  if (quiet < IDLE_MS) return later(IDLE_MS - quiet);

  drawing = true;
  try {
    const built = await backend.buildProject<Evaluated>(part.path);
    const png = drawThumbnail(geometryOf(built), boundsOf(built.snapshot));
    if (!png) throw new Error("the thumbnail drew nothing");
    // Refused if part.js changed since the listing: the next listing has it stale again.
    await backend.saveProjectPreview(part.path, png, part.source, THUMBNAIL_LOOK);
    drawn.value = new Map(drawn.peek()).set(part.path, part.source);
  } catch (e) {
    console.warn(`parcad: no thumbnail for ${part.path}`, e);
    failed.add(`${part.path}@${part.source}`);
  } finally {
    drawing = false;
  }
  later(0);
}

/** Start drawing whenever the listing changes; returns the stop. */
export function watchThumbnails(): () => void {
  const stop = effect(() => {
    S.projects.value;
    drawn.value = new Map();
    later(IDLE_MS);
  });
  return () => {
    stop();
    window.clearTimeout(timer);
  };
}
