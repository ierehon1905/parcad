// Selected edge treatments. The queries are geometric, not `edge[7]`:
// >Z = topmost, >Y = positive-Y-most, |X = a straight edge running along X.
// If a later edit makes that query empty, the B-rep evaluator refuses instead
// of treating a different edge by accident.

const body = box(80, 60, 8).tag("body");

const rounded = body
  .edges(">Z and >Y and |X")
  .fillet(2);

// A chamfer uses the same selected-edge contract but makes a planar bevel.
return rounded
  .edges("<Z and >Y and |X")
  .expect({ count: 1 })
  .chamfer(1)
  .tag("edge_treatments");
