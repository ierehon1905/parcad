// A rolling ball of radius 5 does not fit on the top edges of a 10 mm cube.
// This used to take the worker process down — the builder reported not-done
// and the accessor's StdFail_NotDone went uncaught — and the case pinned the
// crash supervision. The boundary now catches what OCCT raises, so what this
// case asserts is the refusal: it names the request, and it names the largest
// radius the kernel *measured* to build here (4.85 mm, verified to build as a
// fresh evaluation), instead of leaving the caller to bisect by hand.
return box(10, 10, 10).edges(">Z").fillet(5);
