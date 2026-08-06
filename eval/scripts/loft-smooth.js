// A smooth loft through three sections: one fitted surface, Fusion's default
// look. There is no closed form for the fitted wall, so the recorded numbers
// are the kernel's own, held to catch regression rather than derived — what
// this case *proves* is different from the frustum's arithmetic:
//
// - the smooth path through OCCT's ThruSections stays alive and watertight;
// - the backend's containment gate agrees the fit stayed inside the
//   sections' bounding box (a fit that bulged past it is refused, so this
//   case passing is also that check measuring clean);
// - the bounding box below equals the sections' own extent, which is the
//   claim measure.rs makes for the mesher and renderer.
return loft(
  [
    { z: 0, outline: [[-20, -20], [20, -20], [20, 20], [-20, 20]] },
    { z: 15, outline: [[-18, -18], [18, -18], [18, 18], [-18, 18]] },
    { z: 30, outline: [[-10, -10], [10, -10], [10, 10], [-10, 10]] },
  ],
  { smooth: true },
).tag("fitted");
