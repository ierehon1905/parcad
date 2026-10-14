// A tray with a 1.63 mm floor and walls under 4 mm thick, its four bottom
// edges chamfered by 5.41 mm, deeper than the walls. Written cut-then-chamfer,
// built chamfer-then-cut: the chamfer's reach meets the pocket, so the box is
// chamfered first and the pocket cut through it, and the pocket breaks out
// through the chamfer faces. OpenCASCADE's own chamfer on the cut tray
// returned a closed solid whose chamfer faces ran through the pocket's walls.
return box(40, 40, 20).cut(box(32.45, 34.35, 20).at(0, 0, 1.63)).edges("<Z").chamfer(5.41);
