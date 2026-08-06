// A smooth loft whose fitted surface genuinely leaves its sections' bounding
// box — the case the backend's containment gate exists for. The graph tells
// the mesher and renderer that a loft stays inside its sections' extent;
// ruled walls cannot leave it, but a surface fitted through oscillating
// sections overshoots between them like any interpolant. Measured when this
// case was written: 2.82 mm proud of a 80 mm box. The honest outcomes are a
// ruled loft, an intermediate section, or this refusal — never a bound that
// quietly stopped being one.
const sq = (s) => [[-s, -s], [s, -s], [s, s], [-s, s]];
return loft(
  [
    { z: 0, outline: sq(10) },
    { z: 10, outline: sq(40) },
    { z: 20, outline: sq(10) },
    { z: 30, outline: sq(40) },
  ],
  { smooth: true },
).tag("wavy");
