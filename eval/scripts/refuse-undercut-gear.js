// The textbook 17-tooth, 20° gear: a hob undercuts it unless the profile is
// shifted out by 1 - 8.5 sin²20° = 0.0057, and this one is not shifted.
return extrude(spurGearOutline({ module: 2, teeth: 17 }), 10);
