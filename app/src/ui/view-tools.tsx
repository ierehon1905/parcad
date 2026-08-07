/**
 * The controls that change what you are looking at, on the thing you are
 * looking at.
 *
 * These three — which kernel, how finely it is meshed, where it is cut open —
 * used to live in the titlebar, beside Save and Export. That grouping was wrong
 * in a way worth naming, because the fix is not cosmetic: **none of them
 * changes the part.** Save writes a file, Export writes a file, and the picker
 * chooses which file; those belong to the document. A section plane belongs to
 * the eye. Putting all six in one row said they were the same kind of thing,
 * and the row was crowded enough that the second author — the agent editing the
 * same file — got pushed to the far edge of the window.
 *
 * So they moved onto the viewport, which is also where every other CAD program
 * puts them — and on the way, one of the three turned out not to exist.
 *
 * **The mesh-detail slider is gone, not moved.** It was there for a long time,
 * dimmed under B-rep, and reading the host settled what it actually did:
 * `Backend::is_exact` covers *both* kernels this window offers, "no grid to
 * coarsen and no use for a requested depth". So the slider moved a number
 * nothing read, in either mode, every time it was dragged. Dimming it under one
 * kernel had disguised that as a mode rather than a defect. See `DEPTH` in
 * `state.ts` for what replaced it and what would bring it back.
 *
 * **The section position appears once there is a section.** No axis means no
 * plane to place, and a slider that cannot move is furniture. That is the same
 * rule the deletion above came from: show the controls that apply to the state
 * the app is actually in, and if a control never applies, it is not a control.
 *
 * This file owns no measurement. The section's travel is the part's own
 * measured bounds, and the deflection actually reached is reported by the
 * kernel in the panel below rather than promised here.
 */

import { useComputed, useSignalEffect } from "@preact/signals";

import * as engine from "../engine";
import * as S from "../state";
import { Icon, type IconName } from "./icons";
import { tip } from "./tooltip";

/** A row: an icon that names the setting, then the setting. */
const ROW = "flex items-center gap-2 h-6";
/** The trough a segmented control sits in. */
const SEGMENTS = "flex items-center gap-0.5 p-0.5 rounded-md border border-line bg-panel-2/70";
/** Shape then colour, never both from two sources — see project-browser.tsx. */
const SEGMENT =
  "flex items-center gap-1 px-1.5 py-0.5 rounded cursor-pointer text-ink-dim hover:text-ink";
const SEGMENT_ON =
  "flex items-center gap-1 px-1.5 py-0.5 rounded cursor-pointer text-accent bg-accent-deep/70";

export function ViewTools() {
  // The plane is handed to three.js, which is not reactive: an effect is the
  // seam between the two, and it re-runs whenever any part of the cut changes.
  useSignalEffect(() => {
    const axis = S.sectionAxis.value;
    const bounds = S.bounds.value;
    S.viewportRef.current?.setSection(
      axis === "" || !bounds ? undefined : { axis, atMm: S.sectionAt.value, keep: S.sectionKeep.value },
    );
  });

  // Both halves matter. Re-fitting is what makes one slider work for a 6 mm
  // part and a 600 mm one; keeping the position is what stops a section jumping
  // to the middle of the part on every keystroke, which is the difference
  // between a usable section and one you re-aim after every edit.
  const travel = useComputed(() => {
    const axis = S.sectionAxis.value;
    const bounds = S.bounds.value;
    if (axis === "" || !bounds) return undefined;
    const min = bounds.min[axis];
    const max = bounds.max[axis];
    // A hundred steps across the part: fine enough to walk a plane through a
    // 2 mm wall, coarse enough that dragging it is not a slideshow.
    return { min, max, step: Math.max((max - min) / 100, 1e-4) };
  });

  useSignalEffect(() => {
    const range = travel.value;
    if (!range) return;
    const clamped = Math.min(Math.max(S.sectionAt.peek(), range.min), range.max);
    if (clamped !== S.sectionAt.peek()) S.sectionAt.value = clamped;
  });

  return (
    <div class="flex flex-col gap-1.5 px-2 py-2 rounded-lg border border-line bg-glass/86 backdrop-blur-lg text-ink-dim text-tiny">
      <div
        class={ROW}
        {...tip({
          title: "Geometry kernel",
          // Both of these mesh the same exact solid; what differs is how much
          // of the topology comes back with it. Saying "preview is the faster
          // approximate one" would be the wrong mental model and would explain
          // the missing face count as imprecision rather than as omission.
          text: "Exact returns the solid's real faces and edges, so you can hover an edge, aim a selector at one and read a face count. Preview meshes the same solid and returns the triangles only — quicker to draw, and nothing in the viewport is pickable.",
        })}
      >
        <Icon name="kernel" class="size-4 shrink-0" />
        <Segments
          value={S.kernel.value}
          options={[
            { value: "brep", icon: "kernel", label: "exact" },
            { value: "preview", icon: "mesh", label: "preview" },
          ]}
          onPick={(value) => {
            if (value === S.kernel.value) return;
            S.kernel.value = value;
            // A kernel swap changes the geometry, not the part: the camera stays.
            engine.schedule();
          }}
        />
      </div>

      <div
        class={ROW}
        {...tip({
          title: "Section",
          text: "Cut the part open on a plane, to see a wall or a pocket from inside. The same section an agent asks for by name through evaluate_part.",
        })}
      >
        <Icon name="section" class="size-4 shrink-0" />
        <Segments
          value={S.sectionAxis.value}
          options={[
            { value: "", label: "off" },
            { value: "x", label: "X" },
            { value: "y", label: "Y" },
            { value: "z", label: "Z" },
          ]}
          onPick={(value) => {
            const axis = value as "" | "x" | "y" | "z";
            if (axis === S.sectionAxis.value) return;
            const bounds = S.bounds.peek();
            if (axis !== "" && bounds) {
              // A fresh axis starts in the middle of the part, where a section
              // is most likely to cross something worth seeing — the same
              // default the kernel picks when an agent names an axis and no
              // position. Open it facing the camera: keeping the near half
              // instead puts the cut on the far side of the material, which
              // looks exactly like no section at all.
              S.sectionAt.value = (bounds.min[axis] + bounds.max[axis]) / 2;
              S.sectionKeep.value = S.viewportRef.current?.keepFacingCamera(axis) ?? "below";
            }
            S.sectionAxis.value = axis;
          }}
        />
      </div>

      {travel.value && (
        <div class={ROW}>
          <input
            type="range"
            min={travel.value.min}
            max={travel.value.max}
            step={travel.value.step}
            value={S.sectionAt.value}
            class="w-[112px] p-0"
            onInput={(e) => (S.sectionAt.value = Number(e.currentTarget.value))}
          />
          {/* Holds its width as the slider moves, so the flip button beside it
              does not shuffle sideways while you drag. */}
          <span class="font-mono text-ink min-w-[7ch] text-right">
            {S.sectionAt.value.toFixed(1)} mm
          </span>
          <button
            type="button"
            class="flex p-1 rounded border border-line bg-panel-2 cursor-pointer hover:border-accent"
            {...tip({ title: "Flip the cut", text: "Keep the other half." })}
            onClick={() => (S.sectionKeep.value = S.sectionKeep.value === "below" ? "above" : "below")}
          >
            <Icon name="flip" class="size-3.5 shrink-0" />
          </button>
        </div>
      )}
    </div>
  );
}

/** A closed set of choices, all of them visible at once. */
function Segments({
  value,
  options,
  onPick,
}: {
  value: string;
  options: { value: string; icon?: IconName; label: string }[];
  onPick: (value: string) => void;
}) {
  return (
    <div class={SEGMENTS}>
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          class={option.value === value ? SEGMENT_ON : SEGMENT}
          onClick={() => onPick(option.value)}
        >
          {option.icon && <Icon name={option.icon} class="size-3.5 shrink-0" />}
          <span>{option.label}</span>
        </button>
      ))}
    </div>
  );
}
