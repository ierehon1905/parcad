// A 60 x 40 x 3 plate on two standoffs imported from one saved template: one
// at its default 10 mm, one set to 6. Each is a tube of 4 mm outer and 1.7 mm
// bore radius, so the part is 7200 + pi * (16 - 1.7^2) * (10 + 6) mm³, 7858.980
// by that closed form, 13 mm tall where the default one stands.
const plate = box(60, 40, 3).at(0, 0, 1.5);
const low = importPart("templates/standoff", { height: 6 });
const high = importPart("templates/standoff");
return union(plate, low.at(-20, 0, 3 + 6 / 2), high.at(20, 0, 3 + 10 / 2));
