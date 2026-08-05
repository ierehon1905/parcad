// A slip-on pipe flange, dimensioned after ASME B16.5 class 150, NPS 2.
//
// Nominal dimensions, all millimetres: OD 152.4, flange thickness 19.1,
// hub OD 92.1, length through hub 25.4 measured from the back face, bore 60.3,
// four bolt holes of 19.1 on a 120.7 bolt circle. The holes straddle the
// centrelines, which is the part of the standard people get wrong: they sit at
// 45°, not at 0°.
//
// `polar()` is the rotational counterpart to `grid()`, and `straddle` is the
// convention itself: the holes sit half a step off the centrelines. Written as
// arithmetic it was a `+ 0.5` nobody could check against a drawing.

const od = 152.4;
const thickness = 19.1;
const bore = 60.3;
const hubOd = 92.1;
const throughHub = 25.4;   // back face to top of hub
const boltCircle = 120.7;
const boltDia = 19.1;
const bolts = 4;

// The flange plate straddles z = 0; the hub grows off its top face, so the
// back face (the one that gets faced flat) stays at z = -thickness / 2.
const plate = cylinder(od / 2, thickness).tag("plate");

// The hub is modelled buried to the plate's mid-plane rather than standing on
// its top face. Two coaxial cylinders that meet exactly on a face abort inside
// OCCT when the union is blended — at any radius, including 1 mm — and an
// overlap is both the fix and what a casting actually is. See docs/DSL_GAPS.md.
const hubTop = throughHub - thickness / 2;
const hub = cylinder(hubOd / 2, hubTop)
  .at(0, 0, hubTop / 2)
  .tag("hub");

// A small blend at the hub root: a sharp internal corner there is a stress
// riser, and every real casting has a radius at it.
const body = union(plate, hub, { blend: 3 }).tag("body");

// Overlength on purpose. A cutting tool that ends exactly on a face leaves a
// zero-thickness sliver that booleans handle badly — extend it past both ends.
const throughBore = cylinder(bore / 2, throughHub * 4).tag("bore");

const boltHoles = polar(bolts, boltCircle / 2, { straddle: true });

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
