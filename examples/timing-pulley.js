// A 20-tooth GT2 timing pulley for 6 mm belt, on a 5 mm motor shaft.
//
// APPROXIMATE, and deliberately so. A real GT2 tooth is a curvilinear profile
// defined by the belt standard; here each groove is a cylinder on the pitch
// circle, which is the right depth and pitch but not the right flank shape.
// Printed, it runs; as a mould tool, it does not. The DSL has no way to sweep
// an authored 2D profile around an axis (docs/DSL_GAPS.md), and inventing a
// "close enough" tooth in the kernel would be exactly the approximation this
// project refuses to make silently — so it is called out here instead.

const teeth = 20;
const pitch = 2;                          // GT2: 2 mm belt pitch
const pitchDia = (teeth * pitch) / Math.PI; // 12.732 for 20 teeth
const beltWidth = 6;
const bodyH = beltWidth + 2;              // belt width plus a little
const flangeDia = pitchDia + 5;
const flangeT = 1.2;
const bore = 5;
const grubDia = 3.3;                      // tap drill for M4 grub screw
const hubDia = bore + 8;                  // 4 mm of wall each side to tap into
const hubH = 8;                           // how far the hub stands off the flange

const body = cylinder(pitchDia / 2 + 0.5, bodyH).tag("body");

// The tooth grooves. Each is a cylinder standing on the pitch circle, so the
// groove depth follows from where the pitch circle sits — that is the one
// dimension a belt actually cares about.
const groove = cylinder(0.62, beltWidth * 3).at(pitchDia / 2, 0);
const grooves = around(groove, teeth).tag("grooves");
const toothed = body.cut(grooves).tag("toothed");

// Flanges keep the belt on; the hub carries the grub screw. All three are
// unioned onto the body, which is one solid in the real part too — these
// pulleys are turned from one blank.
//
// They go on *after* the teeth are cut. Each groove cylinder is longer than the
// body so that it clears both ends, so a flange already in place would be
// drilled through by all twenty of them. Unioned afterwards it fills the groove
// ends instead, and the teeth stop where the belt does.
const flange = cylinder(flangeDia / 2, flangeT);
const blank = union(
  toothed,
  flange.at(0, 0, (bodyH - flangeT) / 2),
  flange.at(0, 0, -(bodyH - flangeT) / 2),
  // Buried 1 mm into the body, so the union has an overlap to work with rather
  // than two faces that exactly touch.
  cylinder(hubDia / 2, hubH + 1).at(0, 0, -(bodyH + hubH) / 2 + 0.5),
).tag("blank");

// The bore runs the whole height, hub included.
const shaftBore = cylinder(bore / 2, (bodyH + hubH) * 2).tag("bore");

// One radial grub screw into the bore, through the hub. A 1.2 mm flange has
// nowhere to put a thread and the belt path cannot be crossed, so the hub is
// the only place it can go — which is where a real pulley puts it. The tool
// starts inside the bore and stops outside the hub: centred on the axis, as a
// primitive is by default, it would drill straight out the far side too.
const grub = cylinder(grubDia / 2, 6)
  .rotate("y", 90)
  .at(hubDia / 2 - 1.5, 0, -(bodyH + hubH) / 2)
  .tag("grub");

// Each cut is its own tagged step. `generatedBy` names the node that made an
// edge, so one cut carrying several tools gives every tool the same name — and
// then the query below cannot tell the bore's rim from the twenty groove rims.
const machined = blank.cut(shaftBore).tag("bored").cut(grub).tag("machined");

// One lead-in, on the bottom of the bore. Position, not face normal: the grub
// screw hole is drilled across the part, and a rim reports its own cylindrical
// wall as adjacent, so a face-normal query would catch it too.
return machined
  .edges({ generatedBy: "bored", curve: "circle", role: "hole", at: { z: "min" } })
  .expect({ count: 1 })
  .chamfer(0.4)
  .tag("bore_lead_in");
