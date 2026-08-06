# Examples

Nineteen parts, each a `.js` script that returns a shape. On first run each is
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
| `extrusion-2020.js` | 20x20 T-slot extrusion, 200 mm | four-fold symmetry by rotating the *cutter*; corner fillets among 37 candidate edges |
| `flange.js` | ASME B16.5 class 150 NPS 2 slip-on flange | a bolt circle, and one cut whose provenance reaches five rims |
| `heat-sink.js` | 60x60 extruded fin sink | one fin shape placed nine times — one graph node, nine placements |
| `hydraulic-line.js` | bent 12 mm hydraulic line | a routed tube: straight runs and real bend radii, bored along the same route |
| `hex-standoff.js` | M3 hex standoff, 5.5 AF | a prism from three intersecting slabs, which is exact |
| `knurled-knob.js` | 30 mm knurled knob, D-bore | a D-bore by intersection; a dish cut with a large sphere; 24 flutes |
| `manifold-block.js` | hydraulic manifold, cross-drilled | galleries that have to actually intersect; position-based rim selection |
| `motor-mount.js` | NEMA 17 mount with gussets | a standard interface, and gussets that must be unioned unblended |
| `pillow-block.js` | 20 mm bore pillow block | a bore-carrying boss on a base; slots whose ends are arcs, not holes |
| `pipe-tee.js` | socket-weld tee for 1" pipe | a saddle intersection curve, blended, inside and out |
| `shaft-coupler.js` | 8-to-10 mm rigid coupler | two blind bores meeting at a web; radial grub screws breaking into them |
| `timing-pulley.js` | 20-tooth GT2 pulley | **approximate** — and says so; see docs/DSL_GAPS.md §5 |
| `v-block.js` | 50 mm toolroom V-block | a 90° vee cut by a rotated cube, so the angle cannot drift |

## Conventions every one of them follows

- **Millimetres**, Z up, primitives centred on the origin and placed with
  `.at()`.
- **Cutters run past the material.** A tool ending exactly on a face leaves a
  zero-thickness sliver, which is a boolean failure waiting to happen.
- **Selectors carry an `.expect({ count })`.** The number is the assertion: if
  an edit changes what a selector reaches, the build fails at the selector
  rather than producing a quietly different part.
- **Comments explain why a dimension is what it is.** Where a number comes from
  a standard, the standard is named.

## What is not here, and why

No example has a thread, a gear or an O-ring groove — not because they were
skipped, but because the graph cannot produce those shapes. Countersinks and
tapers *were* on that list until `revolve` landed; `cover-plate.js` is what
came of it. `docs/DSL_GAPS.md` §0 lists what is missing
and what each absence costs; §1 onward covers what the language *can* do but
makes harder than it should be.
