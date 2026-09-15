// Screw-top jar: a jar with a threaded neck and its cap, printed as two parts.
//
// The thread is modelled, not drawn as a tap drill: M40 × 3 (ISO 261 fine)
// with the ISO 68-1 basic profile, `threadedRod` on the neck and
// `threadedHole` in the cap, each given the same radial clearance so the pair
// turns freely off the printer.
//
// A thread and its mate only fit in phase. Both functions put the tooth on +X
// at z = 0 of their own frame; the neck's frame is at z = 48 and the cap's
// cutter's at z = 46, two millimetres, or 2/3 of a pitch, lower. Turning the
// cutter 360° × 2/3 = 240° backwards — the same as 120° forwards — puts it
// back in phase, and `between_bodies` reads the flank gap:
// clearance · (2/√3) / √(4/3 + (P / 2πr)²), 0.2499 mm at the cap's crest.
//
// Jar: a Ø50 × 45 body with a 2 mm wall and floor, the neck's 7.5 mm of thread
// above it, and a Ø34 bore through the neck. Cap: Ø48 × 12, the thread cut
// 10 mm up into it from its open face, 2 mm of roof above.
//
// A horizontal slice of a thread has the same area at every height, so any
// length L of one is π r1² L + 2π L / P · ∫ r w(r) dr, w the tooth's width:
// 8369.020 mm³ for the neck's 7.5 mm above the body and 11758.317 mm³ for the
// cap's 10 mm of cutter. Jar: 88357.293 + 8369.020 − 68138.003 (cavity) −
// 8625.243 (bore, 43 to 52.5) = 19963.068 mm³. Cap: 21714.688 − 11758.317 =
// 9956.371 mm³. The exact solids read 19963.054 and 9956.381.
const neck = { diameter: 40, pitch: 3 };
const play = 0.25; // clearance on each part, radially; tune it on the printer
const bodyR = 25, bodyH = 45, wall = 2, floor = 2;
const neckLength = 9, neckZ = 48; // thread from 43.5 to 52.5, 1.5 of it inside the body
const boreR = 17;
const capR = 24, capBottom = 46, capHeight = 12, capDepth = 10;

const jar = cylinder(bodyR, bodyH)
  .at(0, 0, bodyH / 2)
  .union(threadedRod(neck, neckLength, { clearance: play }).at(0, 0, neckZ))
  // The bore first, from past the neck's top down into where the cavity will
  // be, so the cavity's cut opens into it rather than sealing a void.
  .cut(cylinder(boreR, 13).at(0, 0, 48.5))
  .cut(cylinder(bodyR - wall, bodyH - floor - wall).at(0, 0, floor + (bodyH - floor - wall) / 2))
  .tag("jar");

const cap = cylinder(capR, capHeight)
  .at(0, 0, capBottom + capHeight / 2)
  .cut(
    threadedHole(neck, capDepth, { clearance: play })
      .rotate("x", 180) // enter from below, going up
      .rotate("z", 120) // back in phase with the neck; see above
      .at(0, 0, capBottom),
  )
  .tag("cap");

return { jar, cap };
