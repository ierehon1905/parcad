// A spur gear, module 2, 20 teeth, 20° pressure angle, 10 thick: every flank
// a certified { curve } drawn from the involute of the 18.794 mm base circle.
// The section's area has a closed form, because along an involute unwound
// from angle 0 the polar area element is r²dθ/2 = rb²t²dt/2: each flank
// sweeps rb²tA³/6, and the rest is the tip and root sectors (the radial
// lines below the base circle sweep nothing).
return extrude(spurGearOutline({ module: 2, teeth: 20 }), 10).tag("gear");
