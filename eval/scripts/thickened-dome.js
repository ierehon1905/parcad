// A hemisphere of radius 30 as a surface of revolution, thickened 2 mm inward.
return surfaceRevolve([[30, 0], { through: [21.2132034356, 21.2132034356] }, [0, 30]]).thicken(2, { side: "in" });
