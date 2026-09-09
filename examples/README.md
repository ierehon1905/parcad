# Examples

Twenty-two parts, each a `.js` script that returns a shape. On first run each is
seeded into parcad's project folder (`~/Documents/parcad`, or
`PARCAD_PROJECTS_DIR`) as a `<name>.parcad` project folder, where it becomes an
ordinary project the user can edit, rename, move into a folder or throw away —
this directory is the seed, not the live copy, so editing a part in the app does
not change it here, and a part the user moves or deletes is not put back.
Most of them are measured in `eval/cases/`, so an example that stops building
fails a case rather than surprising someone.

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

## Parts

| file | part | why it is here |
|---|---|---|
| `cast-foot.js` | cast machine foot, M10 down / M8 up | draft on every wall, and hole sizes read from the fastener table rather than written out |
| `clevis.js` | threaded rod-end clevis, 10 mm pin | one fork arm authored and the other mirrored; spanner flats as a hexagon across the flats |
| `cover-plate.js` | bolted cover, turned spigot | revolved geometry: a tapered spigot and four countersunk screws, both cones |
| `diamond-v19.js` | round brilliant cut, 57 facets | a Fusion 360 recreation, promoted from `fusion360/`: a convex solid as the intersection of its facet half-spaces, agreeing with the export to every published digit |
| `display-bezel.js` | instrument fascia, seated display module | the only part here that recesses *into* a face rather than cutting through one: a milled seat, and the entry-side overlength that stops it becoming a sealed void |
| `extrusion-2020.js` | 20x20 T-slot extrusion, 200 mm | four-fold symmetry by rotating the *cutter*; corner fillets among 37 candidate edges |
| `flange.js` | ASME B16.5 class 150 NPS 2 slip-on flange | a bolt circle, and one cut whose provenance reaches five rims |
| `heat-sink.js` | 60x60 extruded fin sink | one fin shape placed nine times — one graph node, nine placements |
| `hydraulic-line.js` | bent 12 mm hydraulic line | a routed tube: straight runs and real bend radii, bored along the same route, with a round-bottomed O-ring groove on the inlet boss |
| `hex-standoff.js` | M3 hex standoff, 5.5 AF | a prism from three intersecting slabs, which is exact |
| `knurled-knob.js` | 30 mm knurled knob, D-bore | a D-bore by intersection; a dish cut with a large sphere; 24 flutes |
| `manifold-block.js` | hydraulic manifold, cross-drilled | galleries that have to actually intersect; position-based rim selection |
| `motor-mount.js` | NEMA 17 mount with gussets | a standard interface, and gussets that must be unioned unblended |
| `pillow-block.js` | 20 mm bore pillow block | a bore-carrying boss on a base; slots whose ends are arcs, not holes |
| `pipe-tee.js` | socket-weld tee for 1" pipe | a saddle intersection curve, blended, inside and out |
| `plate-stand.js` | vertical dinner-plate stand, 8 plates | a redesign of a printed part whose pegs broke: eighteen cone bosses blended into a base in one union, buried deeper than the blend because that is the ceiling on the radius |
| `shaft-coupler.js` | 8-to-10 mm rigid coupler | two blind bores meeting at a web; radial grub screws breaking into them |
| `timing-pulley.js` | 20-tooth GT2 pulley | **approximate** — and says so; see docs/DSL_GAPS.md §5 |
| `v-block.js` | 50 mm toolroom V-block | a 90° vee cut by a rotated cube, so the angle cannot drift |

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

## What is not here, and why

`fusion360/` holds recreation targets rather than finished parts: real Fusion 360
documents, exported and measured, with their volume, bounding box and face types
recorded in each header. One is a faithful recreation; the rest throw naming the
op they are blocked on. Seeding keeps the folder, so they arrive in the project
list under `fusion360` and a blocked target shows its reason when opened. A target
that becomes faithful gets promoted up here and earns a case in `eval/cases/`.
See `fusion360/README.md`.

No example has a thread or a gear — not because they were skipped, but because
the graph cannot produce those shapes. Countersinks and tapers *were* on that
list until `revolve` landed; `cover-plate.js` is what came of it, and
`hydraulic-line.js`'s groove came off the torus the same way. That groove is
round-bottomed, which is the boundary of the same argument: a catalogue O-ring
gland is rectangular and wider than the cord, so the part carries the section
the kernel can cut exactly and says so rather than passing it off. `docs/DSL_GAPS.md` §0 lists what is missing
and what each absence costs; §1 onward covers what the language *can* do but
makes harder than it should be.
