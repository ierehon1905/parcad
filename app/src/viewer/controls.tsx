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
import { bodyPoses, drawnDrive, valueAt, type Mechanism, type Move } from "../motion";
import { Icon } from "../ui/icons";
import { Slider } from "../ui/components/Slider";
import { TOUCH_TROUGH, Toggle } from "../ui/components/Toggle";
import { VIEWS, type Viewport } from "../viewport";

export interface ShownBody {
  name: string;
  reference: boolean;
  /** Which of the look's body colours it is drawn in, counting from 0; absent for a reference. */
  tone?: number;
}

type View = keyof typeof VIEWS;

const bodies = signal<ShownBody[]>([]);
const mechanisms = signal<Mechanism[]>([]);
/** Each mechanism's drive, 0 to 1. */
const drives = signal<number[]>([]);
const hidden = signal<ReadonlySet<string>>(new Set());
const xray = signal(false);
/** The preset last flown to, until a hand turns the view. */
const view = signal<View | undefined>("threeQuarter");
/** The mechanism sweeping on its own, if one is. */
const playing = signal<number | undefined>(undefined);
/** Whether a part has arrived: a one-body part names no bodies and still gets its controls. */
const partShown = signal(false);

/** One sweep across a mechanism's whole drive, in seconds. */
const SWEEP_S = 2.2;

const PRESETS: { view: View; label: string }[] = [
  { view: "threeQuarter", label: "Corner" },
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
    const poses = bodyPoses(mechanisms.value, drives.value);
    for (const { moves } of mechanisms.value) for (const { body } of moves) viewport.setBodyPose(body, poses.get(body));
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

/** A new part: every body shown, every mechanism at the pose it was drawn in. */
export function showPart(partBodies: ShownBody[], partMechanisms: Mechanism[]) {
  playing.value = undefined;
  hidden.value = new Set();
  bodies.value = partBodies;
  mechanisms.value = partMechanisms;
  drives.value = partMechanisms.map(drawnDrive);
  view.value = "threeQuarter";
  partShown.value = true;
}

// Along the top, where an iso view leaves sky: at the bottom the strip covered the part's near corner.
function OnView({ viewport }: { viewport: Viewport }) {
  if (!partShown.value) return null;
  return (
    <div class="pointer-events-none absolute inset-x-2 top-2 flex items-start justify-between gap-2">
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
      <div class="pointer-events-auto text-small">
        <Toggle size="touch-pill" pressed={xray.value} onClick={() => (xray.value = !xray.value)}>
          <Icon name="xray" class="size-5" />
          <span>See through</span>
        </Toggle>
      </div>
    </div>
  );
}

function Panel() {
  const many = bodies.value.length > 1;
  if (!many && mechanisms.value.length === 0) return null;
  return (
    <div class="flex flex-col gap-1.5 border-t border-line bg-panel px-3 py-2">
      {many && (
        <div class="flex flex-wrap gap-2 py-1">
          {bodies.value.map((body) => (
            <BodyChip key={body.name} body={body} />
          ))}
        </div>
      )}
      {mechanisms.value.map((mechanism, i) => (
        <MotionRow key={i} mechanism={mechanism} index={i} />
      ))}
    </div>
  );
}

/** The dot a chip wears in its body's colour: written out, so Tailwind emits every `--color-body-N` the scene reads. */
const BODY_DOTS = ["bg-body-1", "bg-body-2", "bg-body-3", "bg-body-4", "bg-body-5", "bg-body-6", "bg-body-7", "bg-body-8"];

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
      {body.tone !== undefined && shown && <span class={`size-2.5 shrink-0 rounded-full ${BODY_DOTS[body.tone % BODY_DOTS.length]}`} />}
      <span class="truncate">{body.name}</span>
    </button>
  );
}

function MotionRow({ mechanism, index }: { mechanism: Mechanism; index: number }) {
  const [low, high] = mechanism.drive;
  const drive = drives.value[index] ?? low;
  const along = (d: number) => (d - low) / (high - low);
  const { label } = mechanism;
  // The slider reads in the first moving body's own unit: a lever's degrees, a drawer's mm.
  const lead = mechanism.moves.find((move) => move.kind !== "rides") ?? mechanism.moves[0];
  const value = format(lead, valueAt(lead, drive));
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
      {/* The value beside its name, where the eye goes after the label, not past the end of the track. */}
      <span class="flex shrink-0 flex-col leading-tight">
        <span class="max-w-20 truncate text-small text-ink-dim">{label}</span>
        <span class="min-w-[6.5ch] font-mono text-small text-ink">{value}</span>
      </span>
      <Slider
        at={along(drive)}
        marks={mechanism.failsAt.map(along)}
        label={label}
        text={value}
        onMove={(at) => {
          playing.value = undefined;
          setDrive(index, low + at * (high - low));
        }}
      />
    </div>
  );
}

function format(move: Move, value: number): string {
  const text = move.kind === "turn" ? `${Math.round(value)}°` : `${value.toFixed(1)} mm`;
  return text.replace("-", "−").replace(/^−(0(\.0)?)(°| mm)$/, "$1$3");
}

function setDrive(index: number, drive: number) {
  const next = [...drives.value];
  next[index] = drive;
  drives.value = next;
}

/** Sweep a mechanism end to end and back until stopped, from wherever it is now. */
function play(index: number) {
  if (playing.value === index) {
    playing.value = undefined;
    return;
  }
  playing.value = index;
  const [low, high] = mechanisms.value[index].drive;
  let heading = drives.value[index] >= high ? -1 : 1;
  let last = performance.now();
  const step = (now: number) => {
    if (playing.value !== index) return;
    const seconds = (now - last) / 1000;
    last = now;
    let at = (drives.value[index] - low) / (high - low) + (heading * seconds) / SWEEP_S;
    if (at >= 1) {
      at = 1;
      heading = -1;
    } else if (at <= 0) {
      at = 0;
      heading = 1;
    }
    setDrive(index, low + at * (high - low));
    requestAnimationFrame(step);
  };
  requestAnimationFrame(step);
}
