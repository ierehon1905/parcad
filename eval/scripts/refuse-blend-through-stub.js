// A boss placed on the plate's underside plane instead of its top, so it runs
// through the 6 mm plate and stands 2 mm out of the far face — the plate-stand
// mistake in docs/GOTCHAS.md. The seam is then two loops, and the fillet on
// the stub cannot reach past the stub, which is what caps the radius at under
// 2 mm. The refusal has to say that it is the stub's seam it is measuring,
// rather than offer the stub's radius as the fix for the seam that was meant.
const plate = box(60, 40, 6).at(0, 0, 3).tag("plate");
const boss = cylinder(6, 40).at(0, 0, 18).tag("boss");
return union(plate, boss, { blend: 4 }).tag("body");
