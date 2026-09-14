// Lidded box: an open base and a lid with a locating lip, printed as two parts.
//
// The one thing this part shows is a script returning two bodies that stay
// two — `return { base, lid }` — so the report measures each one and says how
// they sit: the lip is drawn 0.3 mm inside the pocket all round and the lid
// stands 0.5 mm above the rim, so the closest the two come is 0.3 mm, and
// `between_bodies` reports exactly that on the built solids. Nothing here is
// fused, mated or constrained: the lid sits where this script placed it.
//
// Base: 60 x 40 x 20 with a 2 mm wall and floor, so 48000 - 56·36·18 =
// 11712 mm³. Lid: a 60 x 40 x 3 plate plus a 55.4 x 35.4 x 4 lip hanging
// under it, 7200 + 7844.64 = 15044.64 mm³.
const L = 60, W = 40, H = 20;
const wall = 2, floor = 2;
const lidT = 3, lipH = 4;
const fit = 0.3;      // lip to pocket wall, each side
const standoff = 0.5; // lid underside to the rim

const base = box(L, W, H)
  .at(0, 0, H / 2)
  // The pocket cutter runs 2 mm past the top face it leaves through.
  .cut(box(L - 2 * wall, W - 2 * wall, H).at(0, 0, floor + H / 2))
  .tag("base");

const plate = box(L, W, lidT).at(0, 0, H + standoff + lidT / 2).tag("plate");
const lip = box(L - 2 * wall - 2 * fit, W - 2 * wall - 2 * fit, lipH)
  .at(0, 0, H + standoff - lipH / 2)
  .tag("lip");
const lid = plate.union(lip).tag("lid");

return { base, lid };
