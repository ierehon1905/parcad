/**
 * The icon set: one small drawing per operation the DSL can perform.
 *
 * Every mechanical CAD program draws its operations rather than naming them,
 * and for the same reason: `fillet` and `chamfer` are four letters apart and a
 * rounded corner beside a cut one is not. What is *not* worth copying is what
 * the icon does when pressed — in Fusion a toolbar button starts a modal
 * command, and there is no modal command here, because `part.js` is the whole
 * model. An icon in this app labels a name in the language; pressing it writes
 * that name into the source.
 *
 * The drawing grammar is Fusion's, though, because it is the right one and
 * every machinist already reads it:
 *
 * - **Material is shaded, not outlined.** One isometric frame, lit from the
 *   top-left, three tones off `currentColor` — `<Top>` at 92%, `<Side>` at 58%,
 *   `<Shade>` at 34%. Because they are `currentColor`, an icon dims with the
 *   control it sits in rather than needing a disabled variant.
 * - **The feature the operation is *about* is the accent.** The fillet's arc,
 *   the hole's bore, the tag's named face. Grey says "stock", accent says "this
 *   is what the call does". That single rule is what makes thirty-odd small
 *   drawings legible without a legend.
 * - **A ghost is what was there before.** Dashed and dim: the sharp corner a
 *   fillet removed, the place a `translate` moved from.
 *
 * Two frames on purpose. Geometry gets shaded solids; chrome — folder, save,
 * export — stays a line icon, because it is an affordance rather than a part.
 * Gold appears nowhere: it means "source selection" everywhere else in this app
 * and its whole job is meaning one thing.
 */

import type { JSX } from "preact";

// --------------------------------------------------------------- materials

/** The lit face. */
const Top = ({ d }: { d: string }) => <path d={d} fill="currentColor" fill-opacity=".92" />;
/** The face turned away from the light. */
const Side = ({ d }: { d: string }) => <path d={d} fill="currentColor" fill-opacity=".58" />;
/** The face in shadow. */
const Shade = ({ d }: { d: string }) => <path d={d} fill="currentColor" fill-opacity=".34" />;
/** A body seen flat rather than in the isometric frame. */
const Body = ({ d, at = ".5", rule }: { d: string; at?: string; rule?: "evenodd" }) => (
  <path d={d} fill="currentColor" fill-opacity={at} fill-rule={rule} />
);

/** What the operation is about, filled. */
const Feature = ({ d, at = "1", rule }: { d: string; at?: string; rule?: "evenodd" }) => (
  <path d={d} fill="var(--color-accent)" fill-opacity={at} fill-rule={rule} />
);
/** What the operation is about, as a line — an arc, an axis, a bore rim. */
const Mark = ({ d, w = "1.8" }: { d: string; w?: string }) => (
  <path
    d={d}
    fill="none"
    stroke="var(--color-accent)"
    stroke-width={w}
    stroke-linecap="round"
    stroke-linejoin="round"
  />
);
/** What was there before the operation, or the tool it consumed. */
const Ghost = ({ d }: { d: string }) => (
  <path
    d={d}
    fill="none"
    stroke="currentColor"
    stroke-opacity=".5"
    stroke-width="1.3"
    stroke-dasharray="2.4 2"
    stroke-linecap="round"
    stroke-linejoin="round"
  />
);
/** Chrome: an affordance, not a part. */
const Line = ({ d, o }: { d: string; o?: string }) => (
  <path
    d={d}
    fill="none"
    stroke="currentColor"
    stroke-opacity={o}
    stroke-width="1.7"
    stroke-linecap="round"
    stroke-linejoin="round"
  />
);
const Dots = ({ at, accent = true }: { at: [number, number][]; accent?: boolean }) => (
  <>
    {at.map(([x, y]) => (
      <ellipse
        key={`${x},${y}`}
        cx={x}
        cy={y}
        rx="1.7"
        ry="1"
        fill={accent ? "var(--color-accent)" : "currentColor"}
        fill-opacity={accent ? "1" : ".7"}
      />
    ))}
  </>
);

/**
 * The one isometric cube every solid is drawn in.
 *
 * Shared so `box`, `edges`, `vertices`, `tag` and `section` all agree about
 * which way the part is turned — five cubes at five angles would read as five
 * different parts.
 */
const CUBE_TOP = "M12 2.8 20.4 7.6 12 12.4 3.6 7.6Z";
const CUBE_LEFT = "M3.6 7.6v8.8L12 21.2v-8.8Z";
const CUBE_RIGHT = "M20.4 7.6v8.8L12 21.2v-8.8Z";
const Cube = () => (
  <>
    <Top d={CUBE_TOP} />
    <Side d={CUBE_LEFT} />
    <Shade d={CUBE_RIGHT} />
  </>
);

/** A thin slab, for anything drilled or patterned. */
const PLATE_TOP = "M12 5 21.6 10.4 12 15.8 2.4 10.4Z";
const PLATE_LEFT = "M2.4 10.4v3L12 18.8v-3Z";
const PLATE_RIGHT = "M21.6 10.4v3L12 18.8v-3Z";
const Plate = () => (
  <>
    <Top d={PLATE_TOP} />
    <Side d={PLATE_LEFT} />
    <Shade d={PLATE_RIGHT} />
  </>
);

/** An upright cylinder: the wall, then the lit top disc. */
const Tube = ({ top = 6.6, bottom = 17.4, r = 8.4 }: { top?: number; bottom?: number; r?: number }) => (
  <>
    <Body
      d={`M${12 - r} ${top}v${bottom - top}a${r} 3.8 0 0 0 ${r * 2} 0V${top}a${r} 3.8 0 0 1-${r * 2} 0Z`}
      at=".46"
    />
    <ellipse cx="12" cy={top} rx={r} ry="3.8" fill="currentColor" fill-opacity=".9" />
  </>
);

const DRAWINGS = {
  // ---------------------------------------------------------------- solids
  box: <Cube />,
  cylinder: <Tube />,
  sphere: (
    <>
      <circle cx="12" cy="12" r="9.2" fill="currentColor" fill-opacity=".4" />
      {/* The lit crescent, so a circle reads as a ball. */}
      <Body d="M12 2.8A9.2 9.2 0 0 0 12 21.2 11.6 11.6 0 0 1 12 2.8Z" at=".85" />
    </>
  ),
  cone: (
    <>
      <Side d="M12 2.4 3.4 17.6a8.6 3.7 0 0 0 8.6 3.7Z" />
      <Shade d="M12 2.4 20.6 17.6a8.6 3.7 0 0 1-8.6 3.7Z" />
    </>
  ),
  torus: (
    <>
      <Body
        d="M21.2 12a9.2 5.1 0 1 1-18.4 0 9.2 5.1 0 1 1 18.4 0M15.5 12a3.5 1.9 0 1 0-7 0 3.5 1.9 0 1 0 7 0Z"
        at=".5"
        rule="evenodd"
      />
      {/* The lit crown of the ring. */}
      <path
        d="M2.8 12a9.2 5.1 0 0 1 18.4 0"
        fill="none"
        stroke="currentColor"
        stroke-opacity=".85"
        stroke-width="2.4"
      />
    </>
  ),
  ngon: (
    <>
      <Top d="M12 3.6 20 8.2v2L12 14.8 4 10.2v-2Z" />
      <Side d="M4 10.2v4.4L12 19.2v-4.4Z" />
      <Shade d="M20 10.2v4.4L12 19.2v-4.4Z" />
    </>
  ),

  // -------------------------------------------------------- drawn profiles
  // The outline you author is the accent; the solid it becomes is the stock.
  extrude: (
    <>
      <Top d="M12 2.6 19.6 7 12 11.4 4.4 7Z" />
      <Side d="M4.4 7v5.6L12 17v-5.6Z" />
      <Shade d="M19.6 7v5.6L12 17v-5.6Z" />
      <Mark d="M12 13.6 19.6 18 12 22.4 4.4 18Z" />
    </>
  ),
  revolve: (
    <>
      <Tube top={8.4} bottom={18} r={6.4} />
      {/* The axis it is turned about, and the direction of the turn. */}
      <Mark d="M12 1.4v4.2M12 20.4v2.2" w="1.5" />
      <Mark d="M4.6 4.6a10 10 0 0 1 6-2.8" w="1.5" />
      <Mark d="M3.2 1.9 4.4 4.8 7.4 4" w="1.5" />
    </>
  ),
  loft: (
    <>
      <Body d="M8.6 5.6 3.8 17.6h16.4L15.4 5.6Z" at=".45" />
      <ellipse cx="12" cy="5.6" rx="3.4" ry="1.5" fill="var(--color-accent)" />
      <ellipse cx="12" cy="17.6" rx="8.2" ry="3.5" fill="var(--color-accent)" fill-opacity=".85" />
    </>
  ),
  sweep: (
    <>
      <Mark d="M4.6 21V13.4A6.8 6.8 0 0 1 11.4 6.6H20" />
      <Top d="M4.6 17.6 8.6 19.9 4.6 22.2.6 19.9Z" />
      <Side d="M.6 19.9v1.6l4 2.3v-1.6Z" />
      <Shade d="M8.6 19.9v1.6l-4 2.3v-1.6Z" />
    </>
  ),
  pipe: (
    <>
      <path
        d="M5 21.4V13.6A7 7 0 0 1 12 6.6h7.6"
        fill="none"
        stroke="currentColor"
        stroke-opacity=".45"
        stroke-width="6.4"
        stroke-linecap="round"
      />
      <Mark d="M5 21.4V13.6A7 7 0 0 1 12 6.6h7.6" w="2.2" />
    </>
  ),

  // ------------------------------------------------------------- combining
  // Flat rather than isometric, and deliberately: a boolean is about which
  // region survives, and two overlapping outlines say that in a way two
  // overlapping solids cannot.
  // What survives is the accent; what is consumed is grey. At this size that
  // contrast has to do the whole job — four outlines of two overlapping squares
  // are four identical icons.
  union: <Feature d="M3 5h11v4h7v11H10v-4H3Z" at=".9" />,
  cut: (
    <>
      <Ghost d="M10 9h11v11H10Z" />
      <Feature d="M3 5h11v4h-4v7H3Z" at=".9" />
    </>
  ),
  intersect: (
    <>
      <Body d="M3 5h11v11H3Z" at=".3" />
      <Body d="M10 9h11v11H10Z" at=".3" />
      <Feature d="M10 9h4v7h-4Z" at=".95" />
    </>
  ),
  blend: (
    <>
      <Body d="M3 5h11v4h7v11H10v-4H3Z" at=".5" />
      {/* The seam, rounded — which is the whole of what `blend` adds. */}
      <Mark d="M14 5.6A3.6 3.6 0 0 0 17.6 9.2" w="2.2" />
      <Mark d="M10 19.4A3.6 3.6 0 0 0 6.4 15.8" w="2.2" />
    </>
  ),

  // -------------------------------------------------------- edge treatments
  // Seen as a section on the corner itself, because that is the only view in
  // which a round, a bevel and a curvature-continuous blend differ.
  // The three of these differ only in the shape of the corner, so the corner is
  // most of the drawing and the material each one *removed* is filled in as a
  // ghost. Without that wedge they were three grey squares with a faint blue
  // edge, which is precisely the confusion an icon set is supposed to prevent.
  fillet: (
    <>
      <Body d="M3.5 3.5h5.5a11.5 11.5 0 0 1 11.5 11.5v5.5H3.5Z" at=".52" />
      <Body d="M9 3.5h11.5V15A11.5 11.5 0 0 0 9 3.5Z" at=".16" />
      <Ghost d="M9 3.5h11.5V15" />
      <Mark d="M9 3.5A11.5 11.5 0 0 1 20.5 15" w="2.6" />
    </>
  ),
  chamfer: (
    <>
      <Body d="M3.5 3.5h5.5L20.5 15v5.5H3.5Z" at=".52" />
      <Body d="M9 3.5h11.5V15Z" at=".16" />
      <Ghost d="M9 3.5h11.5V15" />
      <Mark d="M9 3.5 20.5 15" w="2.6" />
    </>
  ),
  smooth: (
    <>
      <Body d="M3.5 3.5h3.5c9 0 13.5 4.5 13.5 13.5v3.5H3.5Z" at=".52" />
      <Body d="M7 3.5h13.5V17C20.5 8 16 3.5 7 3.5Z" at=".16" />
      <Ghost d="M7 3.5h13.5V17" />
      <Mark d="M7 3.5c9 0 13.5 4.5 13.5 13.5" w="2.6" />
    </>
  ),
  shell: (
    <>
      <Side d={CUBE_LEFT} />
      <Shade d={CUBE_RIGHT} />
      {/* The wall left behind is the accent; the cavity inside it is empty. */}
      <Feature d="M12 2.8 20.4 7.6 12 12.4 3.6 7.6Zm0 2.6L6.4 8.6 12 11.8l5.6-3.2Z" at=".9" rule="evenodd" />
    </>
  ),
  offset: (
    <>
      <Top d="M12 6.2 17.8 9.5 12 12.8 6.2 9.5Z" />
      <Side d="M6.2 9.5v6L12 18.8v-6Z" />
      <Shade d="M17.8 9.5v6L12 18.8v-6Z" />
      <Mark d="M12 2.6 21.2 7.9v9.2L12 22.4 2.8 17.1V7.9Z" w="1.5" />
    </>
  ),

  // ------------------------------------------------------------- selection
  edges: (
    <>
      <Cube />
      <Mark d="M12 12.4 20.4 7.6" w="2.6" />
    </>
  ),
  vertices: (
    <>
      <Cube />
      <circle cx="12" cy="2.8" r="2.6" fill="var(--color-accent)" />
    </>
  ),
  tag: (
    <>
      <Feature d={CUBE_TOP} at=".9" />
      <Side d={CUBE_LEFT} />
      <Shade d={CUBE_RIGHT} />
    </>
  ),

  // ------------------------------------------------------------- placement
  translate: (
    <>
      <Ghost d="M8 12.4 14 15.8 8 19.2 2 15.8Z" />
      <Top d="M15 4.4 21 7.8 15 11.2 9 7.8Z" />
      <Side d="M9 7.8v4.4L15 15.6v-4.4Z" />
      <Shade d="M21 7.8v4.4L15 15.6v-4.4Z" />
      <Mark d="M6.6 13.6 11.4 10.8M8.6 10.4h3.2v3.2" w="1.5" />
    </>
  ),
  rotate: (
    <>
      <Cube />
      <Mark d="M21.4 6.6a11 11 0 0 1-3 4.4" w="1.6" />
      <Mark d="M22.2 2.8 21.8 7 17.6 6.2" w="1.6" />
    </>
  ),
  mirror: (
    <>
      <Side d="M10.4 5.2 3.6 9.1v6.4l6.8 3.9Z" />
      <Top d="M10.4 5.2 3.6 9.1l6.8 3.9Z" />
      <Ghost d="M13.6 5.2 20.4 9.1v6.4l-6.8 3.9Z" />
      <Mark d="M12 1.8v20.4" w="1.5" />
    </>
  ),
  scale: (
    <>
      <Top d="M11 7.4 16 10.3 11 13.2 6 10.3Z" />
      <Side d="M6 10.3v4.4L11 17.6v-4.4Z" />
      <Shade d="M16 10.3v4.4L11 17.6v-4.4Z" />
      <Mark d="M14.6 14.2 21 17.9M21.4 13.6v4.8h-4.8" w="1.5" />
    </>
  ),

  // -------------------------------------------------------------- patterns
  repeat: (
    <>
      <g opacity=".45">
        <Top d="M5 6.6 9 8.9 5 11.2 1 8.9Z" />
        <Side d="M1 8.9v4L5 15.2v-4Z" />
        <Shade d="M9 8.9v4L5 15.2v-4Z" />
      </g>
      <g opacity=".7">
        <Top d="M12 6.6 16 8.9 12 11.2 8 8.9Z" />
        <Side d="M8 8.9v4L12 15.2v-4Z" />
        <Shade d="M16 8.9v4L12 15.2v-4Z" />
      </g>
      <Top d="M19 6.6 23 8.9 19 11.2 15 8.9Z" />
      <Side d="M15 8.9v4L19 15.2v-4Z" />
      <Shade d="M23 8.9v4L19 15.2v-4Z" />
    </>
  ),
  grid: (
    <>
      <Plate />
      <Dots at={[[12, 7.4], [7.2, 10.1], [16.8, 10.1], [12, 12.8]]} />
    </>
  ),
  polar: (
    <>
      <Tube top={9} bottom={15.6} r={8.6} />
      <Dots at={[[12, 5.6], [18.6, 9], [18.6, 9], [12, 12.4], [5.4, 9]]} />
    </>
  ),
  around: (
    <>
      <Mark d="M12 1.6v20.8" w="1.5" />
      <g opacity=".55">
        <Top d="M4.6 10 8.4 12.2 4.6 14.4.8 12.2Z" />
        <Side d="M.8 12.2v3.2l3.8 2.2v-3.2Z" />
      </g>
      <Top d="M19.4 10 23.2 12.2 19.4 14.4 15.6 12.2Z" />
      <Side d="M15.6 12.2v3.2l3.8 2.2v-3.2Z" />
      <Shade d="M23.2 12.2v3.2l-3.8 2.2v-3.2Z" />
    </>
  ),

  // ---------------------------------------------------------------- holes
  hole: (
    <>
      <Plate />
      <ellipse cx="12" cy="10.4" rx="3.4" ry="1.9" fill="var(--color-accent)" />
    </>
  ),
  // In section, because that is the only view in which these three differ.
  countersink: (
    <>
      <Body d="M3 4.6h18v14.8H3Z" at=".45" />
      <Feature d="M6.6 4.6 9.6 9.2v10.2h4.8V9.2l3-4.6Z" at=".85" />
      <Mark d="M6.6 4.6 9.6 9.2v10.2M17.4 4.6 14.4 9.2v10.2" w="1.5" />
    </>
  ),
  counterbore: (
    <>
      <Body d="M3 4.6h18v14.8H3Z" at=".45" />
      <Feature d="M6.6 4.6v4.6h3v10.2h4.8V9.2h3V4.6Z" at=".85" />
      <Mark d="M6.6 4.6v4.6h3v10.2M17.4 4.6v4.6h-3v10.2" w="1.5" />
    </>
  ),
  thread: (
    <>
      <Tube top={6} bottom={18} r={6} />
      <Mark d="M6.6 9.4h10.8M6 12.6h12M6.6 15.8h10.8" w="1.4" />
    </>
  ),

  // ---------------------------------------------------------------- chrome
  kernel: <Cube />,
  // A cut, drawn past the silhouette on both sides so it reads as a plane
  // through the part rather than as one of its faces.
  section: (
    <>
      <g opacity=".55">
        <Cube />
      </g>
      <Feature d="M1.2 10.4 12 16.6 22.8 10.4 12 4.2Z" at=".8" />
    </>
  ),
  // A tessellated cube: the mesh, not the solid.
  mesh: (
    <>
      <Line d={CUBE_TOP} o=".85" />
      <Line d="M3.6 7.6v8.8L12 21.2 20.4 16.4V7.6" o=".85" />
      <Line d="M12 12.4v8.8M12 12.4 3.6 7.6M12 12.4 20.4 7.6" o=".5" />
      <Line d="M3.6 7.6 12 21.2 20.4 7.6M3.6 16.4 12 12.4l8.4 4" o=".3" />
    </>
  ),
  folder: (
    <Line d="M3 6.6A2 2 0 0 1 5 4.6h4.6l2.4 3h7A2 2 0 0 1 21 9.6v8A2 2 0 0 1 19 19.6H5a2 2 0 0 1-2-2Z" />
  ),
  save: (
    <>
      <Line d="M3.6 5.6a2 2 0 0 1 2-2h10.2l4.6 4.6v10.2a2 2 0 0 1-2 2H5.6a2 2 0 0 1-2-2Z" />
      <Line d="M8 3.6v5.6h8V3.6M7 20.4v-6h10v6" />
    </>
  ),
  export: (
    <>
      <Line d="M13.8 3.6H5.4a1.8 1.8 0 0 0-1.8 1.8v13.2a1.8 1.8 0 0 0 1.8 1.8h13.2a1.8 1.8 0 0 0 1.8-1.8v-8.4" />
      <Line d="M11.2 12.8 20.4 3.6M14.4 3.6h6v6" />
    </>
  ),
  copy: (
    <>
      <Line d="M9.4 9.4a1.8 1.8 0 0 1 1.8-1.8h8.2a1.8 1.8 0 0 1 1.8 1.8v8.2a1.8 1.8 0 0 1-1.8 1.8h-8.2a1.8 1.8 0 0 1-1.8-1.8Z" />
      <Line d="M16.4 7.6V4.8A1.8 1.8 0 0 0 14.6 3H4.8A1.8 1.8 0 0 0 3 4.8v9.8a1.8 1.8 0 0 0 1.8 1.8h2.8" />
    </>
  ),
  flip: (
    <>
      <Line d="M3.6 8.4h14.1M14.1 4.8l3.6 3.6-3.6 3.6" />
      <Line d="M20.4 15.6H6.3M9.9 12l-3.6 3.6L9.9 19.2" />
    </>
  ),
  chevron: <Line d="M6.6 9.3 12 14.7l5.4-5.4" />,
  plus: <Line d="M12 4.8v14.4M4.8 12h14.4" />,
  // The one mark in the set that is not geometry, because it is the only one
  // about the other author rather than the part.
  agent: (
    <path
      d="M12 1.8 14.6 9.4 22.2 12 14.6 14.6 12 22.2 9.4 14.6 1.8 12 9.4 9.4Z"
      fill="currentColor"
    />
  ),
} satisfies Record<string, JSX.Element>;

export type IconName = keyof typeof DRAWINGS;

/**
 * One icon.
 *
 * `aria-hidden` throughout: every icon in this app sits beside a label or on a
 * control that carries its own name, so announcing the drawing as well would
 * only repeat it.
 */
export function Icon({ name, class: cls = "size-5 shrink-0" }: { name: IconName; class?: string }) {
  return (
    <svg viewBox="0 0 24 24" fill="none" class={cls} aria-hidden="true">
      {DRAWINGS[name]}
    </svg>
  );
}
