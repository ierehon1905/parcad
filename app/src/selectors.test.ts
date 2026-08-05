/**
 * The editor's selector parser, held to the same corpus as the kernel's.
 *
 * `crates/parcad-core/src/selectors.rs` has a test of the same name reading the
 * same file. Neither implementation is the specification; `eval/selectors.json`
 * is. Run this with `bun test` from `app/`.
 */

import { expect, test } from "bun:test";
import corpus from "../../eval/selectors.json";
import {
  parseEdgeSelector,
  parseVertexSelector,
  SelectorSyntaxError,
  termSource,
  type SpannedTerm,
} from "./selectors";

interface Expected {
  terms?: string[];
  spans?: [number, number][];
  error?: { message: string; span: [number, number] };
}

test("agrees with the shared selector corpus", () => {
  for (const testCase of corpus.cases as Array<{
    why: string;
    selector: string;
    edge: Expected;
    vertex: Expected;
  }>) {
    for (const [kind, expected, parse] of [
      ["edge", testCase.edge, parseEdgeSelector],
      ["vertex", testCase.vertex, parseVertexSelector],
    ] as const) {
      const at = `${kind} ${JSON.stringify(testCase.selector)} (${testCase.why})`;

      let parsed: SpannedTerm[] | undefined;
      let failure: SelectorSyntaxError | undefined;
      try {
        parsed = parse(testCase.selector);
      } catch (e) {
        if (!(e instanceof SelectorSyntaxError)) throw e;
        failure = e;
      }

      if (expected.terms) {
        expect(failure && `${at}: unexpectedly rejected: ${failure.message}`).toBeUndefined();
        expect([at, parsed!.map(termSource)]).toEqual([at, expected.terms]);
        if (expected.spans) {
          expect([at, parsed!.map((term) => [term.from, term.to])]).toEqual([at, expected.spans]);
        }
      } else {
        expect(failure && at).toBe(at);
        expect([at, failure!.message]).toEqual([at, expected.error!.message]);
        expect([at, [failure!.from, failure!.to]]).toEqual([at, expected.error!.span]);
      }
    }
  }
});
