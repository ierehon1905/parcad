// A truncated cone, which is the one revolved shape with a volume anybody can
// check by hand: pi * h * (r1^2 + r1*r2 + r2^2) / 3 = 3267.256 mm3 for these.
//
// That is the point of this case. `revolve` was the first primitive here
// checked against a closed form rather than a recorded number: OCCT sweeps a
// face about the axis, and a number it must agree with is worth more than one
// it produced.
return cone(10, 4, 20);
