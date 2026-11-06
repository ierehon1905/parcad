/**
 * A slider a finger can hold: a 44 px row, the track filled from where it
 * starts, and marks at the points along it that mean something.
 *
 * Driven by pointer events rather than a native range input, because a drag
 * that starts on it must belong to it: in a chat on a phone the page sits in
 * a native scroll view that takes any touch the page does not cancel, and a
 * native range input's own touch handling is what cancelling would switch off.
 */

import { useEffect, useRef } from "preact/hooks";

import { holdTouches } from "../../hold-touches";

export interface SliderProps {
  /** Where the thumb is, 0 to 1. */
  at: number;
  /** Points along the track to mark, 0 to 1. */
  marks?: number[];
  onMove: (at: number) => void;
  label: string;
  /** What `at` means, for a screen reader: the value with its unit. */
  text?: string;
}

/** How far an arrow key moves it, and Page Up or Down. */
const STEP = 0.01;
const PAGE = 0.1;

export function Slider({ at, marks = [], onMove, label, text }: SliderProps) {
  const row = useRef<HTMLDivElement>(null);
  const track = useRef<HTMLDivElement>(null);
  useEffect(() => holdTouches(row.current!), []);
  const follow = (event: PointerEvent) => {
    const box = track.current!.getBoundingClientRect();
    onMove(Math.min(1, Math.max(0, (event.clientX - box.left) / box.width)));
  };
  const keys: Record<string, number> = {
    ArrowLeft: at - STEP,
    ArrowDown: at - STEP,
    ArrowRight: at + STEP,
    ArrowUp: at + STEP,
    PageDown: at - PAGE,
    PageUp: at + PAGE,
    Home: 0,
    End: 1,
  };
  return (
    <div
      ref={row}
      role="slider"
      tabIndex={0}
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(at * 100)}
      aria-valuetext={text}
      class="relative h-11 min-w-0 flex-1 cursor-pointer touch-none rounded-full outline-none focus-visible:ring-2 focus-visible:ring-accent-edge"
      onPointerDown={(event) => {
        event.currentTarget.setPointerCapture(event.pointerId);
        follow(event);
      }}
      onPointerMove={(event) => {
        if (event.currentTarget.hasPointerCapture(event.pointerId)) follow(event);
      }}
      onKeyDown={(event) => {
        const next = keys[event.key];
        if (next === undefined) return;
        event.preventDefault();
        onMove(Math.min(1, Math.max(0, next)));
      }}
    >
      {/* Inset by half the thumb, so the thumb's centre runs end to end of the track. */}
      <div ref={track} class="absolute inset-x-3.5 top-1/2 h-1.5 -translate-y-1/2 rounded-full bg-line">
        <div class="h-full rounded-full bg-accent" style={{ width: `${at * 100}%` }} />
        {marks.map((mark) => (
          <div
            key={mark}
            class="absolute top-1/2 h-4 w-1 -translate-x-1/2 -translate-y-1/2 rounded-full bg-bad"
            style={{ left: `${mark * 100}%` }}
          />
        ))}
        <div
          class="absolute top-1/2 size-7 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-accent bg-ink shadow-[0_2px_8px_rgb(0_0_0/0.45)]"
          style={{ left: `${at * 100}%` }}
        />
      </div>
    </div>
  );
}
