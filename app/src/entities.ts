/** Ephemeral B-rep vertex inspection data derived from exact visible edges. */
export interface VertexPoint {
  /** Valid only for this evaluated shape; never accepted by the modelling DSL. */
  id: string;
  point: [number, number, number];
  /** Number of visible logical edges ending at this point. */
  degree: number;
}

interface EdgeEndpoint {
  points: number[][];
}

const VERTEX_PRECISION = 1000;
const EPSILON = 1 / VERTEX_PRECISION;

/**
 * Recover inspectable B-rep vertices from exact logical edge endpoints.
 *
 * The worker intentionally omits closed-curve seams from the visible topology.
 * Matching that rule here matters: the arbitrary parameter origin of a circle
 * is not a corner a semantic `.vertices(...)` selector can target.
 */
export function verticesFromEdges(edges: readonly EdgeEndpoint[]): VertexPoint[] {
  const found = new Map<string, Omit<VertexPoint, "id">>();

  for (const edge of edges) {
    const start = asPoint(edge.points[0]);
    const end = asPoint(edge.points[edge.points.length - 1]);
    if (!start || !end || keyFor(start) === keyFor(end)) continue;
    for (const point of [start, end]) {
      const key = keyFor(point);
      const existing = found.get(key);
      if (existing) existing.degree++;
      else found.set(key, { point, degree: 1 });
    }
  }

  return [...found]
    .sort(([a], [b]) => a.localeCompare(b, undefined, { numeric: true }))
    .map(([, vertex], index) => ({ id: `vertex@${index}`, ...vertex }));
}

/** Suggest the shortest unique directional query accepted by `.vertices(...)`. */
export function suggestVertexSelector(vertex: VertexPoint, all: readonly VertexPoint[]): string | undefined {
  if (all.length === 0) return undefined;
  const axes = ["X", "Y", "Z"] as const;
  const terms: string[] = [];

  for (let axis = 0; axis < 3; axis++) {
    const values = all.map((candidate) => candidate.point[axis]);
    if (Math.abs(vertex.point[axis] - Math.max(...values)) <= EPSILON) terms.push(`>${axes[axis]}`);
    if (Math.abs(vertex.point[axis] - Math.min(...values)) <= EPSILON) terms.push(`<${axes[axis]}`);
  }

  const matches = (candidate: VertexPoint, term: string) => {
    const axis = axes.indexOf(term[1] as (typeof axes)[number]);
    const values = all.map((other) => other.point[axis]);
    const extreme = term[0] === ">" ? Math.max(...values) : Math.min(...values);
    return Math.abs(candidate.point[axis] - extreme) <= EPSILON;
  };

  const selected: string[] = [];
  let candidates = all;
  while (candidates.length > 1) {
    const currentCount = candidates.length;
    const next = terms
      .filter((term) => !selected.includes(term))
      .map((term) => ({ term, candidates: candidates.filter((candidate) => matches(candidate, term)) }))
      .filter(({ candidates: remaining }) => remaining.length > 0 && remaining.length < currentCount)
      .sort((a, b) => a.candidates.length - b.candidates.length)[0];
    if (!next) break;
    selected.push(next.term);
    candidates = next.candidates;
  }

  return candidates.length === 1 && selected.length > 0 ? selected.join(" and ") : undefined;
}

function asPoint(point: number[] | undefined): [number, number, number] | undefined {
  if (!point || point.length < 3 || !point.slice(0, 3).every(Number.isFinite)) return undefined;
  return [point[0], point[1], point[2]];
}

function keyFor(point: [number, number, number]): string {
  return point.map((value) => Math.round(value * VERTEX_PRECISION)).join(",");
}
