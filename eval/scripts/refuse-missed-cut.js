// A cutter that misses the material entirely: a hole drawn 10 mm past the
// edge of the plate. This used to evaluate silently to the untouched plate,
// and the missing hole was found one evaluation later by counting faces.
return box(40, 40, 10).cut(cylinder(3, 20).at(30, 0, 0));
