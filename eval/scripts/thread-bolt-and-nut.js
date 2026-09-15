// An M8 bolt in a printed nut, each given 0.2 mm of clearance, as two bodies.
//
// Clearance c moves the whole profile radially: the bolt's every diameter
// down by 2c, the nut's cutter's up by 2c. Its volumes, from the slab closed
// form in thread-m8-3-turns with r1 = 3.323418 ∓ 0.2:
//   bolt, L = 20:           π r1² L + 2π L / P · ∫ r w dr = 738.740412 mm³
//   nut, 13 × 13 × 8 block: 1352 − the cutter's 8 mm      = 983.731029 mm³
// The flanks lean 30° from radial, so a radial shift c is c/2 across a flank:
// c/2 on the bolt and c/2 on the nut make the gap c, and 2c at crest and root.
// The helix leans the flank's normal by the lead angle, which shortens that
// gap to c · (2/√3) / √(4/3 + (P / 2πr)²): 0.19976 mm at the nut's crest,
// r = 3.5234, the narrowest place.
//
// The bolt's top is at z = 5 and the nut's entry face at z = 10: 4 and 8
// pitches from the tooth's origin at z = 0, so the two are in phase. Off by
// 0.4 of a pitch (the nut's face at z = 8) they read as interfering.
const bolt = threadedRod("M8", 20, { clearance: 0.2 }).at(0, 0, -5);
const nut = box(13, 13, 8)
  .at(0, 0, 6)
  .cut(threadedHole("M8", 8, { through: true, clearance: 0.2 }).at(0, 0, 10));
return { bolt, nut };
