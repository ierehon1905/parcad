// A disc of radius 20 revolved from a line, trimmed to what lies inside a
// cylinder of radius 10.
return surfaceRevolve([[0, 0], [20, 0]]).trim(cylinder(10, 10), { keep: "inside" });
