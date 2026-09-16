// A half tube of radius 5, thickened 12 mm toward its axis.
return surfaceExtrude([[5, 0], { through: [0, 5] }, [-5, 0]], 20).thicken(12, { side: "in" });
