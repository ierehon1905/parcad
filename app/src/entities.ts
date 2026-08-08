import { shortestUniqueSelector } from "./shortest-selector";

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
export function suggestVertexSelector(
  vertex: VertexPoint,
  all: readonly VertexPoint[],
): string | undefined {
  return shortestUniqueSelector(vertex, all, (v) => v.point, EPSILON);
}

function asPoint(point: number[] | undefined): [number, number, number] | undefined {
  if (!point || point.length < 3 || !point.slice(0, 3).every(Number.isFinite)) return undefined;
  return [point[0], point[1], point[2]];
}

function keyFor(point: [number, number, number]): string {
  return point.map((value) => Math.round(value * VERTEX_PRECISION)).join(",");
}
