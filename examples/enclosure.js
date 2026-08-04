// A printable enclosure — shows shell() and offset().

const w = 70, d = 45, h = 28;
const wall = 2.0;

// offset() grows the shape and rounds every convex edge as it goes,
// which is the cheapest way to break sharp corners for printing.
const outer = box(w - 6, d - 6, h - 6).offset(3).tag("outer");

const body = outer.shell(wall).tag("walls");

// Open the top by cutting away everything above the rim.
const lid = box(w, d, h).at(0, 0, h).tag("open_top");

const port = cylinder(5, 40).rotate("x", 90).at(0, -d / 2, 0).tag("port");

return body.cut(lid).cut(port);
