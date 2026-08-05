// A cast machine foot: a drafted pedestal on a drafted base, bolted down
// through four holes and tapped on top for the equipment it carries.
//
// This is the first part here that could actually be cast. Every wall leans by
// a couple of degrees so the pattern releases from the sand — before `draft`
// existed the same shape had vertical walls, which is a part a foundry sends
// back. The hole diameters are not literals either: `holeFor("M10", ...)` knows
// the ISO 273 clearance is 11.0 and `{ tapped: true }` knows the M8 coarse tap
// drill is 6.8.

const baseX = 120;
const baseY = 80;
const baseT = 14;
const baseDraft = 2;     // degrees, sand casting

const padX = 60;
const padY = 40;
const rise = 46;         // top of the pedestal above the floor
const padDraft = 3;

const boltX = 90;        // bolt centres in the base
const boltY = 56;
const tapX = 36;         // tapped centres on the pad
const tapY = 20;

const rect = (x, y) => [
  [-x / 2, -y / 2],
  [x / 2, -y / 2],
  [x / 2, y / 2],
  [-x / 2, y / 2],
];

const base = extrude(rect(baseX, baseY), baseT, { draft: baseDraft }).at(0, 0, baseT / 2);

// The pedestal starts inside the base rather than on top of it: a blended union
// of two solids that only touch on a face aborts inside OCCT
// (docs/GOTCHAS.md), and a casting has a generous root radius there anyway.
const buried = 6;
const pedestalH = rise - baseT + buried;
const pedestal = extrude(rect(padX, padY), pedestalH, { draft: padDraft }).at(
  0,
  0,
  baseT - buried + pedestalH / 2,
);

const casting = union(base, pedestal, { blend: 5 }).tag("casting");

// Four M10 through the base, four M8 tapped into the pad. Both cutters enter
// the face they are placed on and run past it — that is what `holeFor` does, and
// why no example has to say so in a comment any more.
const bolts = grid(2, 2, boltX, boltY).map(([x, y]) => holeFor("M10", baseT, { through: true }).at(x, y, baseT));
const taps = grid(2, 2, tapX, tapY).map(([x, y]) => holeFor("M8", 16, { tapped: true }).at(x, y, rise));

const machined = casting.cut(...bolts, ...taps).tag("machined");

// The four bolt holes where they break out of the underside, which is the face
// that sits on the floor: a burr there rocks the machine. The tapped holes are
// blind and the pedestal top is a machined pad, so `z: "min"` reaches exactly
// the four rims meant here.
return machined
  .edges({ generatedBy: "machined", curve: "circle", role: "hole", at: { z: "min" } })
  .expect({ count: 4 })
  .chamfer(0.8)
  .tag("seat_deburr");
