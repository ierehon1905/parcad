// The reproducer of the helical cut that "opened past two turns": a Ø1 pipe on
// a three-turn helix of radius 3, pitch 2, cut from the cylinder it wraps.
// It opened because parcad's own seam-pcurve pass damaged the cylinder's side
// faces, not in the boolean; docs/GOTCHAS.md, "A helix cut through its own
// cylinder".
//
// The tube is a disk of radius a = 0.5, perpendicular to the helix at
// (3, 0, −3), moved by screw motion through θ = 6π. The part of the tube
// inside the cylinder is exactly the sweep of the part of that disk inside
// r < 3, because screw motion keeps r. A planar region swept by a screw motion
// of angular rate 1 and rise p = 2/2π per radian encloses
//   V = θ ∫ (r·φ̂ + p·ẑ) · n dA = θ ∫ ((3 + u)·3 + p²) / √(9 + p²) du dv
// over the disk points with (3 + u)² + (v p / √(9 + p²))² < 9, which
// integrates to 20.760439 mm³. The part is π·9·10 − that = 261.982900 mm³.
return cylinder(3, 10).cut(pipe({ helix: { radius: 3, pitch: 2, turns: 3 } }, 1));
