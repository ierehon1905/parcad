// A 12 mm wide section swept along a spline that turns sharply at its middle
// point: the inside of that bend would sweep through itself, and the
// refusal names the radius and where it is.
return pipe({ spline: [[0, 0, 0], [10, 8, 0], [20, 0, 0]] }, 24);
