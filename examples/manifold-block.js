// A hydraulic manifold block: a solid with cross-drilled galleries that meet
// inside it, plus the plugged drilling access every real manifold has.
//
// The interesting property of a manifold is that its function lives in the
// negative space. Nothing here is a feature on the surface — the part is a
// rectangle of metal and a set of intersecting bores, and whether it works
// depends on whether those bores actually meet.

const w = 80, d = 50, h = 40;
const gallery = 8;      // main flow bore
const port = 10;        // threaded port drill
const mountHole = 6.6;  // clearance for M6

const body = box(w, d, h).tag("body");

// The long gallery runs the length of the block on the centre plane, drilled
// in from one end. In a real block the far end is plugged; here it is simply
// drilled through, which is the honest representation of the cut.
const mainBore = cylinder(gallery / 2, w * 1.2).rotate("y", 90).tag("main_bore");

// Two ports drop from the top face and intersect the gallery. Their depth is
// what makes them meet it, and it has to overshoot: a port that bottoms on the
// gallery's centreline is tangent to the bore's lower half, which leaves a
// flat floor with its own rim instead of an opening. One millimetre past the
// far wall of the gallery is a real, unambiguous intersection.
const portDepth = h / 2 + gallery / 2 + 1;
const dropPort = cylinder(port / 2, portDepth * 2);
const ports = union(
  dropPort.at(-22, 0, h / 2),
  dropPort.at(22, 0, h / 2),
).tag("ports");

// A cross gallery on the other axis, meeting the main bore at the centre.
const crossBore = cylinder(gallery / 2, d * 1.2).rotate("x", 90).tag("cross_bore");

const mounting = repeat(
  cylinder(mountHole / 2, h * 2),
  grid(2, 2, w - 20, d - 20),
);

const drilled = body
  .cut(mainBore, ports, crossBore, mounting)
  .tag("drilled");

// Every rim that opens onto the top face gets a chamfer: two ports and four
// mounting holes. Port chamfers are functional here — a sealing fitting needs
// the lead-in — so this is not just deburring.
//
// `at: { z: "max" }` and not `adjacentTo: { faceNormal: "+z" }`, which is what
// the other examples use. A rim is "adjacent to" its own cylindrical wall as
// well as the flat face it sits in, and a bore drilled along X reports that
// wall as +Z-facing, so the face-normal form also picks up both end rims of
// the main gallery. See docs/DSL_GAPS.md; on a part made mostly of cross
// drillings, position is the selector that says what was meant.
return drilled
  .edges({
    generatedBy: "drilled",
    curve: "circle",
    role: "hole",
    at: { z: "max" },
  })
  .expect({ count: 6 })
  .chamfer(1)
  .tag("top_face_lead_ins");
