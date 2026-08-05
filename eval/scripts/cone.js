// A truncated cone, which is the one revolved shape with a volume anybody can
// check by hand: pi * h * (r1^2 + r1*r2 + r2^2) / 3 = 3267.256 mm3 for these.
//
// That is the point of this case. `revolve` is the first primitive here whose
// geometry the two backends build by completely different means — OCCT sweeps a
// face, the implicit backend maxes a set of half-planes — so a closed form both
// must agree with is worth more than a recorded number either one produced.
return cone(10, 4, 20);
