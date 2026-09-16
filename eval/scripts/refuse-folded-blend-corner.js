// A 0.4 mm blend where a slot's floor meets a bore that runs through the slot's
// wall. The corner patch OpenCASCADE makes there folds over near its
// degenerate corner: sampled from its own poles, its normal turns over on 224
// of 40401 points. Nothing else sees it — no two faces cross, the mesh closes,
// and the validity check passes — so this case holds the check of each new
// face against itself, which is most of what the treatment check costs.
return box(40, 30, 20).cut(
  box(3.26, 40, 11.21).at(3.82, 0, 10),
  cylinder(3.67, 30).at(1.11, -0.36, 0),
  { blend: 0.4 },
);
