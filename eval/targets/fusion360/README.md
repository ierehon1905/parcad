# Fusion 360 recreation targets that do not build yet

Real Fusion 360 documents, exported and measured, whose recreation needs
geometry parcad cannot make. Each script records the export's volume, bounding
box and face types in its header and throws naming what it is blocked on. They
are deliberately not approximate solids: a stub that returned something roughly
right would measure as a part and read as progress, which is worse than nothing.

They are not in `examples/`, so they are never seeded into anyone's project
folder. When one builds faithfully it moves to `examples/fusion360/` and earns a
case in `eval/cases/`. The exports themselves live in `reference/fusion/`, which
is gitignored; `examples/fusion360/README.md` shows how to probe one.

## What each is blocked on

| file | part | Fusion volume | faces | NURBS | blocked on |
|---|---|---|---|---|---|
| `spiral-v1.js` | Spiral v1 | 406,116 mm³ | 3 | 0 | a loft whose sections rotate as they rise; also two solids |
| `steam-top-4-holed-v1-v6.js` | Steam Top 4 Holed v1 v6 | 8,976 mm³ | 39 | 10 | Patch — out by decision: surface logic |
| `v2.js` | ваза v2 | 144,242 mm³ | 9 | 8 | builds since arcs and point sections; OCCT's smooth fit is +0.07% volume but −2% area and 0.75 mm narrower than Fusion's |
| `v3.js` | шар v3 | 515,661 mm³ | 460 | 32 | reverse-engineering a 460-face ornament on a ball; its paths are circles, not splines |
| `v4.js` | v4 | 7,375 mm³ | 3 | 1 | the export's outline is a ~390-pole fit with fillets merged into the wall, and its header disagrees with its own file by 7% |

**Curves in sections and paths (2026-09-15) moved one of the four targets that
waited on them.** `untitled2-v1` is recreated — both bodies, from the export's
own degree-5 pole rows — and lives in `examples/fusion360/`. The other three
turned out, once probed, not to be waiting on curves: `v2`'s sections are a
square, a circle, a turned square and a point, which now build, but the smooth
wall between them is a different fit; `v3` is an ornament of cones and tori;
`v4`'s export is not the sketch it was drawn from. Each header records what the
probe found. `spiral-v1` waits on loft sections that tilt as they rise.
`steam-top-4-holed-v1-v6` waits on Patch, which is out of scope by decision.
