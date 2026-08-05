// A deliberately wrong cardinality. `expect({ count })` must fail *before* the
// treatment changes the solid — the whole point is that a changed match count
// is loud rather than a quietly different part.
return box(20, 20, 20).edges(">Z").expect({ count: 99 }).fillet(1);
