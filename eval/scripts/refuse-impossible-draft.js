// A 10 mm rib cannot carry 30° of draft over 40 mm: the two walls would meet
// 20 mm up and the solid would close on itself.
//
// The refusal has to name the angle that *would* work, because that is the
// caller's next question, and it has to be measured rather than estimated —
// the graph bisects for it. Both backends refuse from the same function, so a
// draft that builds in one can never fail in the other.
return extrude(
  [
    [-5, -5],
    [5, -5],
    [5, 5],
    [-5, 5],
  ],
  40,
  { draft: 30 },
);
