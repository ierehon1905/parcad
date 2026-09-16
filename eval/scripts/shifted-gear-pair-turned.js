// The shifted pair of shifted-gear-pair.js, rolled on by 7° of the pinion
// (2.8° of the wheel the other way): involute flanks keep the same gap along
// the line of action at every angle, so the flanks in contact are still 0.05
// apart.
const pair = spurGearPair({ module: 2, teeth: [12, 30], profileShift: [0.3, 0], backlash: 0.1 });
const roll = 7;
const pinion = extrude(pair.outlines[0], 8).rotate("z", roll);
const wheel = extrude(pair.outlines[1], 8)
  .rotate("z", pair.turn - (roll * 12) / 30)
  .at(pair.centres, 0, 0);
return { pinion, wheel };
