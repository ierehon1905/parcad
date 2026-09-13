// Every upright corner already rounded; "the upright edges" now names only
// the eight tangent lines those fillets left. There is nothing to round,
// and the refusal says which edges it was looking at.
return box(40, 30, 20).edges("|Z").expect({ count: 4 }).fillet(5).edges("|Z").fillet(1);
