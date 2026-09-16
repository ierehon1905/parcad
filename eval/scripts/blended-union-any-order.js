// A blend rounds the seam of the whole union. Joined one operand at a time,
// the two bosses listed first fused into a pair that meets nowhere, the blend
// of that empty seam failed, and the part was refused — while the same union
// with the plate listed first built.
const plate = box(60, 40, 6);
const left = cylinder(5, 10).at(-15, 0, 6);
const right = cylinder(5, 10).at(15, 0, 6);
return union(left, right, plate, { blend: 2 });
