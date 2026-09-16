// A tray with a 1.63 mm floor and walls under 4 mm thick, its four bottom
// edges chamfered by 5.41 mm: the chamfer cuts clean through the wall into the
// pocket. OpenCASCADE does not open the pocket; it returns a closed solid whose
// chamfer faces run through the pocket's walls and floor, which its validity
// check, the growth check and both mesh backstops all passed.
return box(40, 40, 20).cut(box(32.45, 34.35, 20).at(0, 0, 1.63)).edges("<Z").chamfer(5.41);
