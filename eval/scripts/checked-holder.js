// A part that carries its own checks, one of which fails by a closed-form
// margin: docs/COIN_HOLDER_REVIEW.md, B2.
//
// A 60 x 40 x 3 plate standing on z = 0 and a Ø24 stack of coins 20 mm tall
// whose underside sits at z = 3.13, so the pair is `clear` by exactly 0.13 mm
// against a check asking for 0.2. The other four checks hold: the part is
// 60 x 40 x 23.13 inside 115 x 65 x 30, two closed bodies, watertight, and
// the plate's whole 2400 mm² underside is on the bed. evaluate_part reports
// the failure first and builds on; export_part and save_project refuse it
// unless told why it is acceptable.
const plate = box(60, 40, 3).at(0, 0, 1.5).tag("plate");
const stack = cylinder(12, 20).at(0, 0, 13.13).tag("stack");
return {
  plate,
  stack,
  checks: [
    { clear: ["plate", "stack"], atLeast: 0.2, why: "coins must not bind on the plate" },
    { size: { max: [115, 65, 30] } },
    { standsOn: { atLeast: 0.3 } },
    { bodies: 2 },
    { watertight: true },
  ],
};
