// A 60 x 40 x 10 plate with a 20 mm square hole through its centre and a
// 10 mm bore at x = 20. fit-in-the-holes.json lays a block and two pins in
// them with check_fit; each reference is its own fit-in-the-holes-*.js.
return box(60, 40, 10).cut(box(20, 20, 20), cylinder(5, 20).at(20, 0, 0));
