// A slip-on pipe flange, dimensioned after ASME B16.5 class 150, NPS 2.
//
// Nominal dimensions, all millimetres: OD 152.4, flange thickness 19.1,
// hub OD 92.1, length through hub 25.4 measured from the back face, bore 60.3,
// four bolt holes of 19.1 on a 120.7 bolt circle. The holes straddle the
// centrelines, which is the part of the standard people get wrong: they sit at
// 45°, not at 0°.
//
// The DSL has no polar array, so the bolt circle is written out with Math.
// See docs/DSL_GAPS.md — a `polar()` helper next to `grid()` would remove
// this boilerplate from every rotationally symmetric part.

const od = 152.4;
const thickness = 19.1;
const bore = 60.3;
const hubOd = 92.1;
const throughHub = 25.4;           // back face to top of hub
const hubHeight = throughHub - thickness;
const boltCircle = 120.7;
const boltDia = 19.1;
const bolts = 4;

// The flange plate straddles z = 0; the hub grows off its top face, so the
// back face (the one that gets faced flat) stays at z = -thickness / 2.
const plate = cylinder(od / 2, thickness).tag("plate");

const hub = cylinder(hubOd / 2, hubHeight)
  .at(0, 0, (thickness + hubHeight) / 2)
  .tag("hub");

// A small blend at the hub root: a sharp internal corner there is a stress
// riser, and every real casting has a radius at it.
const body = union(plate, hub, { blend: 3 }).tag("body");

// Overlength on purpose. A cutting tool that ends exactly on a face leaves a
// zero-thickness sliver that booleans handle badly — extend it past both ends.
const throughBore = cylinder(bore / 2, throughHub * 4).tag("bore");

const boltHoles = Array.from({ length: bolts }, (_, i) => {
  // 45° offset so no hole lands on a centreline, per the standard.
  const angle = ((i + 0.5) / bolts) * Math.PI * 2;
  return [
    Math.cos(angle) * (boltCircle / 2),
    Math.sin(angle) * (boltCircle / 2),
  ];
});

// One tagged cut, not two. `generatedBy` names a single operation, so bore and
// bolt holes have to be drilled by the same node for one selector to reach all
// five rims — splitting the cut in two silently halves what the query matches.
const drilled = body
  .cut(throughBore, repeat(cylinder(boltDia / 2, thickness * 4), boltHoles))
  .tag("drilled");

// Break the bore rim where a gasket seats. `role: "hole"` is what keeps the
// hub's outside rim — same curve, same face normal — out of the selection.
return drilled
  .edges({
    generatedBy: "drilled",
    curve: "circle",
    role: "hole",
    adjacentTo: { faceNormal: "-z" },
  })
  .expect({ count: 5 })
  .chamfer(1.5)
  .tag("back_face_breaks");
