// The control box's defect in four lines: a Ø2.8 grille hole through a 2.5 mm
// top, its axis 4 mm from that of the Ø7 screw boss standing up to the top's
// underside, and drilled 1.5 mm too deep, so its foot takes a lens out of the
// boss. The lens is where circles of radius 3.5 and 1.4, 4 apart, overlap:
// 1.515975 mm² by the two-circle formula, over 1.5 mm, so `grille` cuts
// `boss` by 2.273962 mm³, while the π · 1.4² · 2.5 = 15.394 mm³ it takes from
// the top is what it is for. A model that never runs the sweep still reads
// this line in every reply.
const top = box(30, 30, 2.5).at(0, 0, 8.75).tag("top");
const boss = cylinder(3.5, 7.5).at(0, 0, 3.75).tag("boss");
const grille = cylinder(1.4, 4.5).at(4, 0, 8.25).tag("grille");
return top.union(boss).cut(grille);
