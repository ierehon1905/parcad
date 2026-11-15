import { describe, expect, it } from "bun:test";

import { bodyNameAt } from "./hidden-marks";

describe("bodyNameAt", () => {
  const source = `const lid = plate.union(lip);\nconst lidder = 1;\nreturn { base: box(1, 1, 1), lid };`;

  it("finds the declaration, not a longer name that starts the same", () => {
    expect(bodyNameAt(source, "lid")).toBe(source.indexOf("lid ="));
  });

  it("falls back to the key in the returned object", () => {
    expect(bodyNameAt(source, "base")).toBe(source.indexOf("base:"));
  });

  it("names nothing it cannot find", () => {
    expect(bodyNameAt(source, "hinge")).toBeUndefined();
  });
});
