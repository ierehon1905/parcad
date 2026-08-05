// Same impossible fillet, two millimetres larger. At radius 5 OCCT dies; at
// radius 8 it returns a shape, and the shape is wrong — a 10 mm cube comes back
// roughly 14.95 x 14.10 x 10.54 mm.
//
// No fillet can enlarge a solid. It removes material at a convex edge and adds
// it inside a concave one, and neither moves a bounding-box extreme outward.
// backend.rs checks exactly that bound after the builder returns, so the wrong
// shape is now a refusal naming the radius as the cause.
//
// Note what the correct answer is *not*: a 10 mm cube. A rolling ball of
// radius 8 does not fit these edges at all, so there is no fillet to compute
// and refusing is the whole of the right behaviour.
return box(10, 10, 10).edges(">Z").fillet(8);
