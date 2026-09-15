# Style: field instrument

A way for a part to look and be made: a small precise instrument that is also
a little bit of a toy. Use it when the user asks for this style by name, or for
something "like a synth / a lab instrument / a toy that is a tool". It changes
proportions, edges and layout — never the fit, the fasteners or the checks.

The numbers below were measured off shipped hand-held instruments in this
manner, from scale drawings and renders. They are a starting point to keep, not
a range to wander in.

## The idea in five rules

1. **One simple solid.** A thin slab, a box or a cylinder. Not a sculpted shell,
   not a stack of shapes.
2. **One module, and everything on it.** Pick a pitch and put every hole, key,
   knob, pocket and edge margin on it or on half of it.
3. **Flat faces, detail only where something touches.** Large uninterrupted
   planes; features only where a finger, a screw, a cable or a foot goes.
4. **One gesture.** Exactly one control or feature is larger, rounder or odder
   than the rest: a big dial, a dish, a crank, a handle that is also the stand.
5. **The process shows.** A machined part looks machined, a moulded part looks
   moulded. Do not mix the two looks in one body.

## Proportions

| kind of part | plan ratio | thickness |
|---|---|---|
| hand-held, desk object | about √2 : 1 (1.36–1.45) | 6–10 % of the short side; more only for a stated reason |
| long control strip | about 2.75 : 1 | 5–10 mm |
| character object, speaker | 1 : 1 | as the contents need |

## Edges

| body | plan corners (vertical edges) | top perimeter | bottom perimeter |
|---|---|---|---|
| machined metal look | radius ≈ 5 % of the short side (3 mm at 62, 5 mm at 102) | crisp, or a 0.3–0.5 mm break | a larger chamfer, 1–1.5 mm, so the part reads as floating on its feet |
| moulded plastic look | 0–0.5 mm: near-sharp | crisp | crisp |

Keep every radius constant along its edge. No blends between features, no domes.

## Layout

- **Module**: often a multiple of 2.5 mm (7.5, 12.5), or 8 mm when the part
  should take LEGO-compatible pins. Keys sit on 12.5–24 mm pitches; secondary
  controls sit on twice the key pitch.
- **Margins**: one small margin, repeated — about 4.5 mm on a hand-held, or one
  module. Every row of features shares an edge line with another row.
- **Gaps between keys**: 0.3–1 mm.
- **Pockets and caps**: a cap stands 0.5 mm inside its pocket on every side
  (15 mm cap in a 16 mm pocket), its corners rounded 1.5–1.75 mm, pocket
  corners near-sharp.
- **Knobs**: a small cap in a larger recessed ring — Ø8 in Ø16, Ø6.4 caps on
  narrow strips. One knob may be the gesture.

## Holes and grilles

- **Grille**: holes on a square grid, hole diameter 0.55–0.75 of the pitch
  (Ø1.15 on 2.0, Ø1.6 on 2.2, Ø3.0 on 4.0), in one rectangular or round field on
  a flat face. One field per face.
- **For a print**, keep the web between grille holes at 1.2 mm or more (three
  lines of a 0.4 mm nozzle): Ø2.8 on a 4 mm pitch, not Ø3.0.
- **Fasteners** hidden on the top face; countersunk or recessed underneath.
  Standard metric only: `holeFor`, `clearance`, `counterbore`, `countersink`.
- **Feet**: four round recesses for bumpers, one module in from the corners and
  never over a screw.

## Colour and marks

- A neutral body and at most **one accent** on the single most important
  feature. Colour means something: red or orange is record, stop or danger.
- A part is one colour when printed, so express the accent as geometry: the
  caps as their own body (`return { body, caps }`), a ring, a deeper pocket.
- Labels, when there are any, are lowercase, small and on the module. There is
  no text operation yet; do not fake letters from boxes.

## Do not

- Round the top perimeter of a moulded body, or leave a metal body's plan
  corners sharp.
- Scatter features off the module "to balance" a layout.
- Add a second gesture, a second accent, or a decorative feature with no job.
- Copy a real product's face, name or model code. Use the style's rules, not
  anyone's product.

## Before you call it done

The style makes small features, and small features make thin walls and
collisions that every number in `evaluate_part` passes. Two checks, every time:

1. `measure_wall_thickness` with `threshold_mm` set to the process minimum
   (1.2 for a 0.4 mm nozzle). Read every `thin_spots` entry; a spot between two
   features you did not mean to touch is a defect, not a style choice.
2. For a part in several bodies, `between_bodies`: caps `touching` their pocket
   floors, nothing `interfering`.

Then tell the user the thinnest wall and where it is, in the same reply as the
part. A pocket that leaves 1 mm under it, a grille hole that nicks a screw boss
and a cable channel that leaves a 0.01 mm sliver under a slot have all shipped
from this style once, and all three were visible to check 1.

`examples/field-instrument/` has two parts built by these rules:
`control-box.js` (moulded) and `desk-stand.js` (machined).
