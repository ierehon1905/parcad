// Untitled2 v1 — recreated from the Fusion 360 export.
//
// Two turned bodies: a wavy teardrop (Body1) standing clear inside a cup
// (Body2), each a revolved spline section. Fusion built them with Sketch,
// Revolve and Fillet. Measured in Fusion (measurements.json) and on the
// export's B-rep (`parcad --probe-step`), against this script's exact solids
// (its STEP export, probed the same way):
//
//   Body1  volume  36359.856 mm^3  Fusion  36356.903 (+0.008%)   export  36420.013 (-0.165%)
//          area     7027.379 mm^2  Fusion   7025.506 (+0.027%)   export   7024.176 (+0.046%)
//          bbox 36.85 x 36.85 x 81.01 mm, z 30.12..111.13     (the same)
//          faces 1: one surface of revolution; Fusion writes the same surface
//          as a rational NURBS
//   Body2  volume 126698.727 mm^3  Fusion 126698.252 (+0.0004%)  export 126430.918 (+0.21%)
//          area    26405.298 mm^2  Fusion  26405.273 (+0.0001%)  export  26400.340 (+0.02%)
//          bbox 97.09 x 97.09 x 41.93 mm                        (the same)
//          faces 6: 2 revolved walls, 2 fillets, 2 planes; Fusion's 4 NURBS
//          are the same walls and fillets
//
// The sections are the export's own curves, not a fit. Each revolved wall in
// the STEP is a surface of revolution of a clamped *uniform* degree-5
// B-spline (knots every 1/40 on Body1, every 1/15 on Body2), so its pole row
// copies verbatim into a `{ bspline, degree: 5 }` entry between the two
// corners it joins. The export's own B-rep reads a little off Fusion's
// figures on both bodies, and this recreation agrees with Fusion's rather
// than with the export's, so that gap is in how the export's rational
// surfaces integrate, not in the curves.
//
// Body2's walls in the export run on past their fillets — the outer wall's
// last pole is the sharp rim corner at z = 41.927706 — which is how the
// construction reads: revolve the section with a sharp rim, then fillet both
// rim edges. The radius is not in the export as a number; each rim's circular
// section edge is tangent to the top plane and passes through both of its
// ends at radius 5.000 mm.

// Body1: one revolved degree-5 B-spline, its 45 poles read off the export.
const drop = revolve([
  [0, 110.956529],
  [0, 31.010477],
  {
    bspline: [[0.434686, 30.846916], [1.307967, 30.557504], [2.629619, 30.236505], [4.415271, 30.034994], [6.678844, 30.162045], [8.950334, 30.670388], [11.183364, 31.562444], [13.297781, 32.841928], [15.188718, 34.504875], [16.744759, 36.525782], [17.863131, 38.84557], [18.466293, 41.357811], [18.517669, 43.897669], [18.020622, 46.289171], [17.018561, 48.394208], [15.593711, 50.15413], [13.869357, 51.651437], [11.999475, 53.093312], [10.148375, 54.732665], [8.472936, 56.805719], [7.104508, 59.466256], [6.1289, 62.711896], [5.589295, 66.41657], [5.49236, 70.378382], [5.813035, 74.361236], [6.49969, 78.139664], [7.479006, 81.540402], [8.660774, 84.490009], [9.942852, 87.046953], [11.217719, 89.34711], [12.378732, 91.560526], [13.326613, 93.8431], [13.975601, 96.292617], [14.260524, 98.896898], [14.134341, 101.560477], [13.564632, 104.140216], [12.530568, 106.477204], [11.019736, 108.430836], [9.025166, 109.909947], [6.54393, 110.861103], [4.169344, 111.179453], [2.169654, 111.165981], [0.739405, 111.045011]],
    degree: 5,
  },
]);

const cup = revolve([
  [0, 0],
  [31.41768, 0],
  {
    bspline: [[31.660743, 0.377641], [32.155253, 1.143033], [32.922166, 2.321445], [33.995368, 3.953755], [35.420329, 6.099194], [36.921685, 8.354726], [38.48245, 10.726729], [40.069618, 13.222056], [41.638334, 15.847665], [43.138939, 18.610216], [44.52321, 21.515156], [45.750413, 24.565748], [46.795755, 27.760978], [47.648505, 31.09665], [48.306297, 34.567528], [48.67852, 37.448406], [48.872704, 39.666054], [48.964664, 41.169631]],
    degree: 5,
  },
  [49.00127, 41.927706],
  [37.975465, 41.927706],
  {
    bspline: [[37.833177, 41.03918], [37.551404, 39.292794], [37.137153, 36.765221], [36.601733, 33.580466], [35.947159, 29.91391], [35.264678, 26.563895], [34.468106, 23.5234], [33.408751, 20.768284], [31.865399, 18.251355], [29.615743, 15.921851], [26.504055, 13.742413], [22.509251, 11.705599], [17.831331, 9.853358], [12.819886, 8.26233], [7.888254, 7.025318], [4.32704, 6.392207], [2.010378, 6.150094], [0.640955, 6.096291]],
    degree: 5,
  },
  [0, 6.096291],
])
  .edges({ curve: "circle", at: { z: "max" } })
  .expect({ count: 2 })
  .fillet(5);

return { body1: drop, body2: cup };
