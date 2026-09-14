// The manifold of docs/PERCEPTION.md §3: a blind Ø10 port down from the top
// face to z = −5, and a Ø8 gallery straight through along X on the mid-plane.
// They meet, and the ray that proves it runs across the part at the gallery's
// height, where the void is bounded by the port's wall and 10 mm wide.
const block = box(60, 30, 30).tag("block");
const port = cylinder(5, 25).at(-20, 0, 7.5).tag("port");
const gallery = cylinder(4, 80).rotate("y", 90).tag("gallery");
return block.cut(port, gallery);
