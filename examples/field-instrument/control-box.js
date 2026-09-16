// Control box: a moulded enclosure for a small board, in the field-instrument
// manner — one slab, one module, one hero control, detail only where a finger
// or a screw goes. The numbers are the style's, from
// docs/styles/field-instrument.md (read_docs `style-field-instrument`):
//
// - an 8 mm module: the plan is 14 x 10 cells and every centre below is on it
//   or on half of it;
// - a moulded body keeps its plan corners near-sharp, r 0.5, and its top
//   perimeter crisp — rounding is for machined metal;
// - a square grille on a 4 mm pitch, hole 0.55–0.75 of the pitch: Ø2.8 here,
//   which leaves 1.2 mm webs a 0.4 mm nozzle lays as three lines;
// - pad pockets 16 mm on a 24 mm pitch (three cells), their caps 15 mm, so a
//   cap stands 0.5 mm inside its pocket all round;
// - one knob: an Ø8 cap in an Ø16.2 recessed ring.
//
// Three bodies: the shell, its base plate, and the caps with the knob, which
// is where a second colour would go. Printed separately; the caps are drawn
// seated in their pockets, 2 mm proud, so `caps` is seven pieces on purpose.
const M = 8; // module
const L = 14 * M; // 112
const W = 10 * M; // 80
const H = 2 * M; // 16
const wall = 2;
const top = 2.5; // 1.5 mm left under a 1 mm pocket
const pocketDepth = 1;
const proud = 2;
const plateT = 2;
const plateFit = 0.2;

// --- shell ---------------------------------------------------------------
let shell = box(L, W, H)
  .at(0, 0, H / 2)
  .fillet(0.5, "|Z", { count: 4 })
  // Open underneath: the cutter leaves through the bottom face by 0.5 mm.
  .cut(box(L - 2 * wall, W - 2 * wall, H - top + 0.5).at(0, 0, (H - top - 0.5) / 2))
  .tag("shell");

// Screw bosses from the base plate up to the underside of the top, in the
// corners and clear of every hole through the top: the nearest grille hole
// wall is 1.3 mm from the top-left boss.
const bossAt = grid(2, 2, 12 * M, 8 * M); // (±48, ±32)
const bossH = H - top - plateT;
const boss = cylinder(3.5, bossH + 0.5).at(0, 0, plateT + bossH / 2 + 0.25);
shell = shell.union(repeat(boss, bossAt));
// Tapped from below: the cutter enters at the plate's top face and goes up.
shell = shell.cut(repeat(holeFor("M2", 8, { tapped: true }).rotate("x", 180).at(0, 0, plateT), bossAt));

// Grille: 8 x 8 holes, left half; its top row meets the function keys' top edge.
const grilleX = -4 * M;
const grilleY = 1.5 * M;
const grilleHole = cylinder(1.4, top + 2).at(0, 0, H - top / 2);
shell = shell.cut(repeat(grilleHole, grid(8, 8, M / 2, M / 2).map(([x, y]) => [x + grilleX, y + grilleY])));

// Pads: 2 x 2 at a three-cell pitch, right half, and two function keys as a
// third row on the same pitch. The bottom pads and the knob ring share a row,
// one module in from the edge; the function keys and the grille top share one.
const padsX = 3 * M;
const padPockets = grid(2, 2, 3 * M, 3 * M).map(([x, y]) => [x + padsX, y - 1.5 * M]);
const fnPockets = grid(2, 1, 3 * M, 0).map(([x]) => [x + padsX, 3 * M]);
const pocketZ = H - pocketDepth / 2 + 0.25;
shell = shell.cut(
  repeat(box(16, 16, pocketDepth + 0.5).at(0, 0, pocketZ), padPockets),
  repeat(box(16, 8, pocketDepth + 0.5).at(0, 0, pocketZ), fnPockets),
);

// Knob ring: the one round thing on the deck, below the grille.
const knobX = grilleX;
const knobY = -3 * M;
shell = shell.cut(cylinder(16.2 / 2, pocketDepth + 0.5).at(knobX, knobY, pocketZ));

// Two holes on the short side for pegs that tile boxes together, on the module.
shell = shell.cut(
  repeat(cylinder(2.4, wall + 2).rotate("y", 90).at(-L / 2, 0, H / 2), [[0, -1.5 * M], [0, 1.5 * M]]),
);

// --- base plate ----------------------------------------------------------
const plate = box(L - 2 * wall - 2 * plateFit, W - 2 * wall - 2 * plateFit, plateT)
  .at(0, 0, plateT / 2)
  .cut(
    repeat(holeFor("M2", plateT, { through: true }).at(0, 0, plateT), bossAt),
    // Countersunk from underneath, so the screw heads sit flush with the bed face.
    repeat(countersink("M2").rotate("x", 180), bossAt),
    // Four foot recesses, a module inside the screws so a foot never covers one.
    repeat(cylinder(4, 1).at(0, 0, 0), grid(2, 2, 10 * M, 6 * M)),
  )
  .tag("plate");

// --- caps ----------------------------------------------------------------
const capZ = H - pocketDepth + (pocketDepth + proud) / 2;
const pad = box(15, 15, pocketDepth + proud).fillet(1.5, "|Z", { count: 4 }).at(0, 0, capZ);
const fn = box(15, 7, pocketDepth + proud).fillet(1.75, "|Z", { count: 4 }).at(0, 0, capZ);
const knobH = 11.8;
const knob = cylinder(4, knobH).at(knobX, knobY, H - pocketDepth + knobH / 2);
const caps = union(repeat(pad, padPockets), repeat(fn, fnPockets), knob).tag("caps");

return { shell, plate, caps };
