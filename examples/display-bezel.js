// An instrument fascia: a display module drops into a milled seat in the front
// face, and looks out through an aperture cut all the way to the back.
//
// Every other part here cuts *through* something, so only one end of a cutter
// is ever in question — "a tool ending exactly on a face leaves a zero-thickness
// sliver", which is the exit. A recess is the other half of that rule: it
// enters a face and stops inside, and the entry needs the same overshoot. A
// seat cutter whose outer face lands four microns short of the face it enters
// makes no seat at all — it makes a sealed void under a feather edge, and the
// part still measures watertight, right size, right shape. docs/GOTCHAS.md,
// "A cut needs overlength at both ends", has the measurements.
//
// `over` is that overshoot, and it appears at the entry of the seat and at both
// ends of the aperture. It is 0.5 mm because that is what `holeFor` uses.

const panelW = 120;
const panelD = 80;
const panelT = 6;

const seatW = 70;        // the module's outline, plus assembly clearance
const seatD = 52;
const seatDepth = 1.6;   // the module's bezel, so its face finishes flush

const apertureW = 58;    // the active area, opened out to clear the glass
const apertureD = 44;

const endMill = 3;       // a 6 mm cutter: no milled pocket has sharper corners
const boltX = 104;
const boltY = 64;

const over = 0.5;

const panel = box(panelW, panelD, panelT).at(0, 0, panelT / 2).tag("panel");

// A milled pocket is not a box: its corners carry the cutter's radius. Two
// crossed slabs and four corner cylinders is that shape exactly, which is the
// same reason `pillow-block.js` builds its slots out of three primitives.
const pocket = (w, d, h) =>
  union(
    box(w - 2 * endMill, d, h),
    box(w, d - 2 * endMill, h),
    ...grid(2, 2, w - 2 * endMill, d - 2 * endMill).map(([x, y]) =>
      cylinder(endMill, h).at(x, y),
    ),
  );

// The seat stops inside the panel, so the entry is the end that has to
// overshoot: the cutter is `over` taller than the seat is deep and stands that
// much proud of the front face. The floor lands where it was going to anyway.
const seat = pocket(seatW, seatD, seatDepth + over)
  .at(0, 0, panelT + over - (seatDepth + over) / 2)
  .tag("seat");

// The aperture goes all the way through, so it overshoots at both ends.
const aperture = pocket(apertureW, apertureD, panelT + 2 * over)
  .at(0, 0, panelT / 2)
  .tag("aperture");

const bolts = grid(2, 2, boltX, boltY).map(([x, y]) =>
  holeFor("M4", panelT, { through: true }).at(x, y, panelT),
);

// The seat is cut in its own tagged step so the lead-in below can name it.
// Rolled into one cut with the bolt holes, `generatedBy` would reach their rims
// as well and the count would stop meaning anything — docs/GOTCHAS.md,
// "`generatedBy` names the cut, not the tool".
const seated = panel.cut(seat).tag("bezel_seat");
const machined = seated.cut(aperture, ...bolts).tag("machined");

// Break the seat's rim, which is the edge the module drops past. Eight of them:
// four straight sides and the four corner arcs the end mill leaves behind.
//
// This selector is also what fails if `over` is ever taken away: with the seat
// cutter stopping short of the front face there is no rim, `bezel_seat` tracks
// no edge there, and the build stops by name instead of shipping a void.
return machined
  .edges({ generatedBy: "bezel_seat", at: { z: "max" } })
  .expect({ count: 8 })
  .chamfer(0.4)
  .tag("seat_lead_in");
