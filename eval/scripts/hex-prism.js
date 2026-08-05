// A hexagon prism, which is the one extruded outline with a volume anybody can
// check by hand: a regular hexagon 20 mm across the flats has area
// 2*sqrt(3)*10^2 = 346.410 mm2, so at 10 mm thick it is 3464.102 mm3.
//
// That is the point of this case, as it was for `cone.js`. The two backends
// build an extrusion by unrelated means — OCCT sweeps a face along a vector,
// the implicit backend maxes six half-planes against a slab — so a closed form
// both must agree with is worth more than a number either one produced.
//
// It also pins `ngon`'s across-the-flats convention: read as across the corners
// this solid would be 2598.076 mm3, and both backends would agree on it.
return ngon(6, 20, 10, { across: "flats" });
