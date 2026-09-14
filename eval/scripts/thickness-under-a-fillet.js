// A 30 mm square plate, 8 thick, with its four top edges rounded at r = 2.
// From the top face the plate is 8 mm thick; from the underside, beneath the
// round, it is thinner — down to 6 at the walls — and that is the wall a
// distance field measured without: it dropped the fillet and read the sharp
// corner's 8 everywhere.
return box(30, 30, 8).tag("body").edges(">Z").fillet(2);
