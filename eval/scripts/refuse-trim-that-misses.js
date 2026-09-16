return surfaceExtrude([[0, 0], [40, 0]], 20).trim(box(5, 5, 5).at(100, 0, 0), { keep: "outside" });
