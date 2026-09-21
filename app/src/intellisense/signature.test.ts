/**
 * A signature breaks where a reader expects it to, or not at all.
 *
 * The cases are real quick-info output, spans and all — `holeFor` is the one
 * that started this, and the method form is the one a naive bracket count gets
 * wrong, because `(method)` is a bracket group that is not a parameter list.
 */

import { describe, expect, test } from "bun:test";

import type { Span } from "./analyzer";
import { render, wrapSignature } from "./signature";

/** A signature as spans, from a string where `|` separates the tokens. */
const spans = (source: string): Span[] =>
  source.split("|").map((text) => ({
    text,
    kind: /^[(){}[\],:<>.]$|^=>$/.test(text)
      ? "punctuation"
      : text.trim() === ""
        ? "space"
        : "text",
  }));

const wrap = (source: string, columns = 40) => render(wrapSignature(spans(source), columns));

describe("a signature that already fits", () => {
  test("is not touched", () => {
    const source = "const| |plate|:| |Shape";
    expect(wrap(source)).toBe("const plate: Shape");
  });

  test("however many parameters it has", () => {
    const source = "box|(|x|:| |number|,| |y|:| |number|)|:| |Shape";
    expect(wrap(source, 60)).toBe("box(x: number, y: number): Shape");
  });
});

describe("a signature that does not fit", () => {
  test("puts one parameter on each line", () => {
    const source =
      "const|" +
      " |holeFor|:| |(|thread|:| |string|,| |depth|:| |number|,| |options|?|:| |Fit|)| |=>| |Shape";
    expect(wrap(source, 40)).toBe(
      [
        "const holeFor: (",
        "    thread: string,",
        "    depth: number,",
        "    options?: Fit",
        ") => Shape",
      ].join("\n"),
    );
  });

  test("and knows `(method)` is not the parameter list", () => {
    const source =
      "(|method|)| |EdgeSelection|.|fillet|(|radius|:| |number|,| |options|?|:| |FilletOptions|)|:| |Shape";
    expect(wrap(source, 40)).toBe(
      [
        "(method) EdgeSelection.fillet(",
        "    radius: number,",
        "    options?: FilletOptions",
        "): Shape",
      ].join("\n"),
    );
  });

  test("and does not split a generic's own comma", () => {
    const source =
      "const|" +
      " |sizes|:| |(|table|:| |Record|<|string|,| |number|>|,| |fit|:| |Fit|)| |=>| |Shape";
    expect(wrap(source, 30)).toBe(
      ["const sizes: (", "    table: Record<string, number>,", "    fit: Fit", ") => Shape"].join(
        "\n",
      ),
    );
  });

  test("and indents a block TypeScript had already broken", () => {
    // Wide columns on purpose: this breaks because TypeScript had already
    // broken it, not because any one line is too long. A signature that is
    // going to be four lines either way is better with its parameters aligned.
    const source =
      "const|" +
      " |f|:| |(|a|:| |string|,| |o|?|:| |{\n    b?: boolean;\n}|)| |=>| |Shape";
    expect(wrap(source, 200)).toBe(
      [
        "const f: (",
        "    a: string,",
        "    o?: {",
        "        b?: boolean;",
        "    }",
        ") => Shape",
      ].join("\n"),
    );
  });
});

describe("a signature it cannot improve", () => {
  test("no parameter list at all", () => {
    const source = "const|" + " |METRIC|:| |Record|<|string|,| |{| |d|:| |number| |}|>";
    expect(wrap(source, 20)).toBe(render(spans(source)));
  });

  test("an empty parameter list", () => {
    const source = "const|" + " |aVeryLongNameIndeed|:| |(|)| |=>| |Shape";
    expect(wrap(source, 20)).toBe(render(spans(source)));
  });
});
