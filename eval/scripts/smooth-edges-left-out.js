// Two of a box's four upright corners rounded, then "every upright edge"
// rounded again. The second selector also matches the four tangent lines
// the first fillets left behind, which no radius can build on; a treatment
// leaves those out, so the count is the two corners still sharp.
return box(40, 30, 20)
  .edges(">X and >Y and |Z")
  .expect({ count: 1 })
  .fillet(5)
  .edges("<X and <Y and |Z")
  .expect({ count: 1 })
  .fillet(5)
  .edges("|Z")
  .expect({ count: 2 })
  .fillet(2);
