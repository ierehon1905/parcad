// A 40 x 40 x 10 block. From above, everything over a plane through
// (0, 0, 6) that falls 15° towards +Y goes; from below, a channel 10 wide up to
// z = 5. The plane reaches z = 5 at y = 1 / tan 15° = 3.732, so over the
// channel the material between its ceiling and the ramp tapers to nothing
// along that line, at 15°: the desk stand's cable-to-slot sliver.
const angle = -15;
const a = (angle * Math.PI) / 180;
const ramp = box(80, 80, 20)
  .rotate("x", angle)
  .at(0, -10 * Math.sin(a), 6 + 10 * Math.cos(a))
  .tag("ramp");
const channel = box(10, 60, 6).at(0, 0, 2).tag("channel");
return box(40, 40, 10).at(0, 0, 5).tag("block").cut(ramp, channel);
