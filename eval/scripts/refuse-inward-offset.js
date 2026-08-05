// The exact backend only grows. An inward offset is a different operation with
// different failure modes (self-intersection, vanishing faces), so it is
// refused rather than routed through the same code path.
return box(30, 20, 10).offset(-2);
