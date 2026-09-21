// The two numbers a verdict leaves out: how deep an overlap goes, and how
// much of a face a touch covers.
//
// A 40 x 40 x 4 plate about the origin, its top face at z = 2. The stud is a
// 5 mm cube whose underside sits at z = 1.6, so it is sunk 0.4 mm into the
// plate: 5 x 5 x 0.4 = 10 mm³ shared, and 0.4 mm deep wherever it is
// deepest. The lid is a 10 x 10 x 3 slab resting on that same top face: the
// pair is `touching` over the whole 100 mm² of its underside, in one patch.
// Read either as a verdict alone and a 2 µm graze reads like a catch and a
// corner like a seat — docs/COIN_HOLDER_REVIEW.md §2.3.
const plate = box(40, 40, 4).tag("plate");
const stud = box(5, 5, 5).at(-12, 0, 4.1).tag("stud");
const lid = box(10, 10, 3).at(12, 0, 3.5).tag("lid");
return {
  plate,
  stud,
  lid,
  checks: [
    { interferes: ["plate", "stud"], deeperThan: 0.3, why: "the stud is pressed in, not resting on the plate" },
    { touching: ["plate", "lid"], contactAtLeast: 90, why: "the lid seats on a face, not on a corner" },
  ],
};
