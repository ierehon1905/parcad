// A plywood corner and what it carries. Two 18 mm boards meet in a 45° mitre
// and a cleat is ripped at 45° along one edge: all three are cut from a sheet,
// so each 45° edge is a saw's bevel, not a feather, and no printer judges them —
// no overhang, no bed. The magnet is bought and judged by nothing; the clip is
// printed and is the one body measured as it prints: its 20 x 6 mm lip hangs
// out past its 10 mm foot, 120 mm² facing down. 200 mm deep along x.
const ply = sheet(18, { material: "birch plywood" });
const alongX = (section) => extrude(section, 200).rotate({ x: 1, y: 1, z: 1 }, 120).at(100, 0, 0);
const side = alongX([[0, 0], [18, 0], [18, 282], [0, 300]]).made(ply);
const top = alongX([[0, 300], [18, 282], [300, 282], [300, 300]]).made(ply);
const cleat = alongX([[-18, 100], [0, 100], [0, 158], [-18, 140]]).made(ply);
const clip = box(20, 10, 8).at(50, 150, 304).union(box(20, 6, 2).at(50, 158, 307));
const magnet = cylinder(5, 3).at(120, 150, 301.5).bought("10 x 3 mm neodymium magnet");
return { side, top, cleat, clip, magnet };
