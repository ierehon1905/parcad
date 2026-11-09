// One corner of a 20 mm cube rounded 2 mm: the reply's count is the corner,
// as .vertices(...).expect({ count }) counts it.
return box(20, 20, 20).vertices(">X and >Y and >Z").expect({ count: 1 }).fillet(2);
