# Sheet goods: a part says how each body is made

Design, 2026-11-11. Not implemented. The picture is
[images/sheet-goods-shelves.png](images/sheet-goods-shelves.png).

This note builds on `docs/notes/process-architecture.md`, which is untracked
and lives in the main checkout. That note designs a declared process for every
way of making a body. This one is the sheet-goods part of it, which the plant
shelves needed, and it changes one of that note's decisions: a body is one
piece of wood, not a fusion of boards.

## What went wrong

The plant shelves (2026-11-08..11) are three trays of 18 mm poplar plywood,
cut on a panel saw. ParCAD did not hold the truth about them:

- **The work happened outside ParCAD.** There were about 70 ParCAD calls
  against about 1,360 Python and Blender runs. Those runs produced:
  - the cut list;
  - the sheet layout;
  - four workshop sheets;
  - the renders;
  - the load numbers.
- **The model and the paper drifted apart.**
  - In `plant-shelves-v9.js` each shelf is one fused body, and its facing
    is a single `ring()` solid.
  - On paper the facing is four boards, mitred at 40°, 26.4° and 23.7°.
  - Those angles exist only in `cut_v9.py` and `despiece_v9.py`.
  - At one point the model had a separate rim and apron where the paper had
    one 90 mm board.
- **FDM checks ran on plywood, and the geometry was bent to quiet them.**
  - The report judged the boards against a 256 mm bed and for overhangs.
  - Any edge enclosing less than 60° of material is a feather
    (`EDGE_MIN_WEDGE_DEG` in `perceive.rs`). That covers a 45° rip and a
    40° mitre (a 50° wedge).
  - So the cleats got a 2 mm flat, and the LED strip ends were shortened.
- **The noise is still there after splitting the model by piece.** Split
  into one body per piece, the shelves build today with these lines in the
  CLI report:
  - "prints flat on no bed here … split it";
  - "NOT watertight … 14 SEPARATE BODIES" (touching bodies merged into one
    mesh);
  - 666 `between` pairs, of which 64 say anything.

## The declaration

A body is one of four things. The process is data, set in place like
`.reference()`, so a bare `led.bought(...)` statement works. The paper test in
process-architecture.md found `const lid = lid.made(...)` written as a
redeclaration, which is why these set in place.

```js
const PLY18 = sheet(18, { stock: "poplar plywood B/BB", size: [2500, 1220], kerf: 4 });
brief({ process: PLY18 });                      // every made body is cut from PLY18 unless told otherwise

const back = box(945, 18, 158).at(487.5, -27, 1231);                        // made: PLY18, from the brief
const led = box(754, 12, 7).at(440, -250, 1228.5).bought("LED profile 12×7, 24 V strip");
const wall = box(1300, 100, 2700).at(650, 50, 1350).reference();
const clip = box(20, 10, 8).at(900, -60, 1240).made(fdm());                  // a printed part in the same assembly
```

| kind | declared by | manufacturing checks | in fit and interference checks | emits |
|---|---|---|---|---|
| made | `.made(process)`, or the brief's `process` | the process's own | yes | the process's outputs |
| bought | `.bought(name)` | none | yes | a line in the bill |
| reference | `.reference()` | none | yes, against made and bought bodies | nothing |
| undeclared | nothing | integrity only, plus one suggestion line | yes | nothing |

- **Printing stays exactly as it is for a part that says it prints.**
  `brief({ printer })` and `.printedUp()` declare `fdm()`. The undeclared row,
  integrity and the suggestion line are process-architecture.md's; this note
  does not change them.
- **The suggestion is measured.** It counts the bodies that measure as flat
  panels of a common sheet thickness and names the declaration. For example:
  "27 bodies are flat panels of 18.0 mm — `.made(sheet(18))` gives a cut
  list".
- **A sheet has per-body options**, used only where a piece needs them:
  - `grain`: an axis or a direction. The default is along the piece's long
    side, and a piece with declared grain is not turned when the sheets are
    estimated.
  - `band`: the directions of edges that get edge banding. Each one selects
    the side faces whose outward normal is nearest to it.
  - Example: `.made(PLY18, { grain: "z", band: ["-y"] })`.
  - The shelves need neither.
- **`cut` is `"saw"` (the default) or `"router"`, with `tool` for the
  cutter's diameter.** The checks depend on it; see the next table.
- **A bought name is free text for now.** A catalogue in the manner of
  `METRIC_FASTENERS` comes later, once there are parts that need one.

### One body per piece

process-architecture.md planned `board()` nodes inside a fused body, with the
cut list read from the graph and each board sized by intersecting it with the
finished body. This design makes every piece of wood its own body instead,
for these reasons:

- **One thing has one name.** The body, its cut-list row, the number on its
  drawing and the pencil mark on the board are the same object. No board is
  hidden inside a union.
- **It is measured, not inferred.** The check reads the exact solid, however
  it was built: a box, an extrude, a cut, an imported part. There is no graph
  to search and no boolean per board.
- **Joints show up for free.** `between_bodies` already reports each joint as
  touching, with its contact area: a mitre over about 1,800–2,100 mm², the
  base on the back board over 16,362 mm². Two pieces that interfere are wood
  counted twice, and that is a failure.
- **The cost is many bodies, and today's DSL already covers it.** A function
  returns a level's pieces, and the part spreads them into its return:
  `return { ...level(1, 460), ...level(2, 1250), ...level(3, 1850), wall }`.
  The example script builds exactly this way.
- **It needs process-architecture's "relations: problems only" first.** Of
  666 pairs, 602 say nothing.

## What a sheet body is checked for

Integrity still applies, with one correction. A feather that **fails**
everywhere would refuse every mitre and every French cleat. So:

- The 60° wedge rule moves into `fdm`.
- Integrity keeps a feather only below an angle no process cuts on purpose,
  such as a cut that grazed a face. That angle is set from the corpus, the way
  `INTEGRITY_FLOOR_MM` is.

Every check below runs on the exact B-rep. The strength column uses
process-architecture.md's words.

| check | strength | how |
|---|---|---|
| one flat panel of the stock's thickness | proven | Find two opposite planar faces with the largest area. Every vertex must lie between them along their normal, and their distance must equal the stock thickness within 0.05 mm. A violation names the measured thickness and the fix ("90 mm through its thinnest direction: one ring, not a panel; make each side its own body"). |
| fits the sheet | proven | The cut size must fit inside the sheet less its trim, in either orientation, or only along the grain when grain is declared. |
| straight through-cuts (`saw`) | proven | Every other face is planar and spans the whole thickness. A curved face or a face that stops part-way gets `review`: "needs `cut: "router"`". |
| an inside corner sharper than the cutter (`router`) | proven | The corner radius against `tool / 2`. |
| bevel or mitre within the saw | guideline | The tilt from square must be at most 45°, which is how far a blade tilts. The measured tilt is in the cut list. |
| two pieces overlap | proven | `between_bodies`, made against made. |
| not checked | unchecked | Always listed: load, the real sheet thickness (plywood runs 17.6–18.3 mm), grain appearance, finish, hardware. |

## What it emits

1. **`cut_list`**, one row per distinct piece:
   - **What each row holds:** the position, the bodies, the cut size
     L × W × T, the count, the cuts, and the grain and banding when they
     are declared.
   - **Cut size:** the smallest rectangle around the panel's outline with
     one side on an outline edge. That is the blank the saw cuts first, so
     a mitred or rotated piece gets its real blank. OpenCutList and Fusion's
     ExportCutlist take an axis-aligned bounding box instead, which ignores
     mitres.
   - **Cuts:** the in-plane angled cuts, each mitre with its end, and each
     bevel along an edge, all as measured tilts.
   - **Count:** pieces congruent in outline, thickness and cuts share a row.
   - **Positions:** numbered in the order the bodies are returned.
2. **`sheets`**, an estimate with both bounds:
   - The low bound is cut area grown by the kerf, divided by usable sheet
     area.
   - The high bound comes from a guillotine heuristic (first fit
     decreasing), which turns a piece only when no grain is declared.
   - The layout itself stays the shop's or an optimiser's
     (`dont-duplicate-what-slicers-do`): the v9 shop message already said
     "Раскладку можете сделать свою" (make your own layout).
   - If ParCAD ever needs a real layout, use
     [PackingSolver](https://github.com/fontanf/packingsolver) (MIT, C++).
     Its guillotine mode handles stages, kerf and trims, and OpenCutList
     uses it. Do not write a layout engine.
3. **`export_part` formats.**
   - **`cutlist`:** a CSV with the columns cut-list optimisers read:
     `Label, Length, Width, Thickness, Qty, Material, Grain, L1, L2, W1,
     W2`. CutList Optimizer matches columns by header, MaxCut and CutList
     Evo map them on import, and a panel shop feeds the same list to Cut
     Rite or Ardis. No open standard exists, so one wide file is the
     handoff.
   - **`drawings`:** A3 sheets in the ISO 128 and 129 style of the
     v9 order, as a PDF with the same DXF beside it.
     - Every distinct shaped piece gets a view.
     - Every vertex is dimensioned from the cut-size rectangle's corner.
     - A mitred or bevelled piece gets an edge view with its angles, measured
       from square (the way a mitre saw is set).
     - Plain rectangles appear only in the table.
   - The DXF outlines also serve a CNC router.
4. **`bought`**, the bill of bought parts: name, count, and the measured
   length of a profile or strip.

The assembly drawing, the joinery and the finishing notes (sheet 1 of the v9
order) stay with the author. A reply cannot know where the confirmat screws
go.

## How a model reads it

The reply lives under process-architecture's `manufacture`, one object per
process. It puts status first, then `unchecked`, then the outputs:

```json
"sheet": {
  "status": "no_blockers_found",
  "process": "sheet: 18 mm poplar plywood B/BB, 2500 × 1220, saw, kerf 4",
  "unchecked": ["load", "real sheet thickness (17.6–18.3 mm)", "grain appearance", "finish"],
  "proven": ["27 pieces are each one flat panel 18.0 mm", "every piece fits the sheet", "no two pieces overlap"],
  "cut_list": [
    { "pos": 1, "bodies": ["base1", "base2", "base3"], "cut_size_mm": [909, 304.7], "count": 3, "cuts": "2 angled cuts in the panel" },
    { "pos": 2, "bodies": ["front1", "front2", "front3"], "cut_size_mm": [827.8, 90], "count": 3, "cuts": "mitres: 26.4° at one end, 40° at the other" }
  ],
  "sheets": { "estimate": "1 sheet of 2500 × 1220", "cut_area_m2": 2.0, "share_of_sheet": "66%",
              "can_miss": "a layout that nests shaped pieces into each other's offcuts, which can only lower the count" },
  "bought": [{ "name": "LED profile 12×7, 24 V strip", "count": 8, "lengths_mm": [754, 754, 754, 754, 149, 149, 149, 149] }]
}
```

- **`cut_size_mm`, never `size`.** `size` already means the bounding box in
  every reply, and a model reads a field by its noun (WRITING_FOR_MODELS.md,
  rule 6). "Cut size" is OpenCutList's term too.
- **The cut list is complete, or it says it is not.** A truncated cut list
  is a wrong one. The reply carries up to 25 rows. Past that it carries a
  count and names `export_part` with `format: "cutlist"`.
- **`read_docs` gets three entries,** `sheet`, `made` and `bought`, each with
  one sentence, rules and a built example (rule 8). Draft rules for
  `sheet`:
  - One body is one piece, and a body made from a sheet must measure as one
    flat panel of its thickness.
  - Give each piece its own body. A function that returns several, spread
    into the returned object, keeps the part short.
  - The reply's `sheet.cut_list` is the cut list: quote it, never sizes
    worked out from your own script.
  - Bevels and mitres up to 45° from square are saw cuts. A curve or a pocket
    needs `cut: "router"`.
- **Field cases, before and after,** on the real MCP:
  - "a shelf from plywood": does the model make a body per piece, declare
    `sheet`, and quote `cut_list` instead of its own arithmetic?
  - "the ring": given v9's fused shelf declared as sheet, does it read the
    violation and split it?

## The example, measured

The picture is the shelves written this way, with a cut list that ParCAD
would produce. Nothing in it was typed in by hand:

1. **The script.** It uses v9's geometry with one body per piece: 27 pieces
   of plywood, 8 LED profiles, the wall and the air conditioner. The
   cleats' 2 mm flat is removed, because only the FDM check wanted it.
2. **The build.** Built with parcad 0.0.18 and exported to STEP.
3. **The measurement.** A Python prototype ran the checks above over
   `parcad --probe-step`, and the sheet was drawn from its output in the v9
   drawing style. It passes the layout check.

The prototype and the script are kept outside the repository, in
`~/Library/Application Support/Claude/side-session-notes/local_3440926e-4788-429e-bee4-aa8337e07859/sheet-goods/`,
with a README that reproduces the sheet. The numbers below are the record:

- **Panels:** all 27 pieces measure as one flat panel of 18.000 mm.
- **Overlaps:** none of the 666 pairs interferes.
- **Joints:** 42 wood-to-wood joints touch.
- **Rows:** the cut list has 10 rows.
- **Against the v9 shop order, 0.1 mm and 0.1° rounding aside, three things
  differ:**
  - Cleats 7 and 8 are 58 and 49 mm, not 56 and 47. That is the flat
    removed.
  - The window mitre measures 23.64°. The order rounded it twice, to 23.7°.
  - The back of the top shelf (138 mm) gets its own position, 10. The order
    had it as 6.1.
- **Sheets:** the guillotine heuristic fits all 27 cut sizes onto one
  2500 × 1220 sheet, with a 4 mm kerf and a 10 mm trim. The cut sizes cover
  2.00 m² of 3.05 (66 %). The hand layout in `cut_v9.py`, with the bases
  nested, also used one sheet.

## Order of work

This is step 4 of process-architecture.md ("`stock`, `board`, `sheet` and the
cut list"), in the corrected form above. Steps 1–3 come first: integrity split
from `print.rs`, the process data, and "relations: problems only".

1. **`sheet()`, `.made()` and `.bought()`.**
   - They go in `dsl.ts`, as `made` and `bought` on each `bodies` entry
     next to `reference`.
   - The brief gains a `process` key (`KEYS` and `BRIEF_KEYS`).
2. **The panel measurement in the kernel crate.** It goes beside
   `perceive.rs` and reuses the planar faces and wires that
   `Shape_geometry_json` already returns for `probe_step_export`. It
   measures thickness, through-cuts, the outline, the cut size, the tilts and
   congruence. **This is geometry work: give it to Fable.**
3. **The feather threshold split** between `fdm` and integrity, settled on
   the corpus.
4. **The cut list, the estimate and the bill in `service.rs`.** The
   guillotine heuristic is about 100 lines, and the prototype's version is
   the reference.
5. **Exports: the `cutlist` CSV, then `drawings`.** Before writing the
   drawing writer, evaluate the existing DXF and PDF crates and their
   licences (`check-prior-art-before-building`).
6. **The shelves become the first eval case and the first field case.** The
   eval case pins the 10 rows above.
