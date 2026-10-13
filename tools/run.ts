#!/usr/bin/env bun
/**
 * Headless DSL runner: script in, intent graph JSON out.
 *
 *   bun tools/run.ts examples/bracket.js > graph.json
 *   ./target/release/parcad graph.json --out out --regions
 *   bun tools/run.ts part.js --parts ~/Library/Application\ Support/parcad
 *
 * `importPart(name)` reads `name.parcad/part.js` or `name.js` under `--parts`,
 * or beside the script when that is not given.
 *
 * The GUI runs scripts in the webview and this runs them in bun, but both use
 * the same DSL and both hand the same JSON to the same core. Keeping the script
 * layer outside the Rust binary is what lets that be true.
 */

import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import * as dsl from "../app/src/dsl";
import { Shape, build } from "../app/src/dsl";

const args = process.argv.slice(2);
const partsAt = args.indexOf("--parts");
const partsDir = partsAt >= 0 ? args.splice(partsAt, 2)[1] : undefined;
const file = args[0];
if (!file || (partsAt >= 0 && !partsDir)) {
  console.error("usage: bun tools/run.ts <script.js> [--parts DIR]");
  process.exit(2);
}

const source = readFileSync(file, "utf8");
const names = Object.keys(dsl);
const root = partsDir ?? dirname(file);

/** A saved part's source by name, in either of the forms `projects.rs` reads. */
function readPart(name: string): string {
  const segments = name.split("/");
  if (name.startsWith("/") || segments.some((s) => s === "" || s === "." || s === "..")) {
    throw new Error(`importPart(${JSON.stringify(name)}): a part's name is a path inside the parts folder, like "Fasteners/spacer"`);
  }
  for (const path of [join(root, `${name}.parcad`, "part.js"), join(root, `${name}.js`)]) {
    if (existsSync(path)) return readFileSync(path, "utf8");
  }
  throw new Error(`importPart(${JSON.stringify(name)}): no part at ${join(root, name)}(.parcad|.js); pass --parts DIR for another folder`);
}
dsl.__parcadReadPartsWith(readPart, dsl as Record<string, unknown>);

let fn: (...args: unknown[]) => unknown;
try {
  fn = new Function(...names, source) as (...args: unknown[]) => unknown;
} catch (e) {
  const shadowed = dsl.__parcadShadowedBuiltins(source, names);
  const why = shadowed.length ? dsl.__parcadShadowedBuiltinMessage(shadowed, names) : (e as Error).message;
  console.error(`${file} did not parse: ${why}`);
  process.exit(1);
}

const result = fn(...names.map((n) => (dsl as Record<string, unknown>)[n]));
if (!(result instanceof Shape) && (typeof result !== "object" || result === null)) {
  console.error(
    `${file} must return a shape, or an object of named shapes for a part in several bodies, ` +
      "e.g.  return body.cut(hole)   or   return { base, lid }",
  );
  process.exit(1);
}

try {
  console.log(JSON.stringify(build(result as dsl.Part), null, 2));
} catch (e) {
  console.error(`${file}: ${(e as Error).message}`);
  process.exit(1);
}
