// Shell is `solid − solid.offset_surface(−t)`, not the wrapper's hollow(),
// which returned a shrunken solid instead of a hollow one. The volume below is
// what distinguishes those two outcomes.
return box(64, 39, 22).shell(2);
