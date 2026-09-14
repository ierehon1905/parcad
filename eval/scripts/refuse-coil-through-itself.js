// A coil wound tighter than its own wire: a 2 mm wire at pitch 2 touches the
// next turn all the way up, and anything under that sweeps through it. OCCT
// builds the self-intersecting surface without a word, so the graph refuses
// it first, naming the pitch that would clear.
return pipe({ helix: { radius: 10, pitch: 2, turns: 3 } }, 2);
