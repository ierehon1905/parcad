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

return body
  .cut(
    ...grid(2, 2, 50, 40).map(([x, y]) => hole.at(x, y)),
    { blend: 0.8 },
  )
  .tag("drilled");
