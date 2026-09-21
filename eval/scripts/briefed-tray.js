// A part that carries what it is for, and misses it.
//
// The brief is the pocket the session was asked for in its first four words
// and never judged anything against: 95 × 70 × 16 mm, 12 cm³ of plastic. The
// tray is 110 × 56.9 × 10.7, which is the shape that session shipped in
// answer to "its too big" — 15 mm longer than the pocket along x, and 18.2
// cm³ against a 12 cm³ budget — this tray's well is shallower, so it measures
// 34.1, and the verdict is over on both counts either way. Every other number
// in that session's report was green.
// The envelope is judged in the best of the six axis orientations, so 56.9
// takes the 70 and 10.7 the 16, and only x is over.
brief({
  envelope: [95, 70, 16],
  budgetCm3: 12,
  holds: ["3 × 2€", "4 × 1c"],
  gesture: "one hand, thumb only",
  printer: "Bambu A1 mini",
  material: "PETG",
});
const tray = box(110, 56.9, 10.7).tag("tray");
const well = box(100, 46.9, 8).at(0, 0, 2.35).tag("well");
return tray.cut(well);
