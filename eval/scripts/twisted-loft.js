// A ruled loft between a square and the same square listed a quarter turn on.
//
// The pairing is by outline index, taken literally, so shifting the top
// outline one vertex around twists every wall: each becomes the doubly-ruled
// bilinear patch through its four corners. The section at height fraction t
// is the square mapped by (1-t)I + tR, R a quarter turn, so its area is
// a^2((1-t)^2 + t^2) and the volume integrates to (2/3) a^2 L exactly —
// 2000 mm^3 here against the prism's 3000.
//
// This case exists because OCCT's ThruSections compatibility pass used to
// re-origin the wires and silently rebuild exactly this loft as that straight
// prism — right vertex count, right height, wrong solid, no error. The volume
// is the tripwire: 3000 means the pairing was "fixed" behind the author's
// back again. See vendor/opencascade/PARCAD-CHANGES.md.

const a = 10;
const L = 30;
const sq = [
  [a / 2, a / 2],
  [-a / 2, a / 2],
  [-a / 2, -a / 2],
  [a / 2, -a / 2],
];
const quarter = ([x, y]) => [-y, x];
return loft([
  { z: -L / 2, outline: sq },
  { z: L / 2, outline: sq.map(quarter) },
]);
