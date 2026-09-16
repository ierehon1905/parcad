/**
 * The DSL's half of `eval/sections.json`: what `checkSection` says, reached
 * through `extrude()`, about each outline the section fuzz kept. The core and
 * the kernel are held to the same file by `agrees_with_the_shared_section_corpus`
 * in `section_crossing.rs` and `backend.rs`. Run this with `bun test` from `app/`.
 */

import { expect, test } from "bun:test";
import corpus from "../../eval/sections.json";
import { extrude, type SectionEntry } from "./dsl";

test("agrees with the shared section corpus", () => {
  for (const testCase of corpus.cases as Array<{ why: string; from: string; outline: unknown[]; dsl: { ok?: true; refuses?: string } }>) {
    let got: string | undefined;
    try {
      extrude(testCase.outline as SectionEntry[], 2);
    } catch (e) {
      got = (e as Error).message;
    }
    expect({ case: `${testCase.from} (${testCase.why})`, refuses: got }).toEqual({
      case: `${testCase.from} (${testCase.why})`,
      refuses: testCase.dsl.refuses,
    });
  }
});
