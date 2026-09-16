// A round pipe along a spline that loops back across its own first stretch.
// The spline bends no tighter than the pipe, so the graph's curvature rule
// passes, and the swept tube used to build, watertight and "valid", with the
// crossing counted twice in its volume. The refusal names the two places on
// the path that meet and where the surface crosses itself.
return pipe({ spline: [[0, 0, 0], [16, 0, 0], [18, 10, 0], [8, 10, 0], [8, -6, 0]] }, 2);
