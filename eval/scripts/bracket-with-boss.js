// The selector-drift case. This is examples/bracket.js with one unrelated
// cylindrical boss unioned onto the plate *before* the three edge treatments,
// so every selector resolves against changed topology.
//
// The assertion is the three `.expect({ count })` calls, which the exact
// backend checks before it modifies the solid. If a boss can turn the
// four-hole-rim fillet into a five-edge fillet, this case goes red at the
// selector, not at a volume that someone has to interpret. That is the Phase 2
// exit criterion from docs/AI_CAD_PLATFORM.md, made executable.

const t = 8;
const w = 80;
const d = 60;
const wallH = 40;

const plate = box(w, d, t).tag("plate");

const wall = box(t, d, wallH)
  .at(-(w - t) / 2, 0, (wallH + t) / 2 - t / 2)
  .tag("wall");

const body = union(plate, wall, { blend: 6 }).tag("body");

// The perturbation: a solid boss with its own circular rim, sitting on the top
// face, related to nothing the selectors below name.
//
// Centred on the origin deliberately. The mounting holes sit at (±18, ±20), so
// a 5 mm boss here clears the nearest by ~19 mm. An earlier draft put it at
// (20, -20), where it swallowed a hole and the rim count legitimately fell to
// three — a wrong fixture, not selector drift, and the harness said so.
const boss = union(body, cylinder(5, 6).at(0, 0, t / 2 + 3 - 0.001)).tag("boss");

const hole = cylinder(3, t * 4);

const drilled = boss
  .cut(...grid(2, 2, 36, 40).map(([x, y]) => hole.at(x, y)))
  .tag("mount_holes");

// Still four, because provenance and role exclude the boss rim.
const roundedHoles = drilled
  .edges({
    generatedBy: "mount_holes",
    curve: "circle",
    role: "hole",
    adjacentTo: { faceNormal: "+z" },
  })
  .expect({ count: 4 })
  .fillet(0.8)
  .tag("top_hole_rims");

// Still one: the boss is nowhere near the leftmost underside edge.
const chamferedBase = roundedHoles
  .edges("<X and <Z and |Y")
  .expect({ count: 1 })
  .chamfer(1)
  .tag("left_base_chamfer");

// Still one: the boss is inside the plate footprint and above it, so it moves
// no extreme that this corner is defined by.
return chamferedBase
  .vertices(">X and >Y and <Z")
  .expect({ count: 1 })
  .fillet(2)
  .tag("outer_corner_round");
