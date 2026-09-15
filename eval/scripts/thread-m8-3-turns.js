// Three turns of M8 × 1.25, right- and left-handed, against the ISO 68-1
// basic profile's closed form.
//
// H = √3/2 · 1.25 = 1.082532; the core is the basic minor radius
// 4 − 5H/8 = 3.323418, and the tooth is a trapezoid 0.9375 (3P/4) wide there
// and 0.15625 (P/8) wide at r = 4, 0.676582 deep: area 0.370006 mm².
// A horizontal slice of a screw-symmetric solid has the same area at every
// height, and one pitch of it is the tooth revolved once, so over a length L
//   V = π r1² L + 2π L / P · ∫ r w(r) dr,   ∫ r w(r) dr = 1.325052 mm³
// which at L = 3.75 is 155.098715 mm³ — for either hand, a mirror image.
const right = threadedRod("M8", 3 * 1.25).tag("right");
const left = threadedRod("M8", 3 * 1.25, { hand: "left" }).at(20, 0, 0).tag("left");
return { right, left };
