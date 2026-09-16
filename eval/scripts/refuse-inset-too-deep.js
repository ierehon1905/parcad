// A 20 x 6 rectangle inset by 4: nothing is left, and the refusal names the
// 3 mm that is the most the outline takes.
return extrude(inset([[-10, -3], [10, -3], [10, 3], [-10, 3]], 4), 5);
