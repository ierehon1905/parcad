/**
 * What a person can do to the part in a chat: hide a body, see through the
 * part, look from a preset side, and move each moving body along the motion
 * its checks declare. Made for a phone — a finger per control, a word or none,
 * and the view keeping the room.
 *
 * Every pose is the exact built body moved by the transform its motion names
 * (`motion.ts`); nothing here draws an approximation of the part.
 */

import { effect, signal } from "@preact/signals";
import { render } from "preact";

import { holdTouches } from "../hold-touches";
import { bodyPoses, type Motion } from "../motion";
import { Icon } from "../ui/icons";
import { Slider } from "../ui/components/Slider";
import { TOUCH_TROUGH, Toggle } from "../ui/components/Toggle";
import { VIEWS, type Viewport } from "../viewport";

export interface ShownBody {
  name: string;
  reference: boolean;
}

type View = keyof typeof VIEWS;

const bodies = signal<ShownBody[]>([]);
const motions = signal<Motion[]>([]);
/** Each motion's value, in its own unit: degrees or mm from the drawn pose. */
const values = signal<number[]>([]);
const hidden = signal<ReadonlySet<string>>(new Set());
const xray = signal(false);
/** The preset last flown to, until a hand turns the view. */
const view = signal<View | undefined>("threeQuarter");
/** The motion sweeping on its own, if one is. */
const playing = signal<number | undefined>(undefined);

/** One sweep across a motion's whole range, in seconds. */
const SWEEP_S = 2.2;

const PRESETS: { view: View; label: string }[] = [
  { view: "threeQuarter", label: "3/4" },
  { view: "front", label: "Front" },
  { view: "side", label: "Side" },
  { view: "back", label: "Back" },
];

/** Draw the controls: the ones that sit on the view, and the panel under it. */
export function mountControls(viewport: Viewport, onView: HTMLElement, under: HTMLElement) {
  effect(() => viewport.setXray(xray.value));
  effect(() => {
    for (const body of bodies.value) viewport.setBodyHidden(body.name, hidden.value.has(body.name));
  });
  effect(() => {
    const poses = bodyPoses(motions.value, values.value);
    for (const motion of motions.value) viewport.setBodyPose(motion.body, poses.get(motion.body));
  });
  const stage = onView.parentElement!;
  const onPart = (target: EventTarget | null) => target instanceof HTMLCanvasElement;
  // Turning and pinching the part, never scrolling the chat; the buttons on it keep their taps.
  holdTouches(stage, onPart);
  // Any touch on the part is a hand turning it, which no preset names any more.
  stage.addEventListener("pointerdown", (event) => {
    if (onPart(event.target)) view.value = undefined;
  });
  render(<OnView viewport={viewport} />, onView);
  render(<Panel />, under);
}

/** A new part: every body shown, every motion at the pose it was drawn in. */
export function showPart(partBodies: ShownBody[], partMotions: Motion[]) {
  playing.value = undefined;
  hidden.value = new Set();
  bodies.value = partBodies;
  motions.value = partMotions;
  values.value = partMotions.map(() => 0);
  view.value = "threeQuarter";
}

function OnView({ viewport }: { viewport: Viewport }) {
  if (bodies.value.length === 0) return null;
  return (
    <>
      <div class="absolute top-2 right-2">
        <Toggle size="touch" pressed={xray.value} aria-label="See through" onClick={() => (xray.value = !xray.value)}>
          <Icon name="xray" class="size-6" />
        </Toggle>
      </div>
      <div class="pointer-events-none absolute inset-x-0 bottom-2 flex justify-center">
        <div class={`${TOUCH_TROUGH} pointer-events-auto text-small`}>
          {PRESETS.map((preset) => (
            <Toggle
              key={preset.view}
              size="touch-segment"
              pressed={view.value === preset.view}
              onClick={() => {
                view.value = preset.view;
                viewport.look(VIEWS[preset.view]);
              }}
            >
              {preset.label}
            </Toggle>
          ))}
        </div>
      </div>
    </>
  );
}

function Panel() {
  const many = bodies.value.length > 1;
  if (!many && motions.value.length === 0) return null;
  return (
    <div class="flex flex-col gap-1.5 border-t border-line bg-panel px-3 py-2">
      {many && (
        <div class="flex flex-wrap gap-2 py-1">
          {bodies.value.map((body) => (
            <BodyChip key={body.name} body={body} />
          ))}
        </div>
      )}
      {motions.value.map((motion, i) => (
        <MotionRow key={i} motion={motion} index={i} />
      ))}
    </div>
  );
}

const CHIP = "flex min-w-0 max-w-40 items-center gap-1.5 h-10 pl-2.5 pr-3.5 rounded-full border cursor-pointer text-small";

const CHIP_COLOURS = {
  shown: "text-ink border-line bg-panel-2",
  /** The thing the part holds, in the colour it is drawn in. */
  reference: "text-reference border-line bg-panel-2",
  hidden: "text-ink-dim border-dashed border-line bg-transparent",
};

/** A body's name, which shows or hides it. */
function BodyChip({ body }: { body: ShownBody }) {
  const shown = !hidden.value.has(body.name);
  const tone = !shown ? "hidden" : body.reference ? "reference" : "shown";
  return (
    <button
      type="button"
      aria-pressed={shown}
      class={`${CHIP} ${CHIP_COLOURS[tone]}`}
      onClick={() => {
        const next = new Set(hidden.value);
        if (shown) next.add(body.name);
        else next.delete(body.name);
        hidden.value = next;
      }}
    >
      <Icon name={shown ? "eye" : "hidden"} class="size-[18px] shrink-0" />
      <span class="truncate">{body.name}</span>
    </button>
  );
}

function MotionRow({ motion, index }: { motion: Motion; index: number }) {
  const value = values.value[index] ?? 0;
  const span = motion.to - motion.from;
  const along = (v: number) => (span === 0 ? 0 : (v - motion.from) / span);
  const label = motionLabel(motion);
  return (
    <div class="flex items-center gap-2.5">
      <Toggle
        size="touch"
        pressed={playing.value === index}
        aria-label={playing.value === index ? `Stop ${label}` : `Move ${label}`}
        onClick={() => play(index)}
      >
        <Icon name={playing.value === index ? "pause" : "play"} class="size-5" />
      </Toggle>
      <span class="max-w-20 shrink-0 truncate text-small text-ink-dim">{label}</span>
      <Slider
        at={along(value)}
        marks={motion.failsAt.map(along)}
        label={label}
        text={format(motion, value)}
        onMove={(at) => {
          playing.value = undefined;
          setValue(index, motion.from + at * span);
        }}
      />
      <span class="min-w-[6.5ch] shrink-0 text-right font-mono text-small text-ink">{format(motion, value)}</span>
    </div>
  );
}

/** The body's name, and the axis when the body moves more than one way. */
function motionLabel(motion: Motion): string {
  const ways = motions.value.filter((m) => m.body === motion.body).length;
  if (ways === 1) return motion.body;
  const principal = motion.axis.findIndex((v) => Math.abs(Math.abs(v) - 1) < 1e-6);
  return principal < 0 ? motion.body : `${motion.body} ${motion.axis[principal] < 0 ? "−" : ""}${"xyz"[principal]}`;
}

function format(motion: Motion, value: number): string {
  const text = motion.kind === "turn" ? `${Math.round(value)}°` : `${value.toFixed(1)} mm`;
  return text.replace("-", "−").replace(/^−(0(\.0)?)(°| mm)$/, "$1$3");
}

function setValue(index: number, value: number) {
  const next = [...values.value];
  next[index] = value;
  values.value = next;
}

/** Sweep a motion end to end and back until stopped, from wherever it is now. */
function play(index: number) {
  if (playing.value === index) {
    playing.value = undefined;
    return;
  }
  playing.value = index;
  const motion = motions.value[index];
  const span = motion.to - motion.from;
  let heading = (values.value[index] - motion.from) / span >= 1 ? -1 : 1;
  let last = performance.now();
  const step = (now: number) => {
    if (playing.value !== index) return;
    const seconds = (now - last) / 1000;
    last = now;
    let at = (values.value[index] - motion.from) / span + (heading * seconds) / SWEEP_S;
    if (at >= 1) {
      at = 1;
      heading = -1;
    } else if (at <= 0) {
      at = 0;
      heading = 1;
    }
    setValue(index, motion.from + at * span);
    requestAnimationFrame(step);
  };
  requestAnimationFrame(step);
}
