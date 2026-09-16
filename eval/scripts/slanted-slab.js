// A slab 2 mm thick between its faces, turned 30° about Y. A line straight
// down through it runs 2 / cos 30° = 2.309 mm of material; the wall is the
// ball that fits between the faces, 2.000.
return box(40, 30, 2).rotate("y", 30).tag("slab");
