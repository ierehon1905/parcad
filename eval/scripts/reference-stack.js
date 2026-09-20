// A reference body: the coins a holder is checked against, which are not the
// holder. The stack is built, drawn in blue and measured — its own entry in
// named_bodies, and `clear` of the tray by 0.5 mm in between_bodies — but it
// is in no file and no whole-part number: `bodies` is 1, the volume the
// tray's 60 x 40 x 3 = 7200 mm³ alone, the size 60 x 40 x 3, and the check
// against it passes. Appendix C §2 of docs/COIN_HOLDER_REVIEW.md.
const tray = box(60, 40, 3).at(0, 0, 1.5).tag("tray");
const stack = cylinder(12, 20).at(0, 0, 13.5).tag("stack").reference();
return {
  tray,
  stack,
  checks: [{ clear: ["tray", "stack"], atLeast: 0.2, why: "coins must lift off the tray" }],
};
