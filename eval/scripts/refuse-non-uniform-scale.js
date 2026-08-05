// A non-uniform scale is not a harder uniform scale: it turns cylinders into
// elliptic cylinders and spheres into ellipsoids, which the exact backend has
// no surfaces for. It must say that, not silently scale by an average.
return box(10, 10, 10).scale(2, 3, 4);
