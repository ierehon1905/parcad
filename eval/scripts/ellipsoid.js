// A sphere stretched to an ellipsoid of semi-axes 20, 10 and 5 — the body and
// head of a figurine. The exact kernel used to refuse any non-uniform scale;
// BRepBuilderAPI_GTransform converts the sphere to its exact rational B-spline,
// and the backend holds the result to the determinant: 4/3·π·20·10·5 =
// 4188.790 mm³. The recorded volume is the mesh's, a tessellation under that.
return sphere(10).scale(2, 1, 0.5);
