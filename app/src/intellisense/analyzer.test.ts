/**
 * What the editor is allowed to say about a part.
 *
 * Two questions, and the second is the one that makes this worth running. The
 * first — does a real mistake get reported — is easy and every checker passes
 * it. The second is whether *correct* parts come out clean, and a type checker
 * aimed at hand-written JavaScript fails that one by default: the seed corpus
 * produced ten complaints before `UNPROVABLE_LENGTH`, every one of them about
 * an array literal whose length TypeScript cannot prove and a part cannot
 * annotate. A checker that cries wolf on the shipped examples is worse than no
 * checker, so the corpus is the gate.
 */

import { describe, expect, test } from "bun:test";
import { readFileSync, readdirSync } from "node:fs";
import { resolve } from "node:path";

import * as dsl from "../dsl";
import { createAnalyzer, type AnalyzerSources } from "./analyzer";
import { globalsFor, toCompiler, toDocument } from "./part-file";

const root = resolve(import.meta.dir, "../../..");
const libDir = resolve(root, "app/node_modules/typescript/lib");

const sources: AnalyzerSources = {
  libs: Object.fromEntries(
    readdirSync(libDir)
      .filter((name) => /^lib\.(es|decorators)[\w.]*\.d\.ts$/.test(name) && !name.includes(".full."))
      .map((name) => [name, readFileSync(resolve(libDir, name), "utf8")]),
  ),
  modules: {
    "/dsl.ts": readFileSync(resolve(root, "app/src/dsl.ts"), "utf8"),
    "/selectors.ts": readFileSync(resolve(root, "app/src/selectors.ts"), "utf8"),
  },
  names: Object.keys(dsl),
};

const analyzer = createAnalyzer(sources);
const problemsFor = (source: string) => {
  analyzer.setPart(source);
  return analyzer.diagnostics();
};

describe("the language the editor compiles", () => {
  test("has nothing wrong with it under the editor's own options", () => {
    // `dsl.ts` is written under `strict`, and the editor reads parts under a
    // looser setting so that hand-written JavaScript is not hammered. Where
    // the two disagree the DSL stops compiling and every part below it loses
    // its types — which is how `strictNullChecks` came to be on: without it a
    // `??` whose left side cannot be null keeps the right side in the type.
    expect(analyzer.languageProblems()).toEqual([]);
  });
});

describe("the seed parts", () => {
  const examples = readdirSync(resolve(root, "examples")).filter((name) => name.endsWith(".js"));

  test("there are some", () => {
    expect(examples.length).toBeGreaterThan(20);
  });

  for (const name of examples) {
    test(`${name} has nothing wrong with it`, () => {
      const problems = problemsFor(readFileSync(resolve(root, "examples", name), "utf8"));
      expect(problems.map((problem) => problem.message)).toEqual([]);
    });
  }
});

describe("a mistake is still reported", () => {
  const cases: [string, string, string][] = [
    ["a misspelt method", "return box(1, 2, 3).edges('>Z').filet(2);", "fillet"],
    ["a missing argument", "return cylinder(3);", "Expected 2 arguments"],
    ["an option that does not exist", "return box(1,2,3).edges('>Z').fillet(2, { radiuss: 1 });", "radiuss"],
    ["a string where a number goes", "return box(10, 'wide', 30);", "not assignable"],
    ["a name that does not exist", "return boxx(1, 2, 3);", "Cannot find name"],
    ["a type annotation, which a part cannot have", "const t: number = 3;\nreturn box(t, t, t);", "TypeScript files"],
    ["a literal of the wrong length", "return extrude([[0,0],[1,1,1],[2,2]], 5);", "not assignable"],
  ];
  for (const [what, source, says] of cases) {
    test(what, () => {
      const problems = problemsFor(source);
      expect(problems.length).toBeGreaterThan(0);
      expect(problems.map((problem) => problem.message).join("\n")).toContain(says);
    });
  }
});

describe("a length TypeScript cannot prove is not a mistake", () => {
  test("points computed by a map", () => {
    expect(problemsFor("const p = [1,2,3].map((i) => [i, i * 2]);\nreturn extrude(p, 5);")).toEqual([]);
  });
  test("a path computed by a map", () => {
    expect(problemsFor("const p = [1,2].map((i) => [i, 0, i]);\nreturn pipe(p, 2);")).toEqual([]);
  });
});

describe("a part may not rebind the DSL", () => {
  test("const, which stops the script parsing", () => {
    const problems = problemsFor("const box = 3;\nreturn cylinder(1, 2);");
    expect(problems.some((problem) => problem.message.includes("part of the DSL"))).toBe(true);
  });

  test("the same by destructuring", () => {
    const problems = problemsFor("const { hull } = {};\nreturn cylinder(1, 2);");
    expect(problems.some((problem) => problem.message.includes("part of the DSL"))).toBe(true);
  });

  test("but var and function may, because JavaScript lets them", () => {
    // `new Function("box", "var box = 3")` is legal; `const` is not. The editor
    // has to draw that line where the runtime draws it.
    expect(new Function("box", "var box = 3; return box;")(1)).toBe(3);
    const problems = problemsFor("var box = 3;\nreturn cylinder(1, 2);");
    expect(problems.some((problem) => problem.message.includes("part of the DSL"))).toBe(false);
  });
});

describe("what a hover says", () => {
  const source = 'const plate = box(80, 60, 8);\nreturn plate.edges(">Z").fillet(2);\n';

  test("a function carries its signature, its prose and its example", () => {
    analyzer.setPart('return holeFor("M5", 6);');
    const info = analyzer.quickInfo(8)!;
    expect(info.signature.map((span) => span.text).join("")).toContain("thread: string");
    expect(info.documentation.map((span) => span.text).join("")).toContain("named fastener");
    expect(info.tags.map((tag) => tag.name)).toContain("example");
  });

  test("never the remarks, which are why rather than how", () => {
    analyzer.setPart('return holeFor("M5", 6);');
    expect(analyzer.quickInfo(8)!.tags.map((tag) => tag.name)).not.toContain("remarks");
  });

  test("the span it reports is the name under the pointer", () => {
    analyzer.setPart(source);
    const info = analyzer.quickInfo(source.indexOf("box(") + 1)!;
    expect(source.slice(info.from, info.to)).toBe("box");
  });

  test("a link in a doc comment reads as the name, not as its markup", () => {
    const source = "const p = box(1, 2, 3);\nreturn p.at(1, 2, 3);";
    analyzer.setPart(source);
    const info = analyzer.quickInfo(source.indexOf(".at(") + 2)!;
    // TypeScript returns `{@link `, `translate`, `}` as three parts; the braces
    // are markup and only the name is prose.
    expect(info.documentation.filter((span) => span.kind === "link").length).toBe(2);
    expect(info.documentation.some((span) => span.kind === "linkName")).toBe(true);
  });

  test("a constant carries its type", () => {
    analyzer.setPart(source);
    const info = analyzer.quickInfo(source.indexOf("plate.edges") + 2)!;
    expect(info.signature.map((span) => span.text).join("")).toBe("const plate: Shape");
  });
});

describe("what a completion offers", () => {
  test("the DSL, by name", () => {
    analyzer.setPart("const a = cyl");
    const labels = analyzer.completions(13).map((entry) => entry.label);
    expect(labels).toContain("cylinder");
  });

  test("never the DSL's own plumbing", () => {
    // TypeScript answers with everything in scope and the editor narrows it to
    // what was typed, so the only place this can be kept out is here.
    analyzer.setPart("const a = __");
    const labels = analyzer.completions(12).map((entry) => entry.label);
    expect(labels).not.toContain("__parcadTreatmentSource");
    expect(labels).toContain("around");
  });

  test("the methods of what the expression is", () => {
    const source = "const plate = box(1, 2, 3);\nplate.";
    analyzer.setPart(source);
    const labels = analyzer.completions(source.length).map((entry) => entry.label);
    expect(labels).toContain("fillet");
    expect(labels).toContain("edges");
    expect(labels).not.toContain("cylinder");
  });
});

describe("what the panel beside a completion says", () => {
  // The panel was a signature and nothing else for three commits, because a
  // DSL name is declared as an alias with no comment of its own and only a
  // hover resolves the alias. A check that reads the reply cannot see that a
  // panel is empty on screen; a check that reads the reply's documentation can.
  test("a DSL name carries the same prose and example as its hover", () => {
    analyzer.setPart("const q = hole");
    const detail = analyzer.completionDetail(14, "holeFor")!;
    expect(detail.documentation.map((span) => span.text).join("")).toContain("named fastener");
    expect(detail.tags.map((tag) => tag.name)).toContain("example");
  });

  test("and so does a method", () => {
    const source = "const p = box(1, 2, 3);\np.";
    analyzer.setPart(source);
    const detail = analyzer.completionDetail(source.length, "fillet")!;
    expect(detail.documentation.map((span) => span.text).join("")).toContain("Round the edges");
  });
});

describe("which argument is being typed", () => {
  test("the second, after the comma", () => {
    const source = 'return holeFor("M5", ';
    analyzer.setPart(source);
    const help = analyzer.signatureHelp(source.length)!;
    expect(help.argument).toBe(1);
    expect(help.signatures[0].params[1].label.map((span) => span.text).join("")).toContain("depth");
  });
});

describe("the wrapper the compiler needs", () => {
  test("is invisible to every offset that crosses it", () => {
    expect(toDocument(toCompiler(17), 100)).toBe(17);
    expect(toDocument(0, 100)).toBe(0);
    expect(toDocument(10_000, 40)).toBe(40);
  });

  test("declares exactly the names the runtime binds", () => {
    const declared = globalsFor(Object.keys(dsl))
      .split("\n")
      .map((line) => line.replace(/^declare const (\w+).*$/, "$1"));
    expect(declared).toEqual(Object.keys(dsl));
  });
});
