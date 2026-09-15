// A radius arc between corners 20 mm apart with a radius of 8: no circle of
// radius 8 passes through both. The refusal names the smallest that does.
return extrude([[0, 0], [20, 0], { radius: 8 }, [20, 20], [0, 20]], 2);
