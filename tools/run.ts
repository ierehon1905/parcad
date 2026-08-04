#!/usr/bin/env bun
/**
 * Headless DSL runner: script in, intent graph JSON out.
 *
 *   bun tools/run.ts examples/bracket.js > graph.json
 *   ./target/release/parcad graph.json --out out --regions
 *
 * The GUI runs scripts in the webview and this runs them in bun, but both use
 * the same DSL and both hand the same JSON to the same core. Keeping the script
 * layer outside the Rust binary is what lets that be true.
 */

import { readFileSync } from "node:fs";
import * as dsl from "../app/src/dsl";
import { Shape, build } from "../app/src/dsl";

const file = process.argv[2];
if (!file) {
  console.error("usage: bun tools/run.ts <script.js>");
  process.exit(2);
}

const source = readFileSync(file, "utf8");
const names = Object.keys(dsl);

let fn: (...args: unknown[]) => unknown;
try {
  fn = new Function(...names, source) as (...args: unknown[]) => unknown;
} catch (e) {
  console.error(`${file} did not parse: ${(e as Error).message}`);
  process.exit(1);
}

const result = fn(...names.map((n) => (dsl as Record<string, unknown>)[n]));
if (!(result instanceof Shape)) {
  console.error(`${file} must return a shape, e.g.  return body.cut(hole)`);
  process.exit(1);
}

console.log(JSON.stringify(build(result), null, 2));
