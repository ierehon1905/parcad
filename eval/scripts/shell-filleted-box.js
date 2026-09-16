// A shell of a filleted box. The kernel returns the inward offset of anything
// treated or combined as a bare shell rather than a solid, and subtracting a
// shell removes nothing — this used to be refused as "the operations
// cancelled all the material away". The cavity is now closed into a solid,
// checked for self-intersection, and the result held to part minus cavity.
//   part    (40*30 - (4-pi)*25) * 20 = 23570.796
//   cavity  (36*26 - (4-pi)*9)  * 16 = 14852.389   (corner radius 5 - 2 = 3)
//   shell                             =  8718.407 mm^3
return box(40, 30, 20).edges("|Z").fillet(5).shell(2);
