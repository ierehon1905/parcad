/**
 * Which script lines made each face: the kernel's `made_by` read against the
 * stack each graph node was created on.
 *
 * Pure, so it is tested in bun and shared by whatever asks. Every input is of
 * one evaluation: node ids index that build's graph, line numbers that build's
 * source.
 */

/** Consecutive steps of a face's story written on one line, folded into one. */
export interface Row {
  line: number;
  nodes: number[];
}

export class Provenance {
  /** For each node, the line its story is told on; 0 where no script line made it. */
  private readonly primary: number[];
  private readonly facesOfNode = new Map<number, number[]>();

  /**
   * @param frames for each graph node, the script lines on its creation stack, innermost first
   * @param chains for each face, `FaceSummary.made_by`: one chain of node ids per maker, leaf first
   */
  constructor(
    private readonly frames: readonly (readonly number[])[],
    private readonly chains: readonly (readonly (readonly number[])[])[],
  ) {
    // A line is a helper's when what calls it is written in more than one place:
    // `const alongX = (o, l) => extrude(o, l).rotate(...)` says nothing about
    // which of its callers a face came from, so the story is told at the caller.
    const callers = new Map<number, Set<number>>();
    for (const lines of frames) {
      const own = distinctRuns(lines);
      for (let i = 0; i + 1 < own.length; i++) {
        if (!callers.has(own[i])) callers.set(own[i], new Set());
        callers.get(own[i])!.add(own[i + 1]);
      }
    }
    const helper = (line: number) => (callers.get(line)?.size ?? 0) >= 2;
    this.primary = frames.map((lines) => {
      const own = distinctRuns(lines);
      return own.find((line) => !helper(line)) ?? own[own.length - 1] ?? 0;
    });

    chains.forEach((made, face) => {
      for (const chain of made)
        for (const node of chain) {
          const faces = this.facesOfNode.get(node) ?? [];
          if (faces[faces.length - 1] !== face) faces.push(face);
          this.facesOfNode.set(node, faces);
        }
    });
  }

  /** The face's story, step by step: its first maker's chain, one row per line it passes. */
  rowsOf(face: number): Row[] {
    const rows: Row[] = [];
    for (const node of this.chains[face]?.[0] ?? []) {
      const line = this.primary[node];
      if (!line) continue;
      const row = rows.find((r) => r.line === line);
      if (row) row.nodes.push(node);
      else rows.push({ line, nodes: [node] });
    }
    return rows;
  }

  /** Every face a step's nodes made or passed on. */
  facesOfRow(row: Row): Set<number> {
    const faces = new Set<number>();
    for (const node of row.nodes) for (const face of this.facesOfNode.get(node) ?? []) faces.add(face);
    return faces;
  }

  /**
   * The feature a face is part of: what its first line made, narrowed to the
   * one use of it the face came through. A helper called for three keys made
   * all three; the face is on one, and so is its feature.
   */
  featureFaces(face: number): Set<number> {
    const [made, used] = this.rowsOf(face);
    if (!made) return new Set();
    const faces = this.facesOfRow(made);
    if (!used) return faces;
    const through = this.facesOfRow(used);
    return new Set([...faces].filter((f) => through.has(f)));
  }

  /**
   * What a line of the script did to the finished part: the faces it made,
   * and the faces made elsewhere that it shaped on their way.
   */
  facesOfLine(line: number): { made: Set<number>; shaped: Set<number> } {
    const made = new Set<number>();
    const shaped = new Set<number>();
    this.chains.forEach((chains, face) => {
      const maker = new Set(this.rowsOf(face)[0]?.nodes ?? []);
      for (const chain of chains)
        for (const node of chain) {
          if (!this.frames[node]?.includes(line)) continue;
          (maker.has(node) ? made : shaped).add(face);
        }
    });
    for (const face of made) shaped.delete(face);
    return { made, shaped };
  }
}

/** A stack's lines with each run of one line, a recursion or a chain written on it, said once. */
function distinctRuns(lines: readonly number[]): number[] {
  return lines.filter((line, i) => i === 0 || lines[i - 1] !== line);
}

/**
 * Every script line on a creation stack, innermost first.
 *
 * @param stack `Error.stack` as the engine formats it, any engine
 * @param url the `sourceURL` the script was compiled under
 * @param header lines the compiler put above the script
 */
export function stackLines(stack: string | undefined, url: string, header: number | undefined): number[] {
  if (!stack || header === undefined) return [];
  const escaped = url.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return [...stack.matchAll(new RegExp(`${escaped}:(\\d+):\\d+`, "g"))]
    .map((m) => Number(m[1]) - header)
    .filter((line) => line >= 1);
}
