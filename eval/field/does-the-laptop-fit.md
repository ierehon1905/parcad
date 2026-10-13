---
tool: check_fit
reach: check_fit
verdict: FIT\s*[=:]\s*CLEAR\b[\s\S]*CLEARANCE\s*[=:]\s*0\.50*\s*mm
trap: CLEARANCE\s*[=:]\s*1(\.0+)?\s*mm
quote: \b0\.5(0+)?\s*mm
why: |
  The question every holder exists to answer, and the one a render cannot:
  does the object fit, and by how much. The tray's pocket is drawn 1 mm
  larger than the laptop all round and the laptop is placed half a millimetre
  above its floor, so the honest answer is clear by exactly 0.5 mm at the
  floor, measured on the two exact solids. A model that compares dimensions
  in the script answers 1 mm; one that reasons from the pocket's walls says
  it touches; only check_fit says 0.5, and only check_fit can. The reference
  is a one-line box, which is what the tool's description shows. The first
  round of this case shipped its tray with the cutter 20 mm too high; every trial read the no-op-cut refusal, moved
  the cutter itself, and measured its own corrected tray. The second round
  gave the laptop's height in prose, and three of four trials placed it by
  their own arithmetic instead and measured that, honestly, at 1.0. Both
  rounds were sound about the tool and wrong about the case, which is why
  the tray is checked with `parcad --fit` before a round and the reference
  is handed over as a script to use unchanged.
---
Use the parcad MCP tools.

Here is a parcad script for a tray that a 16" MacBook Pro lies in:

```js
const laptop = { length: 355.7, width: 248.1, thickness: 16.8 };
const clear = 1;
const floor = 8;
const wall = 4;
const block = box(laptop.length + 2 * (clear + wall), laptop.width + 2 * (clear + wall), floor + 12)
  .at(0, 0, (floor + 12) / 2);
const pocket = box(laptop.length + 2 * clear, laptop.width + 2 * clear, laptop.thickness + 2 * clear)
  .at(0, 0, floor + clear + laptop.thickness / 2);
return block.cut(pocket).tag("tray");
```

The laptop lies in it where this script puts it, and this script is the reference to measure against, unchanged:

```js
return box(355.7, 248.1, 16.8).at(0, 0, 16.9);
```

Question: does the laptop fit, and what is the clearance in millimetres where it is closest to the tray?

Rules: the answer must come from a parcad tool that measures the tray against the laptop, not from comparing the numbers in the script, and the laptop must be measured where the reference script puts it, not where you would put it. End with two lines in exactly this form:

FIT = <CLEAR or TOUCHING or INTERFERING>
CLEARANCE = <number> mm
