// An extruded-profile heat sink, 60 x 60, with a plain fin array.
//
// Fin pitch and thickness are the whole design: closer fins add surface area
// but choke natural convection, and 2 mm walls at a 6 mm pitch is the usual
// compromise for an extrusion. The two mounting holes are on the diagonal of
// a 30 mm square, which is the common pattern for clamping to a TO-247.

const plan = 60;
const baseT = 5;
const finT = 2;
const finH = 25;
const pitch = 6;
const fins = 9;

const base = box(plan, plan, baseT).at(0, 0, baseT / 2).tag("base");

// One fin, reused at every station. Because shapes are values, this is a
// single node in the graph with nine placements — not nine cylinders' worth
// of duplicated intent.
const fin = box(finT, plan, finH + baseT).at(0, 0, (finH + baseT) / 2);

const stations = Array.from({ length: fins }, (_, i) => [
  (i - (fins - 1) / 2) * pitch,
  0,
]);

const body = union(base, repeat(fin, stations)).tag("body");

const mount = cylinder(1.7, baseT * 4);
const drilled = body
  .cut(repeat(mount, [[-15, -15], [15, 15]]))
  .tag("drilled");

// Only the underside rims: that face beds against the device and must not
// carry a burr. The fin roots are untouched — a fillet there would be nice
// for casting but this profile is extruded, and the die makes that corner.
return drilled
  .edges({
    generatedBy: "drilled",
    curve: "circle",
    role: "hole",
    adjacentTo: { faceNormal: "-z" },
  })
  .expect({ count: 2 })
  .chamfer(0.4)
  .tag("seat_face_deburr");
