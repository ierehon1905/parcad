// A 12-tooth, module 2, 20° spur pinion 10 thick, profile shifted out by
// 0.3 module: unshifted, a hob would undercut it (12 < 17.1), and the least
// shift that avoids that is 1 - 6 sin²20° = 0.298. The tip moves out to
// 12 + 2 (1 + 0.3) = 14.6 and the root to 12 - 2 (1.25 - 0.3) = 10.1; the
// tooth is 2 (π/2 + 2 · 0.3 · tan 20°) = 3.5784 thick on the reference circle.
return extrude(spurGearOutline({ module: 2, teeth: 12, profileShift: 0.3 }), 10).tag("pinion");
