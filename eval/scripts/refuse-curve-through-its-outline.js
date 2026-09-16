// A spline between two corners that swings out through the outline's own
// bottom edge. Only the exact curves can say so; the core subdivides them and
// the refusal names the two pieces, where they meet, and what to move.
return extrude([[0, 0], [20, 0], [20, 20], { spline: [[10, -10], [5, 30]] }, [0, 20]], 2);
