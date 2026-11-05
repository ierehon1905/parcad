// A drawer in its frame, 0.3 mm clear on each side and below, 0.3 above:
// pulled 60 mm out along +y it stays clear; pushed 5 mm up it hits the top.
const frame = box(60, 80, 40).at(0, 0, 20).cut(box(50, 82, 30).at(0, 0, 20));
const drawer = box(49.4, 70, 29.4).at(0, 0, 20).cut(box(45, 66, 26).at(0, 0, 22));
return {
  frame,
  drawer,
  checks: [
    { slides: { body: "drawer", along: "y", mm: [0, 60] }, clearOf: ["frame"], atLeast: 0.25 },
    { slides: { body: "drawer", along: "z", mm: [0, 5] }, clearOf: ["frame"], atLeast: 0.25 },
  ],
};
