// A 45° rotation has an exact answer — (20+10)/√2 = 21.213 mm across — so the
// gap between the two backends here is purely dual contouring, with no kernel
// disagreement mixed in.
return box(20, 10, 6).rotate("z", 45);
