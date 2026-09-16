// Field planter: a wide pot standing on a vented plinth, and a tray it sits in.
//
// Every number is the style's (docs/styles/field-instrument.md, read_docs
// `style-field-instrument`): an 8 mm module, a √2 plan, near-sharp moulded
// corners, a crisp top rim, a grille whose holes are 0.7 of the pitch, and one
// gesture — the dial. The plinth is a module narrower than the body all round
// and stands proud of the tray, so the body reads as floating over a shadow
// gap; its vents are the only holes that go through a wall.
const M = 8;

const L = 14 * M; // 112 along the front
const W = 10 * M; // 80 deep — 1.4, the style's plan ratio
const H = 9 * M; // 72 of body above the plinth
const wall = 3; // 1.8 left behind the deepest thing cut into it, the dial
const floor = 3;

const PLINTH_L = L - 2 * M;
const PLINTH_W = W - 2 * M;
const plinthH = 1.5 * M;

// z = 0 is the tray's pool floor; the pot stands on its pads.
const stand = 2;
const plinthTop = stand + plinthH;
const floorTop = plinthTop + floor;
const deck = plinthTop + H / 2; // the front face's own middle, where the graphics sit

// Ventilation through the plinth walls, a module of plain material at each end.
const ventD = 4;

// Drainage through the floor: 0.63 of the pitch, like the vents.
const drainD = 5;

// The grille: blind dimples, because a hole here would leak soil.
const grillD = 2.8;
const grillPitch = 4;
const grillDepth = 1;
const grillCols = 9;
const grillRows = 5;
const grillX = -3.5 * M;

// The gesture: a Ø32 dial sunk 2, its boss standing half a millimetre proud.
const dialD = 32;
const dialDepth = 1.2;
const bossD = 10;
const bossProud = 0.5;
const dialX = 3.5 * M;

const plinth = box(PLINTH_L, PLINTH_W, plinthH)
  .at(0, 0, stand + plinthH / 2)
  .fillet(0.5, "|Z", { count: 4 })
  .tag("plinth");

const body = box(L, W, H)
  .at(0, 0, plinthTop + H / 2)
  .fillet(0.5, "|Z", { count: 4 })
  .tag("body");

const cavity = box(L - 2 * wall, W - 2 * wall, H).at(0, 0, floorTop + H / 2).tag("cavity");
const hollow = box(PLINTH_L - 2 * wall, PLINTH_W - 2 * wall, plinthH + 1).at(0, 0, stand + (plinthH - 1) / 2);

// Each vent is longer than the wall it crosses and centred on it, so it leaves
// through both faces: a cutter that stops short leaves a membrane, and this one
// did — 0.25 mm over every hole, which measure_wall_thickness named.
const ventZ = stand + plinthH / 2;
const vent = cylinder(ventD / 2, wall + 4).rotate("x", 90).tag("vent");
const along = vent.at(0, -PLINTH_W / 2 + wall / 2, ventZ);
const across = vent.rotate("z", 90).at(-PLINTH_L / 2 + wall / 2, 0, ventZ);
const vents = union(
  repeat(along, grid(9, 1, M, 0)),
  repeat(along.mirror("y"), grid(9, 1, M, 0)),
  repeat(across, grid(1, 5, 0, M)),
  repeat(across.mirror("x"), grid(1, 5, 0, M)),
);

const drain = cylinder(drainD / 2, floor + 2).at(0, 0, floorTop - floor / 2).tag("drain");
const drains = repeat(drain, grid(5, 3, M, M));

const dimple = cylinder(grillD / 2, grillDepth * 2)
  .rotate("x", 90)
  .at(grillX, -W / 2 + grillDepth - grillDepth, deck)
  .tag("grille");
// On a vertical face the field runs in x and z, so the grid's second axis is
// lifted into z rather than left in the plan.
const grille = repeat(
  dimple,
  grid(grillCols, grillRows, grillPitch, grillPitch).map(([x, y]) => [x, 0, y]),
);

// The cutter stands 1 mm proud of the wall and ends `dialDepth` inside it; the
// boss then runs from that floor back out to `bossProud` beyond the wall.
const dial = cylinder(dialD / 2, dialDepth + 1)
  .rotate("x", 90)
  .at(dialX, -W / 2 + dialDepth - (dialDepth + 1) / 2, deck)
  .tag("dial");
const boss = cylinder(bossD / 2, dialDepth + bossProud)
  .rotate("x", 90)
  .at(dialX, -W / 2 + dialDepth - (dialDepth + bossProud) / 2, deck)
  .tag("boss");

// One accent, on one body: the tray is RAL 2004, the orange this style uses
// where something matters; the pot is the neutral moulded grey around it.
const moulded = { color: "#d6d8d2", roughness: 0.75 };
const accent = { color: "#e25303", roughness: 0.45, clearcoat: 0.3 };

const pot = body
  .union(plinth)
  .cut(cavity, hollow, vents, drains, grille, dial)
  .union(boss)
  .chamfer(1, { on: "cavity", at: { z: "max" } }, { count: 4 })
  .material(moulded)
  .tag("pot");

// Tray: a plate with a pool the plinth stands clear of, so the pot never sits
// in what it drains. Two modules wider than the body, on the same module.
const TRAY_L = L + 2 * M;
const TRAY_W = W + 2 * M;
const trayFloor = 2.5;
const pool = 6;
const padH = stand;

const pad = box(M, M, padH + 1).at(0, 0, (padH - 1) / 2).tag("pads");

const tray = box(TRAY_L, TRAY_W, trayFloor + pool)
  .at(0, 0, (trayFloor + pool) / 2 - trayFloor)
  .fillet(0.5, "|Z", { count: 4 })
  .cut(box(TRAY_L - 2 * wall, TRAY_W - 2 * wall, pool + 1).at(0, 0, (pool + 1) / 2 - 0.5))
  .chamfer(0.4, "<Z", { count: 8 })
  .union(repeat(pad, grid(2, 2, PLINTH_L - 2 * M, PLINTH_W - 2 * M)))
  .cut(repeat(cylinder(5, 1.6).at(0, 0, -trayFloor), grid(2, 2, TRAY_L - 4 * M, TRAY_W - 4 * M)))
  .material(accent)
  .tag("tray");

return { pot, tray };
