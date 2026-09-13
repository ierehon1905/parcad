// The wrap idiom: grow a filleted body by the clearance and cut it out.
//
// The thick-solid offset of a filleted body came back inside out — right
// size, right shape, faces pointing in — and every boolean then read it as
// all of space minus the part, so this cut removed everything and reported
// "no faces". The pocket: a 50 x 30 x 20 box with its four upright edges
// rounded to 5, grown by 1 to 52 x 32 x 22 with 6 mm corners and every other
// edge rounded to 1, sunk 11 mm into the top of a 100 x 60 x 30 block.
//   plan area  52*32 - (4-pi)*36        = 1633.10
//   pocket     1633.10 * 11            = 17964.1
//   floor rim  (2*(52+32) - 48 + 12pi) * (1 - pi/4) * 1  ~ 33.8 less
//   result     180000 - 17964.1 + 33.8 ~ 162069.7 mm^3
return box(100, 60, 30).at(0, 0, 15).cut(
  box(50, 30, 20).edges("|Z").expect({ count: 4 }).fillet(5).offset(1).at(0, 0, 30),
);
