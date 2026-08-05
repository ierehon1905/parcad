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
const grubDia = 3.3;                      // clearance drill for M4 grub screw

const body = cylinder(pitchDia / 2 + 0.5, bodyH).tag("body");

// Flanges keep the belt on. Both are unioned onto the body, which is one solid
// in the real part too — these pulleys are turned from one blank.
const flange = cylinder(flangeDia / 2, flangeT);
const blank = union(
  body,
  flange.at(0, 0, (bodyH - flangeT) / 2),
  flange.at(0, 0, -(bodyH - flangeT) / 2),
).tag("blank");

// The tooth grooves. Each is a cylinder standing on the pitch circle, so the
// groove depth follows from where the pitch circle sits — that is the one
// dimension a belt actually cares about.
const groove = cylinder(0.62, beltWidth * 3);
const grooves = union(
  ...Array.from({ length: teeth }, (_, i) => {
    const angle = (i / teeth) * Math.PI * 2;
    return groove.at(
      Math.cos(angle) * (pitchDia / 2),
      Math.sin(angle) * (pitchDia / 2),
    );
  }),
).tag("grooves");

// The bore, and one radial grub screw through a flange into it.
const shaftBore = cylinder(bore / 2, bodyH * 2).tag("bore");
const grub = cylinder(grubDia / 2, flangeDia)
  .rotate("y", 90)
  .at(0, 0, (bodyH - flangeT) / 2)
  .tag("grub");

const machined = blank.cut(grooves, shaftBore, grub).tag("machined");

// One lead-in, on the bottom of the bore. Position, not face normal: the grub
// screw hole is drilled across the part, and a rim reports its own cylindrical
// wall as adjacent, so a face-normal query would catch it too.
return machined
  .edges({ generatedBy: "machined", curve: "circle", role: "hole", at: { z: "min" } })
  .expect({ count: 1 })
  .chamfer(0.4)
  .tag("bore_lead_in");
