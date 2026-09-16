return surfaceExtrude([[0, 0], [40, 0]], 20).edges({ role: "boundary", at: { z: "max" } }).patch();
