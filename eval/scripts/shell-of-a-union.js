// A shell of a plate with a fin on it, refused before for the same bare-shell
// reason as shell-filleted-box. Shrinking a solid rounds its concave edges, so
// the cavity has a 1 mm round inside each of the two fin roots:
//   part    40*30*10 + 4*30*15                        = 13800
//   cavity  38*28*8 + 2*28*15 + 2*(1 - pi/4)*28       =  9364.018
//   shell                                              =  4435.982 mm^3
return union(box(40, 30, 10), box(4, 30, 20).at(0, 0, 10)).shell(1);
