// A bow tie: four corners listed out of order, so two edges cross. A
// re-entrant outline builds now; one that crosses itself bounds no region, and
// the graph names the two edges that meet.
return extrude([[0, 0], [10, 10], [10, 0], [0, 10]], 2);
