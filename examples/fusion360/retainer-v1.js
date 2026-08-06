// Retainer v1 — recreated from a Fusion 360 export.
//
// The one Fusion part in here that is a faithful recreation rather than a
// target: it builds, it is watertight, and it agrees with the original.
//
//   volume   143829.66 mm^3 against Fusion's 143825.63  (+0.0028%)
//   bounding box  59.49 x 148.13 x 36.66 mm             (exact)
//   faces    23, and the same 23: 14 plane, 6 cylinder, 2 cone, 1 torus
//
// Ground truth is the STEP in reference/fusion/Retainer-v1. Every number below
// was read off the B-rep in it, not scaled off a drawing. The face count is 23
// rather than the 25 Fusion's own API reports, because that is what its STEP
// export contains and STEP is what both sides were measured from.
//
// It needed a fix to OpenCASCADE to get here — see
// vendor/occt-sys/patches/0001-tangent-pinch-corner.patch. Before that the
// blend where the disc meets the plate returned a solid the kernel's own
// checker rejected.
//
// Axes are remapped from Fusion, whose plate normal was +Y: parcad (x, y, z) =
// Fusion (x, -z, y). That puts the plate normal on +Z, so every hole in the
// part is a plain Z cylinder and nothing needs rotating.

const W = 59.49;        // plate width, and the disc diameter — they are equal
const T = 13.314;       // plate thickness
const PLATE_L = 118.381; // to the disc centre; the disc caps the rest
const R_DISC = W / 2;   // 29.745
const H_DISC = 36.664;  // disc rises this far off the back face

// Disc bore, from the back: a drafted counterbore, then a straight bore out
// the front. The 3.70 degrees is the Draft feature in the Fusion timeline.
// The cone opens toward the back face, not away from it: the STEP puts radius
// 15 at the far end with the axis pointing back, so the mouth at z = 0 is the
// wide end. The STL's back face agrees — its inner boundary sits at 16.701,
// which is this radius and not the 13.3 a cone drafted the other way implies.
const DRAFT_TOP_R = 15;
const DRAFT_DEPTH = 26.3;
const DRAFT_DEG = 3.70003;
const DRAFT_BOTTOM_R = DRAFT_TOP_R + DRAFT_DEPTH * Math.tan((DRAFT_DEG * Math.PI) / 180);
const BORE_R = 10;
const BORE_CHAMFER = 1;

// The threaded hole is modelled at its pitch diameter, which is how Fusion
// leaves a cosmetic thread: no helix reached the B-rep, so this is a plain
// cylinder and the recreation is exact rather than approximate.
const THREAD_R = 7.51775;
const THREAD_X = 29.4901;
const THREAD_Y = 64;

// Bayonet slot: a channel down from the top face, turning left into a pocket.
const SLOT_W = 10.2;
const SLOT_X0 = 39.29;
const SLOT_X1 = 49.49;
const CHANNEL_END_Y = 37.926;   // where the pocket's lower wall lies
const POCKET_Y0 = 27.726;
const POCKET_Y1 = 37.926;
const POCKET_X_END = 27.49;     // centre of the round end, radius SLOT_W / 2
const TURN_R = 12;              // fillet on the outer corner of the turn
const INNER_R = 1.8;            // and on the inner corner
const LEAD_IN = 2.5;            // 45 degree chamfer at the slot mouth

// The threaded hole stays in the final Boolean below. Cutting it here, before
// the blended union, corrupts the result: the part comes out 61.06 mm wide
// against a 59.49 mm plate and loses 85% of its volume. A blend over a shape
// that already has a hole through it is the trigger; both work alone.
const plate = box(W, PLATE_L, T)
  .at(W / 2, PLATE_L / 2, T / 2)
  .tag("plate");

const disc = cylinder(R_DISC, H_DISC)
  .at(R_DISC, PLATE_L, H_DISC / 2)
  .tag("disc");

// The seam only exists where the disc wall meets the plate's front face, which
// is exactly where the reference has its r2 torus.
const stock = union(plate, disc, { blend: 2 }).tag("stock");

// Cut deeper than the disc on both ends; below the draft the counterbore is
// wider than the bore, so the overlap removes nothing extra.
const bore = cylinder(BORE_R, H_DISC * 2).at(R_DISC, PLATE_L, H_DISC);

const draftedBore = cone(DRAFT_BOTTOM_R, DRAFT_TOP_R, DRAFT_DEPTH)
  .at(R_DISC, PLATE_L, DRAFT_DEPTH / 2);

const channel = box(SLOT_X1 - SLOT_X0, CHANNEL_END_Y, T * 3)
  .at((SLOT_X0 + SLOT_X1) / 2, CHANNEL_END_Y / 2, T / 2);

const pocket = box(SLOT_X1 - POCKET_X_END, SLOT_W, T * 3)
  .at((POCKET_X_END + SLOT_X1) / 2, (POCKET_Y0 + POCKET_Y1) / 2, T / 2);

const pocketEnd = cylinder(SLOT_W / 2, T * 3)
  .at(POCKET_X_END, (POCKET_Y0 + POCKET_Y1) / 2, T / 2);

// Both corner radii of the turn are built into the cutter rather than applied
// as edge treatments. They are interior edges of a pocket, and the selector
// grammar reaches document extrema only — there is no term that names the
// vertical edge at (49.49, 37.93) without naming the other three corners too.
// See the note in DSL_GAPS.md. The geometry below is exact, not a stand-in:
// a box minus the fillet cylinder is precisely the material the arc leaves.
const TURN_CX = 37.4901;
const TURN_CY = 26;

const outerCorner = box(TURN_R, POCKET_Y1 - TURN_CY, T * 3)
  .at(TURN_CX + TURN_R / 2, (TURN_CY + POCKET_Y1) / 2, T / 2)
  .cut(cylinder(TURN_R, T * 4).at(TURN_CX, TURN_CY, T / 2));

const innerCorner = box(INNER_R, INNER_R, T * 3)
  .at(TURN_CX + INNER_R / 2, TURN_CY + INNER_R / 2, T / 2)
  .cut(cylinder(INNER_R, T * 4).at(TURN_CX, TURN_CY, T / 2));

const slot = union(channel, pocket, pocketEnd)
  .cut(outerCorner)
  .union(innerCorner);

// A right triangle swept through the thickness is the lead-in at each side of
// the slot mouth. The outline carries its own X and Y, so only the centred Z
// needs placing. These stay geometry rather than edge treatments because the
// edges they chamfer belong to the slot's own cut, and a treatment would have
// to name them after the fact.
const leadInLeft = extrude(
  [[SLOT_X0 - LEAD_IN, 0], [SLOT_X0, 0], [SLOT_X0, LEAD_IN]],
  T * 3,
).at(0, 0, T / 2);

const leadInRight = extrude(
  [[SLOT_X1, 0], [SLOT_X1 + LEAD_IN, 0], [SLOT_X1, LEAD_IN]],
  T * 3,
).at(0, 0, T / 2);

const threadHole = cylinder(THREAD_R, T * 3).at(THREAD_X, THREAD_Y, T / 2);

const drilled = stock
  .cut(bore, draftedBore, slot, leadInLeft, leadInRight)
  .tag("cuts");

// With the threaded hole still to come, the bore's is the only hole rim in
// this lineage on an upward face — the drafted bore's other rim faces down.
// Cutting the thread first would put a second +z rim in the same lineage and
// this selector would take both.
const chamfered = drilled
  .edges({ generatedBy: "cuts", curve: "circle", role: "hole", adjacentTo: { faceNormal: "+z" } })
  .expect({ count: 1 })
  .chamfer(BORE_CHAMFER)
  .tag("bore_rim");

return chamfered.cut(threadHole).tag("thread");
