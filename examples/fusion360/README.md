# Fusion 360 recreations

Parts exported from the reference corpus of Fusion 360 documents and recreated
in parcad's DSL, so that each one is a measurable agreement rather than an
impression. Each file records the volume, area, bounding box and face types read
off the Fusion export, so a recreation either agrees with the original or does
not.

**These are seeded as real projects, in a `fusion360` folder.** `seed()` in
`crates/parcad-host/src/projects.rs` keeps the structure, so this arrives in the
project list as an ordinary folder. Only recreations that build are here;
targets that still throw are in `eval/targets/fusion360/`.

**A seeded copy is frozen, and for targets that is a trap.** Seeding runs once
per path and never overwrites, which is right for a part the user has edited and
wrong for reference material: land a recreation here and the copy in
`~/Documents/parcad/fusion360` still throws the old reason, under the same name.
It reads as the work not having happened. It has caught two sessions, one of
which refreshed *before* the recreation landed and reported it fixed — worse
than not refreshing at all.

Nothing detects it, so refresh by hand and check the refreshed copy, not the
file in this directory:

```bash
for f in examples/fusion360/*.js; do
  n=$(basename "$f" .js); d="$HOME/Documents/parcad/fusion360/$n.parcad"
  [ -d "$d" ] && cp "$f" "$d/part.js" && rm -f "$d/README.md" "$d/preview.png"
done
bun tools/run.ts ~/Documents/parcad/fusion360/untriangle-v3.parcad/part.js >/dev/null
```

The `README.md` and `preview.png` go because they are derived from the old
script; the app rewrites them from measured values on the next save. Skipping the
second line is how the mistake above happened — the check has to run against the
user's copy, because that is the file that was wrong. Targets no longer
reach the folder at all; they moved out of `examples/`.

A target that becomes a faithful recreation moves here from
`eval/targets/fusion360/`, and at that point it should also earn a case in
`eval/cases/`.

The exports themselves (`.step`, `.stl`, `measurements.json`) live in
`reference/fusion/`, which is gitignored — they are real designs, not
fixtures. Without them the numbers in each header are still the specification;
only a side-by-side comparison needs the files.

```bash
bun tools/run.ts examples/fusion360/retainer-v1.js > /tmp/part.json
./target/release/parcad /tmp/part.json --brep --step /tmp/part.step --out /tmp/out

# Measure an export itself: solids, faces with surface data, section polygons.
# reference/ is gitignored — point this at a STEP file of your own.
./target/release/parcad --probe-step reference/fusion/UnTriangle-v3/UnTriangle-v3.step
```

The probe is how a target stops being an impression: it reads the STEP through
the isolated kernel and reports exact mass properties, every face's surface
geometry down to B-spline pole grids, and each all-straight boundary loop as a
ready polygon. The same capability is an MCP tool, `probe_step_export`, so an
agent can do this end to end — probe the export, author the script, export it
and probe both sides. It found, first thing, that this folder's headers and
the exports disagree about what is in them: `UnTriangle-v3.step` contains only
the document's *second* body, while the header recorded the first (see below).

## Recreated

| file | part | agreement with the original |
|---|---|---|
| `retainer-v1.js` | plate with a bored, drafted disc | volume +0.0028%, bbox exact, all 23 faces the same surface types — measured by `eval/cases/retainer-v1.json` |
| `../diamond-v19.js` | round brilliant, 57 planar facets | volume, area and bbox agree to every published digit; the same 57 planes — promoted up into `examples/`, measured by `eval/cases/diamond-v19.json` |
| `untitled2-v1.js` | a wavy teardrop standing in a cup, both turned from splines (both bodies) | Body1 volume +0.008%, area +0.027%; Body2 volume +0.0004%, area +0.0001%, against Fusion's own measurements; both bboxes the same; the same faces as surfaces (Fusion writes each surface of revolution as a NURBS). Sections are the export's clamped uniform degree-5 pole rows, the cup's rim filleted at 5 mm — measured by `eval/cases/untitled2-v1.json` |
| `untriangle-v3.js` | impossible-triangle ring of quarter-twisted bars (the export's body, Body12) | volume +0.00024%, area +0.00005%, bbox exact, the same 30 faces (18 plane, 12 nurbs); every twisted wall is the *identical* bilinear surface, corners matched to 1.2e-4 mm — measured by `eval/cases/untriangle-v3.json` |

The diamond needed no new op at all. Fusion built it with BoundaryFill, but the
solid is convex, so it *is* the intersection of its 57 facet half-spaces — an
intersection of rotated boxes, exact. The blocker recorded in
its header was the construction Fusion happened to use, not what the shape
requires; the audit that found this is the reason each remaining target below
names the *geometry* it is blocked on rather than the Fusion feature list.

That one needed a fix to OpenCASCADE itself
(`vendor/occt-sys/patches/0001-tangent-pinch-corner.patch`): the blend where the
disc meets the plate used to return a solid the kernel's own checker rejected,
while reporting success.

## Targets that do not build yet

Five more exports are measured but not recreated, and they live in
`eval/targets/fusion360/`, which is not seeded: a part that only throws is a
TODO, not an example. That README lists what each is blocked on.

The wall moved when `loft` and `sweep` landed, and again when the probe read
UnTriangle's export properly. What that recreation taught, in order of worth:

- **The exports and the headers can disagree about what is in them.**
  `UnTriangle-v3.step` holds one solid, and it is Body12 (24,800 mm³, 18
  plane + 12 nurbs), not the Body1 the header recorded — a recreation aimed
  at the header's numbers was aiming at a body the reference never
  contained. Probe the export first; the header is a note, the file is the
  specification.
- **"NURBS" in a measurement dump does not mean a fitted surface.** All
  twelve of UnTriangle's twisted walls are exactly bilinear — ruled patches
  fully determined by four corners, exported degree-elevated to bicubic. For
  ruled walls the "does OCCT's fit match Fusion's" question dissolves: both
  kernels build the one doubly-ruled surface through the same corners, and
  the recreation's walls match the export's to 1.2e-4 mm, the export's own
  vertex scatter. A *smooth* loft through three or more sections is still a
  genuine fit, and two kernels' fits still owe each other nothing.
- **The loft op was never the blocker there — the pairing was.** OCCT's
  compatibility pass silently re-origined the section wires and rebuilt the
  twisted loft as a straight prism. Vertex pairing is now literal, so a
  rotated outline authors a twist; docs/GOTCHAS.md has the story.

This README used to end on "every remaining target fails on the *section or
path*". The richer section type landed — arcs, rounds, splines, Béziers,
B-splines, point sections in a loft, spline sweep paths — and moved exactly one
of the four targets that were said to wait on it, `untitled2-v1`. Probing the
other three before and after is what the change taught:

- **A pole row copies into a section when the curve is uniform.** Untitled2's
  walls are surfaces of revolution of clamped uniform degree-5 B-splines, so
  `{ bspline, degree: 5 }` takes the export's poles verbatim and both bodies
  land on Fusion's own volumes to 8e-5 and 4e-6 — closer than the export's
  B-rep does.
- **"Spline sections" can be a misreading of a fitted wall.** `v2`'s eight
  NURBS walls, evaluated at their double knots, give back a square, a circle,
  a turned square and a point: no spline in any sketch. Those sections build
  now; the smooth surface OCCT fits between them is not the one Fusion fits,
  and a smooth loft is exactly where two kernels owe each other nothing.
- **An export can be a fit of a fillet, not a sketch.** `v4`'s outline is one
  closed ~390-pole curve with the rim fillets merged into the wall, and its
  header disagrees with its file by 7%.

See `eval/targets/fusion360/README.md` for each.

## Exports not carried here

Thirteen further exports were measured and set aside, judged from the
measurements rather than the file names:

- **Four are assemblies** — 4, 8, 10 and 13 solids. Recreating an assembly is a
  different exercise; parcad models one solid per project.
- **Two report volumes that are not a part at all** — 9.8 million m³, and 76
  million m³ across 31 solids. A third is 78 m³ across nine solids, which is an
  assembly or a scale mistake in the source document.
- **Two are Form (T-spline) bodies**, 575 of 585 faces NURBS and 836 faces.
- **Two have empty timelines**: the geometry was imported, so there is no
  construction to reproduce.
- One has no solid bodies at all, only five surfaces; one export failed, leaving
  no measurements to aim at.

If one of these is wanted after all, the reason above is the thing to argue with.
