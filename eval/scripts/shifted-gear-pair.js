// A module 2 pair, 12 and 30 teeth, the pinion shifted out by 0.3 and the
// wheel not at all, with 0.1 mm of backlash: spurGearPair moves the wheel out
// to the centre distance the shift needs and thins every tooth by half the
// backlash, so centred as returned the flanks in contact are 0.05 apart.
const pair = spurGearPair({ module: 2, teeth: [12, 30], profileShift: [0.3, 0], backlash: 0.1 });
const pinion = extrude(pair.outlines[0], 8);
const wheel = extrude(pair.outlines[1], 8).rotate("z", pair.turn).at(pair.centres, 0, 0);
return { pinion, wheel };
