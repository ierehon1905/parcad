// Desk stand: a phone rest milled from one slab, in the field-instrument
// manner — the machined-metal half of it, where `control-box.js` is the
// moulded half. The numbers are the style's, from
// docs/styles/field-instrument.md (read_docs `style-field-instrument`):
//
// - plan near √2 (120 x 85), thickness about a sixth of the short side — thicker
//   than the style likes, for 2 mm of floor over the cable;
// - plan corners rounded at about 5 % of the short side, as machined bodies
//   are; top perimeter crisp, broken by 0.4 mm only;
// - a larger chamfer on the bottom perimeter, so the slab reads as floating
//   on its feet;
// - one gesture — a shallow round dish beside the slot — and detail only where
//   something touches: the slot, the cable path, the feet.
const L = 120;
const W = 85;
const T = 14;
const corner = 4; // ≈ 0.05 x 85
const topBreak = 0.4;
const bottomChamfer = 1.2;

// Slot: 12 mm wide for a phone in a case, 9 deep, leaning back 15° from upright.
const slotW = 12;
const slotDepth = 9;
const lean = 15;
const slotY = -W / 2 + 25;

// The dish: Ø40, 2 mm deep, off to the right and behind the slot.
const dishR = 20;
const dishDepth = 2;
const dishX = 30;
const dishY = 17.5;

let stand = box(L, W, T)
  .at(0, 0, T / 2)
  .fillet(corner, "|Z", { count: 4 })
  .chamfer(topBreak, ">Z", { count: 8 })
  .chamfer(bottomChamfer, "<Z", { count: 8 })
  .tag("slab");

// The walls lean back, the floor stays flat: a leaning box, intersected with
// everything above the floor. A leaning floor would drop to its lowest along
// the slot's back edge, which is exactly where the cable channel passes under.
const floorZ = T - slotDepth;
const a = (lean * Math.PI) / 180;
const along = 15; // the leaning box's centre, this far up its own axis from the floor
const slot = box(L + 2, slotW, 40)
  .rotate("x", -lean)
  .at(0, slotY + along * Math.sin(a), floorZ + along * Math.cos(a))
  .intersect(box(L + 4, W, T).at(0, slotY, floorZ + T / 2))
  .tag("slot");

const dish = cylinder(dishR, dishDepth + 1).at(dishX, dishY, T - dishDepth + (dishDepth + 1) / 2).tag("dish");

// Cable path: down through the slot floor at the centre, then out of the back
// edge in a channel under the slab.
// 2 mm of floor stays over the channel along its whole length.
const channelDepth = 3;
const channelW = 6;
const drop = cylinder(4, floorZ + 2).at(0, slotY, floorZ / 2);
const channelLen = W / 2 - slotY + 1;
const channel = box(channelW, channelLen, channelDepth + 1).at(0, slotY + channelLen / 2, (channelDepth - 1) / 2);

// Four recesses for Ø10 rubber bumpers, 10 mm in from each edge.
const foot = cylinder(5.1, 1.6).at(0, 0, 0);

stand = stand.cut(
  slot,
  dish,
  drop.tag("cable"),
  channel.tag("cable"),
  repeat(foot, grid(2, 2, L - 20, W - 20)).tag("feet"),
);

// The leaning walls meet the top at 75° in front and 105° behind; break both so
// the front lip is not a knife edge and a phone finds the slot.
stand = stand.chamfer(1, { between: ["slot", "slab"], at: { z: "max" } }, { count: 2 });

return stand;
