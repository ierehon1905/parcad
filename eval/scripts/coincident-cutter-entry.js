// The safe row of the coincident-face table in docs/GOTCHAS.md: the same
// 30 x 20 x 4 pocket as refuse-sealed-void, with the cutter's outer face
// exactly on the face it enters. Exact coincidence is correct geometry — the
// same 11 faces and 45600 mm3 as standing the cutter proud — and it is the
// boundary the sealed-void refusal must never cross: only the strict gap in
// between seals. This case is the tripwire for that refusal ever growing a
// tolerance.
const plate = box(60, 40, 20);
return plate.cut(box(30, 20, 4).at(0, 0, 8));
