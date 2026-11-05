// A plate with 26 posts stacked at one spot, each wider and taller than the last,
// every post's bottom flush with the plate's bottom, and the union tagged. The
// posts' boxes all overlap, so each is its own boolean layer, and each fuse splits
// the shared bottom face on the same domain — the shape that made a tag's lineage
// double per layer (docs/GOTCHAS.md, "A tag's face list doubled at every boolean
// layer"). The result is just the plate and the last post; the layers are the point.
const n = 26;
const plate = box(60, 60, 2).at(0, 0, 1).tag("plate");
const posts = Array.from({ length: n }, (_, i) => cylinder(4 + 0.1 * i, 3 + i).at(0, 0, (3 + i) / 2));
return plate.union(...posts).tag("stack");
