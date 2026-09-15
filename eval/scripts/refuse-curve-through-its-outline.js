// A spline between two corners that swings out through the outline's own
// bottom edge. Only the exact curves can say so; the kernel's BRepCheck on
// the section face does, and the refusal names what to move.
return extrude([[0, 0], [20, 0], [20, 20], { spline: [[10, -10], [5, 30]] }, [0, 20]], 2);
