// A 40 mm square plate, 6 thick, with a Ø12 bore through its middle: the part
// the probe's closed forms are written on. Every number in its case is read
// off these two lines — the walls beside the bore are (40 − 12) / 2 = 14 mm.
const plate = box(40, 40, 6).tag("plate");
const bore = cylinder(6, 20).tag("bore");
return plate.cut(bore);
