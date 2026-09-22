// The same body twice, drawn post down: a 20 x 20 x 2 plate on a 4 x 4 x 8
// post. `drawn` prints as drawn, +z up, so the plate's underside overhangs —
// 400 - 16 = 384 mm² — and the post's 16 mm² foot is all that meets the bed;
// a support prism under the plate to the bed, less the post, is 384 x 8 =
// 3072 mm³. `flipped` declares .printedUp("-z"): the same geometry laid the
// other way up is a plate on the bed with a post standing on it, 0 mm²
// unsupported, 400 mm² on the bed. Overhang is measured per body in its own
// orientation, never the part's.
const tee = (x) => box(20, 20, 2).at(x, 0, 9).tag("plate").union(box(4, 4, 8).at(x, 0, 4).tag("post"));
return { drawn: tee(0), flipped: tee(40).printedUp("-z") };
