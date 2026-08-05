// A rolling ball of radius 5 does not fit on the top edges of a 10 mm cube.
// OCCT answers this by segfaulting, which is precisely why the kernel runs in a
// child process. What this case asserts is not that OCCT survives — it will not
// — but that the outcome reaches the caller as a typed error naming the last
// operation, instead of taking the session down with it.
return box(10, 10, 10).edges(">Z").fillet(5);
