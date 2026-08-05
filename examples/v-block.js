// A toolroom V-block: a 90° vee that holds round stock on its axis, whichever
// diameter it is.
//
// The vee is a square block rotated 45° and subtracted from the top face. That
// is not a shortcut — a 90° included angle *is* the corner of a square, and
// cutting it this way means the angle cannot drift when the depth changes.

const size = 50;      // cube, the usual matched-pair stock size
const veeDepth = 18;  // how far the vee bites into the top face

const body = box(size, size, size).tag("body");

// The rotated cutter must be wide enough that only its two lower faces bound
// the groove; its half-diagonal has to clear the block, so size * 1.5 is safe.
const cutter = size * 1.5;

// Rotating about Y puts the vee's axis along Y — stock lies front to back. The
// apex sits veeDepth below the top face, and the cutter's own half-diagonal
// (cutter * sqrt(2) / 2) is how far the apex is below its centre.
const vee = box(cutter, size * 2, cutter)
  .rotate("y", 45)
  .at(0, 0, size / 2 - veeDepth + (cutter * Math.SQRT2) / 2)
  .tag("vee");

// Through hole for the clamp screw, plus a cross slot the clamp strap sits in.
const clampHole = cylinder(4.25, size * 2).rotate("x", 90).tag("clamp_hole");
const strapSlot = box(size * 2, 12, 6)
  .at(0, 0, -size / 2 + 3)
  .tag("strap_slot");

const machined = body.cut(vee, clampHole, strapSlot).tag("machined");

// Break every long edge on the top face: the two lips the vee cuts, and the
// two outside edges of the block. Four, not two — ">Z and |Y" says "at the top
// of the block, running along Y", and the outside edges are as much at the top
// as the vee lips are. These are the edges that get handled and the ones that
// mark up a workpiece, so all four wanting a break is the right answer here.
return machined
  .edges(">Z and |Y")
  .expect({ count: 4 })
  .chamfer(1)
  .tag("vee_lip_break");
