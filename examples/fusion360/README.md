# Fusion 360 recreation targets

Parts exported from the author's own Fusion 360 documents, kept here so that
recreating them in parcad's DSL is a measurable exercise rather than an
impression. Each file records the volume, area, bounding box and face types read
off the Fusion export, so a recreation either agrees with the original or does
not.

**These are seeded as real projects, in a `fusion360` folder.** `seed()` in
`app/src-tauri/src/projects.rs` walks the seed folder and keeps its structure, so
this directory arrives in the project list as a folder the user can open, edit,
rename or throw away like any other. The ones that throw show their reason in the
editor, which is the point: a target you can open and see blocked is more use
than a target that only exists in the repository.

`.seeded` records the whole relative path (`fusion360/v2`, not `v2`), so a leaf
name may repeat across folders and deleting a target still keeps it deleted.

A target that becomes a faithful recreation gets promoted up one level into
`examples/` proper, and at that point it should also earn a case in
`eval/cases/`.

The exports themselves (`.step`, `.stl`, `measurements.json`) live in
`reference/fusion/`, which is gitignored — they are the author's own designs, not
fixtures. Without them the numbers in each header are still the specification;
only a side-by-side comparison needs the files.

```bash
bun tools/run.ts examples/fusion360/retainer-v1.js > /tmp/part.json
./target/release/parcad /tmp/part.json --brep --step /tmp/part.step --out /tmp/out
```

## Recreated

| file | part | agreement with the original |
|---|---|---|
| `retainer-v1.js` | plate with a bored, drafted disc | volume +0.0028%, bbox exact, all 23 faces the same surface types |

That one needed a fix to OpenCASCADE itself
(`vendor/occt-sys/patches/0001-tangent-pinch-corner.patch`): the blend where the
disc meets the plate used to return a solid the kernel's own checker rejected,
while reporting success.

## Targets that do not build yet

| file | part | Fusion volume | faces | NURBS | blocked on |
|---|---|---|---|---|---|
| `diamond-v19.js` | diamond v19 | 196,778 mm³ | 57 | 0 | BoundaryFill |
| `spiral-v1.js` | Spiral v1 | 406,116 mm³ | 3 | 0 | Loft along a helix |
| `steam-top-4-holed-v1-v6.js` | Steam Top 4 Holed v1 v6 | 8,976 mm³ | 39 | 10 | Patch |
| `untitled2-v1.js` | Untitled2 v1 | 36,357 mm³ | 1 | 1 | Revolve of a spline profile |
| `untriangle-v3.js` | UnTriangle v3 | 31,976 mm³ | 30 | 15 | Loft |
| `v2.js` | ваза v2 | 144,242 mm³ | 9 | 8 | Loft |
| `v3.js` | шар v3 | 515,661 mm³ | 460 | 32 | Sweep |
| `v4.js` | v4 v4 | 7,375 mm³ | 3 | 1 | SplitBody and DeleteFace |

Each throws with its reason. They are deliberately not approximate solids: a stub
that returned something roughly right would measure as a part and read as
progress, which is worse than nothing.

The pattern across all of them is one thing — **parcad only makes analytic
surfaces** (planes, cylinders, cones, spheres, tori). Every blocked target needs
a freeform surface, and the ops that produce them are loft, sweep and patch. See
`docs/DSL_GAPS.md` and `docs/OP_ROADMAP.md`.

## Exports not carried here

Judged from the measurements, not from the file names.

| export | why not |
|---|---|
| `Demo-lamp-v3` | four solids (Pipe, Shell, Sphere, Revolve): an assembly |
| `For-the-New-Year-v4` | thirteen solids (Loft, Pipe, Shell): an assembly |
| `Retainer-v1` | recreated: see retainer-v1.js |
| `Spunner-v4` | the export failed; there are no measurements to aim at |
| `Submarine-v7` | ten solids driven from a Canvas image: an assembly |
| `lamp-v2` | no solid bodies at all, only five surfaces |
| `v1` | 78 m3 across nine solids - an assembly or a scale mistake, not a part |
| `v10` | Form (T-spline) body, 575 of 585 faces NURBS |
| `v14` | empty timeline: imported geometry, so there is no construction to reproduce |
| `v16` | 76 million m3 across 31 solids: not a part |
| `v3-v2` | empty timeline: imported geometry |
| `v4-v4` | eight solids, 112 faces: an assembly |
| `v5` | 9.8 million m3: not a part |
| `v7` | Form (T-spline) body, 836 faces |

Most are assemblies rather than parts, and recreating an assembly is a different
exercise — parcad models one solid per project. Two have empty timelines, meaning
the geometry was imported and there is no construction to reproduce. Three report
volumes in the millions of cubic metres, which is a scale or assembly problem in
the source document rather than a target.

If one of these is actually wanted, the reason above is the thing to argue with.

