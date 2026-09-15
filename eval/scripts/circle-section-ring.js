// A full circle section, as two arcs between two corners, revolved: a torus
// of major radius 10 and minor radius 5 drawn as a revolve rather than
// torus(). V = 2 pi^2 R r^2 = 2 pi^2 * 10 * 25 = 4934.802 mm^3.
return revolve([[15, 0], { through: [10, 5] }, [5, 0], { through: [10, -5] }]);
