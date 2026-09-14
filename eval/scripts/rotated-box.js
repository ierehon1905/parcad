// A 45° rotation has an exact answer — (20+10)/√2 = 21.213 mm across — which
// the kernel must land on: a rotated box has only planes, so nothing about it
// is a tessellation's approximation.
return box(20, 10, 6).rotate("z", 45);
