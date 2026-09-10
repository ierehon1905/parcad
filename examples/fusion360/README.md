# Fusion 360 recreation targets

Parts exported from the reference corpus of Fusion 360 documents, kept here so that
recreating them in parcad's DSL is a measurable exercise rather than an
impression. Each file records the volume, area, bounding box and face types read
off the Fusion export, so a recreation either agrees with the original or does
not.

**These are seeded as real projects, in a `fusion360` folder.** `seed()` in
`app/src-tauri/src/projects.rs` keeps the structure, so this arrives in the
project list as an ordinary folder. The ones that throw show their reason in the
editor: a target you can open and see blocked is more use than one that only
exists in the repository. `.seeded` records the whole relative path
(`fusion360/v2`, not `v2`), so a leaf name may repeat and a deleted target stays
deleted.

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
user's copy, because that is the file that was wrong. The real fix is that
recreation targets should not be seeded as ordinary parts at all; that is a
change to `seed()` and is not written yet.

A target that becomes a faithful recreation gets promoted up one level into
`examples/` proper, and at that point it should also earn a case in
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
| `retainer-v1.js` | plate with a bored, drafted disc | volume +0.0028%, bbox exact, all 23 faces the same surface types |
| `../diamond-v19.js` | round brilliant, 57 planar facets | volume, area and bbox agree to every published digit; the same 57 planes — promoted up into `examples/`, measured by `eval/cases/diamond-v19.json` |
| `untriangle-v3.js` | impossible-triangle ring of quarter-twisted bars (the export's body, Body12) | volume +0.00024%, area +0.00005%, bbox exact, the same 30 faces (18 plane, 12 nurbs); every twisted wall is the *identical* bilinear surface, corners matched to 1.2e-4 mm — measured by `eval/cases/untriangle-v3.json` |

The diamond needed no new op at all. Fusion built it with BoundaryFill, but the
solid is convex, so it *is* the intersection of its 57 facet half-spaces — an
intersection of rotated boxes, exact in both backends. The blocker recorded in
its header was the construction Fusion happened to use, not what the shape
requires; the audit that found this is the reason each remaining target below
names the *geometry* it is blocked on rather than the Fusion feature list.

That one needed a fix to OpenCASCADE itself
(`vendor/occt-sys/patches/0001-tangent-pinch-corner.patch`): the blend where the
disc meets the plate used to return a solid the kernel's own checker rejected,
while reporting success.

## Targets that do not build yet

| file | part | Fusion volume | faces | NURBS | blocked on |
|---|---|---|---|---|---|
| `spiral-v1.js` | Spiral v1 | 406,116 mm³ | 3 | 0 | a loft whose sections rotate as they rise; also two solids |
| `steam-top-4-holed-v1-v6.js` | Steam Top 4 Holed v1 v6 | 8,976 mm³ | 39 | 10 | Patch — out by decision: surface logic |
| `untitled2-v1.js` | Untitled2 v1 | 36,357 mm³ | 1 | 1 | a spline in a revolve section; also two solids |
| `v2.js` | ваза v2 | 144,242 mm³ | 9 | 8 | spline loft sections; parcad's loft takes polygons |
| `v3.js` | шар v3 | 515,661 mm³ | 460 | 32 | a sweep around a sphere; parcad's sweep follows runs and circular bends |
| `v4.js` | v4 | 7,375 mm³ | 3 | 1 | a spline outline in an extrude, then SplitBody |

Each throws with its reason. They are deliberately not approximate solids: a stub
that returned something roughly right would measure as a part and read as
progress, which is worse than nothing.

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

Every remaining target above fails on the *section or path*, not on the op:
four of the six carry spline sketch geometry the section type cannot hold, one
rotates its sections up a helix, and one needs Patch, which stays out by
decision. The next enabling change is a richer section type — arcs first,
splines after — not another sweep or loft variant. See `docs/DSL_GAPS.md` and
`docs/OP_ROADMAP.md`.

A loft or sweep in a part also has a measured cost on the agent side: those
nodes have no exact distance field, the implicit backend refuses them by name,
and every capability that runs on the field — probes, wall thickness,
raymarched renders and sections — is unavailable for that part. The B-rep
still builds, measures and exports it; what is lost is inspection without
looking.

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
