// A tapered pipe goes through the kernel's shell sweep rather than the
// union of straight tubes a plain pipe is, so it can cross itself. This one's
// last leg runs back through its first; it used to build.
return pipe([[0, 0, 0], [40, 0, 0], [40, 30, 0], [20, 30, 0], [20, -20, 0]], 6, { bend: 5, taper: 0.9 });
