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
| `untitled2-v1.js` | Untitled2 v1 | 36,357 mm³ | 1 | 1 | a spline in a revolve section; also two solids |
| `v2.js` | ваза v2 | 144,242 mm³ | 9 | 8 | spline loft sections; parcad's loft takes polygons |
| `v3.js` | шар v3 | 515,661 mm³ | 460 | 32 | a sweep around a sphere; parcad's sweep follows runs and circular bends |
| `v4.js` | v4 | 7,375 mm³ | 3 | 1 | a spline outline in an extrude, then SplitBody |

Four wait on curves in sections and paths — splines in the profile type, and a
path that is not runs and circular bends: `untitled2-v1`, `v2`, `v3`, `v4`.
`spiral-v1` waits on loft sections that tilt as they rise and on parts with more
than one solid, which `untitled2-v1` also needs. `steam-top-4-holed-v1-v6`
waits on Patch, which is out of scope by decision.
