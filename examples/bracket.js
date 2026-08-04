// parcad — everything is millimetres, Z is up.
// Primitives are centred on the origin; place them with .at(x, y, z).
// The script must return a shape.

const t = 8;          // plate thickness
const w = 80;         // plate width
const d = 60;         // plate depth
const wallH = 40;     // upright height

const plate = box(w, d, t).tag("plate");

const wall = box(t, d, wallH)
  .at(-(w - t) / 2, 0, (wallH + t) / 2 - t / 2)
  .tag("wall");

// A blended union rounds the seam. The blend is the fillet.
const body = union(plate, wall, { blend: 6 }).tag("body");

const hole = cylinder(3, t * 4);

const drilled = body
  .cut(...grid(2, 2, 36, 40).map(([x, y]) => hole.at(x, y)))
  .tag("mount_holes");

// Every selected edge is a circular hole rim bordering the upward-facing top face.
// The same query keeps selecting just the top rim if a hole moves or more
// holes are added; the lower rims and vertical walls are not selected.
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

// This names a corner rather than a kernel vertex ID. The exact backend expands
// it to its three incident edges, then makes one rolling-ball corner fillet.
return roundedHoles
  .vertices(">X and >Y and <Z")
  .expect({ count: 1 })
  .fillet(2)
  .tag("outer_corner_round");
