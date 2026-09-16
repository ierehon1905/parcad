// A cylinder with no ends, as a surface: a closed curve of two arcs extruded.
return surfaceExtrude([[10, 0], { through: [0, 10] }, [-10, 0], { through: [0, -10] }], 30, { closed: true }).tag("tube");
