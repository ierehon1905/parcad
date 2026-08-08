// Two coaxial cylinders meeting exactly on a face — docs/DSL_GAPS.md §4's
// first row, the flange hub before its overlap workaround. There is no corner
// for a fillet to roll along, so no radius blends this seam; the kernel says
// so ("no suitable edges") and the refusal must translate that into the fix —
// overlap the members — rather than blame the radius or kill the worker. The
// probe half matters too: every smaller radius is measured to fail, so the
// message must not suggest one.
const plate = cylinder(76.2, 19.1).tag("plate");
const hub = cylinder(46, 20).at(0, 0, 19.55 + 10).tag("hub");
return union(plate, hub, { blend: 3 }).tag("body");
