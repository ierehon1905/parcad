// bracket-with-boss.js with materials on nested nodes, a cutter's included.
// Appearance is presentational: every number and every `.expect({ count })`
// must match that case exactly.

const t = 8;
const w = 80;
const d = 60;
const wallH = 40;

const steel = { color: "#8a9099", metalness: 0.9, roughness: 0.35 };

const plate = box(w, d, t).tag("plate").material(steel);

const wall = box(t, d, wallH)
  .at(-(w - t) / 2, 0, (wallH + t) / 2 - t / 2)
  .tag("wall");

const body = union(plate, wall, { blend: 6 }).tag("body").material({ color: "#3a6ea5" });

const boss = union(body, cylinder(5, 6).at(0, 0, t / 2 + 3 - 0.001).material({ color: "#c33" })).tag("boss");

const hole = cylinder(3, t * 4).material({ color: "#ff00ff" });

const drilled = boss
  .cut(...grid(2, 2, 36, 40).map(([x, y]) => hole.at(x, y)))
  .tag("mount_holes")
  .material({ color: "#e0b040", roughness: 0.8 });

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

const chamferedBase = roundedHoles
  .edges("<X and <Z and |Y")
  .expect({ count: 1 })
  .chamfer(1)
  .tag("left_base_chamfer");

return chamferedBase
  .vertices(">X and >Y and <Z")
  .expect({ count: 1 })
  .fillet(2)
  .tag("outer_corner_round");
