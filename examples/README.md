# Examples

Twenty-eight parts, each a `.js` script that returns a shape — or, for four
of them, two named shapes. Most are measured in
`eval/cases/`, so an example that stops building fails a case rather than
surprising someone.

This directory is the seed, not the live copy. On first run each part is copied
into `~/Library/Application Support/parcad` (or `PARCAD_PROJECTS_DIR`) as a `<name>.parcad`
folder and becomes an ordinary project. Editing a part in the app does not
change it here, and a part the user moves or deletes is not put back.

```bash
bun tools/run.ts examples/flange.js > /tmp/flange.json   # DSL -> intent graph
./target/release/parcad /tmp/flange.json --brep          # exact, with measurements
cargo run -p parcad-eval -- --case flange                # against recorded values
```

## Learning order

| file | what it teaches |
|---|---|
| `bracket.js` | the whole language: blended union, a hole pattern, three kinds of selector |
| `enclosure.js` | `shell()` and `offset()`, and opening a face by cutting one away |
| `edge-fillets.js` | directional edge selectors on their own |
| `lidded-box.js` | a part in two bodies: `return { base, lid }`, measured per body and between them |

## Parts

| file | part | why it is here |
|---|---|---|
| `cast-foot.js` | cast machine foot, M10 down / M8 up | draft on every wall, and hole sizes read from the fastener table rather than written out |
| `clevis.js` | threaded rod-end clevis, 10 mm pin | one fork arm authored and the other mirrored; spanner flats as a hexagon across the flats |
| `cover-plate.js` | bolted cover, turned spigot | revolved geometry: a tapered spigot and four countersunk screws, both cones |
| `diamond-v19.js` | round brilliant cut, 57 facets | a Fusion 360 recreation, promoted from `fusion360/`: a convex solid as the intersection of its facet half-spaces, agreeing with the export to every published digit |
| `display-bezel.js` | instrument fascia, seated display module | the only part here that recesses *into* a face rather than cutting through one: a milled seat, and the entry-side overlength that stops it becoming a sealed void |
| `extrusion-2020.js` | 20x20 T-slot extrusion, 200 mm | four-fold symmetry by rotating the *cutter*; corner fillets among 37 candidate edges; and the diagonal webs it shipped without, five watertight bars that only the body count and the bed contact told apart from one |
| `flange.js` | ASME B16.5 class 150 NPS 2 slip-on flange | a bolt circle, and one cut whose provenance reaches five rims |
| `heat-sink.js` | 60x60 extruded fin sink | one fin shape placed nine times — one graph node, nine placements |
| `hydraulic-line.js` | bent 12 mm hydraulic line | a routed tube: straight runs and real bend radii, bored along the same route, with a catalogue O-ring gland on the inlet boss — a rectangular revolve section with rounded floor corners |
| `hex-standoff.js` | M3 hex standoff, 5.5 AF | a prism from three intersecting slabs, which is exact |
| `knurled-knob.js` | 30 mm knurled knob, D-bore | a D-bore by intersection; a dish cut with a large sphere; 24 flutes |
| `lidded-box.js` | open box and its lipped lid, printed as two parts | two bodies that stay two, never fused; the lip is drawn 0.3 mm inside the pocket and `between_bodies` measures exactly that on the built solids |
| `manifold-block.js` | hydraulic manifold, cross-drilled | galleries that have to actually intersect; position-based rim selection |
| `motor-mount.js` | NEMA 17 mount with gussets | a standard interface, and gussets that must be unioned unblended |
| `pillow-block.js` | 20 mm bore pillow block | a bore-carrying boss on a base; slots whose ends are arcs, not holes |
| `pipe-tee.js` | socket-weld tee for 1" pipe | a saddle intersection curve, blended, inside and out |
| `pleated-shade.js` | pendant lampshade, 24 twisted pleats, 1.4 mm wall | a surface, then a solid: pleated outlines generated in the script, a loft *surface* through fitted sections of them, `thicken` into a measured wall, and a flat cut at each rim for the print bed |
| `plate-stand.js` | vertical dinner-plate stand, 8 plates | a redesign of a printed part whose pegs broke: eighteen cone bosses blended into a base in one union, and one commit's worth of them placed on the wrong face and standing proud of the underside, which only the low end of the bounding box showed |
| `screw-top-jar.js` | jar and cap, M40 × 3 printed thread | a modelled thread, `threadedRod` on the neck and `threadedHole` in the cap, 0.25 mm clearance each; the cap's cutter turned into phase with the neck, and `between_bodies` reading the flank gap its closed form predicts |
| `shaft-coupler.js` | 8-to-10 mm rigid coupler | two blind bores meeting at a web; radial grub screws breaking into them |
| `spur-gears.js` | module 2 gear pair, 20 and 30 teeth, meshed | curves drawn from a formula: every flank an involute the script certifies to a few millionths of a millimetre, and `between_bodies` reading the 0.094 mm flank gap that 0.1 mm of backlash predicts |
| `timing-pulley.js` | 20-tooth GT2 pulley | **approximate** — and says so; see docs/DSL_GAPS.md §5 |
| `twisted-planter.js` | twisted star planter and drip saucer, two prints | the part ParCAD web opens first: a ruled loft twisted through nine star sections and hollowed along the same twist to a measured 2 mm wall, and a saucer of smooth bumps in rings of 1, 6, 12 and 18, each a revolved smootherstep profile that leaves the floor with no crease |
| `v-block.js` | 50 mm toolroom V-block | a 90° vee cut by a rotated cube, so the angle cannot drift |
| `wash-bottle.js` | wash bottle with a curved spout | curves in section and path: a rounded base corner, a spline shoulder and an arc bead in one revolve section, a second section for the cavity, and a spout that is `pipe({ spline })` bored along the same spline |

## Conventions every one of them follows

- **Millimetres**, Z up, primitives centred on the origin and placed with
  `.at()`.
- **Cutters cross every face they meet, at both ends.** Past the material where
  a tool exits, and proud of it where a tool enters — a recess whose cutter
  stops a few microns short of the face it enters is not a shallow recess, it is
  a sealed void under a feather edge. `display-bezel.js` shows both ends;
  docs/GOTCHAS.md, "A cut needs overlength at both ends", has the measurements.
- **Selectors carry an `.expect({ count })`.** The number is the assertion: if
  an edit changes what a selector reaches, the build fails at the selector
  rather than producing a quietly different part.
- **Comments explain why a dimension is what it is.** Where a number comes from
  a standard, the standard is named.

## `field-instrument/` — one visual manner, for comparing how models follow a style

| file | part | why it is here |
|---|---|---|
| `field-instrument/control-box.js` | moulded enclosure, 112 x 80 x 16, base plate and caps as three bodies | the style's moulded half: an 8 mm module every centre sits on, near-sharp r 0.5 plan corners, a Ø3-on-4 square grille, 16 mm pockets holding 15 mm caps, one knob |
| `field-instrument/desk-stand.js` | phone rest milled from one 120 x 85 x 12 slab | the machined half: √2 plan, plan corners at 5 % of the short side, 0.4 top break and a larger bottom chamfer, one dish as the single gesture, a cable path under the slab |
| `field-instrument/planter.js` | planter and its tray, 112 x 80 x 84 on an 8 mm module | two bodies, and the part that shows what a render cannot: a vent cutter must cross the wall it pierces (it stopped 0.25 mm short over 28 holes), a blind dimple grille, and a dial whose recess left 0.5 mm of front wall until the wall grew to 3 |

Not yet held by `eval/cases/`.

## What is not here, and why

`fusion360/` holds recreations of real Fusion 360 documents, each held to the
export's own measured volume, bounding box and face types. Seeding keeps the
folder, so they arrive in the project list under `fusion360`. Exports that do
not build yet are not examples and are not seeded; they wait in
`eval/targets/fusion360/`, and one that becomes faithful moves here and earns a
case in `eval/cases/`. See `fusion360/README.md`.

No example has a thread or a gear — not because they were skipped, but because
the graph cannot produce those shapes. Countersinks and tapers *were* on that
list until `revolve` landed; `cover-plate.js` is what came of it, and
`hydraulic-line.js`'s groove came off the torus the same way. That groove is
round-bottomed, which is the boundary of the same argument: a catalogue O-ring
gland is rectangular and wider than the cord, so the part carries the section
the kernel can cut exactly and says so rather than passing it off. `docs/DSL_GAPS.md` §0 lists what is missing
and what each absence costs; §1 onward covers what the language *can* do but
makes harder than it should be.
