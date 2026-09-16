// A flat sheet 40 x 20 thickened to 2 mm, centred on it.
return surfaceExtrude([[0, 0], [40, 0]], 20).thicken(2).tag("sheet");
