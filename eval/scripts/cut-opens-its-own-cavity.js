// A multi-tool cut is one cut. The cavity is listed first and stops 3 mm
// inside the top and bottom, so on its own it would be a sealed void; the
// bore through the top cap opens it. Judged tool by tool, this refused while
// cut(bore, cavity) built — the language depended on the order of the tools.
// Closed form: 40·40·30 − 30·30·24 − π·4²·3 = 26249.204 mm³.
const outside = box(40, 40, 30);
const cavity = box(30, 30, 24);
const bore = cylinder(4, 6).at(0, 0, 13);
return outside.cut(cavity, bore);
