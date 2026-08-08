/**
 * The shortest `>X and <Y` conjunction that picks one thing out of a set.
 *
 * A convenience for the inspector, never a hidden id: if the geometry later
 * becomes ambiguous the evaluator refuses, which is the whole point of naming
 * things by description.
 *
 * Written once and used for both edges and vertices. `epsilon` is a parameter
 * rather than a constant because the two callers legitimately disagree: a
 * vertex's tolerance is the grid its identity is rounded onto, so two vertices
 * closer than it are the same vertex, while an edge's centre is not rounded at
 * all and can afford to be stricter.
 */

const AXES = ["X", "Y", "Z"] as const;

/** A term that is not an extremum — today, an edge's `|Z` direction. */
export interface Directional<T> {
  /** The term this item earns, if any. */
  of: (item: T) => string | undefined;
  /** Whether a candidate also earns it. */
  matches: (candidate: T, axis: number) => boolean;
}

export function shortestUniqueSelector<T>(
  item: T,
  all: readonly T[],
  at: (item: T) => readonly number[],
  epsilon: number,
  directional?: Directional<T>,
): string | undefined {
  if (all.length === 0) return undefined;

  // Once per axis rather than once per comparison: `matches` is called inside
  // the narrowing loop, so recomputing these made it quadratic in the set.
  const span = AXES.map((_, axis) => {
    const values = all.map((candidate) => at(candidate)[axis]);
    return { max: Math.max(...values), min: Math.min(...values) };
  });

  const terms: string[] = [];
  const own = directional?.of(item);
  if (own) terms.push(own);
  for (let axis = 0; axis < 3; axis++) {
    if (Math.abs(at(item)[axis] - span[axis].max) <= epsilon) terms.push(`>${AXES[axis]}`);
    if (Math.abs(at(item)[axis] - span[axis].min) <= epsilon) terms.push(`<${AXES[axis]}`);
  }

  const matches = (candidate: T, term: string) => {
    const axis = AXES.indexOf(term[1] as (typeof AXES)[number]);
    if (term[0] === "|") return directional?.matches(candidate, axis) ?? false;
    const extreme = term[0] === ">" ? span[axis].max : span[axis].min;
    return Math.abs(at(candidate)[axis] - extreme) <= epsilon;
  };

  const selected: string[] = [];
  let candidates = all;
  while (candidates.length > 1) {
    const before = candidates.length;
    const next = terms
      .filter((term) => !selected.includes(term))
      .map((term) => ({ term, candidates: candidates.filter((c) => matches(c, term)) }))
      .filter(({ candidates: rest }) => rest.length > 0 && rest.length < before)
      .sort((a, b) => a.candidates.length - b.candidates.length)[0];
    if (!next) break;
    selected.push(next.term);
    candidates = next.candidates;
  }

  return candidates.length === 1 && selected.length > 0 ? selected.join(" and ") : undefined;
}
