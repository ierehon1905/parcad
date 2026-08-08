// The coincident-face trap's sealed half, at the shipped 0.004 mm: a 30 x 20
// pocket cut 4 mm into a 60 x 40 x 20 plate, with the cutter's outer face
// stopping 0.004 mm inside the top face it was meant to enter. The tool breaks
// no face, so this is not a pocket with a thin lid — it is no pocket at all: a
// solid block with the tool's shape entombed as a closed void, watertight and
// plausible in every render. One extra shell is the whole difference from a
// blind pocket, which is why the refusal is topological and needs no
// threshold; exactly coincident and proud cutters build the identical correct
// part and stay silent (v-block ships the coincident case).
const SHORT = 0.004;
const plate = box(60, 40, 20);
const h = 4 - SHORT; // from the pocket floor at z = 6 up to z = 10 - SHORT
return plate.cut(box(30, 20, h).at(0, 0, 6 + h / 2));
