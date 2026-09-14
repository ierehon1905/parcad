// Two bodies drawn through each other: the design error the pairwise report
// exists to state.
//
// A 40 x 40 x 10 plate about the origin and a Ø10 boss 20 mm tall standing
// from z = 0, so the boss's lower 5 mm lies inside the plate: they share
// π·25·5 = 392.699 mm³. Each body is intact on its own; the part-level
// `bodies` and `voids` counts are of two closed mesh shells that cross, which
// is what makes them the wrong number to read here and `between_bodies` the
// right one.
const plate = box(40, 40, 10).tag("plate");
const boss = cylinder(5, 20).at(0, 0, 10).tag("boss");
return { plate, boss };
