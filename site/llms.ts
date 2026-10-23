// The site for a model to read: llms.txt (llmstxt.org), the page as Markdown,
// every read_docs topic as a page of its own, all of it in llms-full.txt, and a
// sitemap. Run after `react-router build`; writes beside the built page.
//
//     bun llms.ts [build/client]
//
// Nothing here is written twice: the page's words come from app/content.ts and
// the language from the Rust that serves read_docs, so neither can drift from
// the page or the tool.

import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  AGENT_CARDS,
  CASE,
  DESCRIPTION,
  EXAMPLES,
  FAQS,
  GET_IT,
  GH,
  LICENCE,
  PLANS,
  REFUSAL,
  SHOWCASE,
  SITE,
  TITLE,
  WEB,
} from "./app/content";

const out = resolve(process.argv[2] ?? "build/client");
const RAW = "https://raw.githubusercontent.com/ierehon1905/parcad/main";
const REPO = fileURLToPath(new URL("..", import.meta.url));

const topics = readDocsTopics(join(out, "docs"));

const page = [
  `# ${TITLE}`,
  `> ${DESCRIPTION}`,
  `Experimental, made by Maging Studio. Units are millimetres. ${LICENCE} or later; the parts you make are yours. Source: ${GH}`,
  "## Get it",
  GET_IT.map((o) => `- ${o.os}: ${o.copy ? `\`${o.copy}\`` : `[${o.label}](${o.href})`}`).join("\n"),
  "## The case for it",
  ...CASE.flatMap((c) => [`### ${c.title}`, c.body, ...(c.media.kind === "refusal" ? ["```text\n" + REFUSAL + "\n```"] : [])]),
  "## In code",
  ...SHOWCASE.flatMap((s) => [
    `### ${s.label}`,
    `\`${s.file}\`:`,
    "```" + s.lang + "\n" + s.code + "\n```",
    s.readout.map(([k, v]) => `- ${k}: ${v}`).join("\n"),
    s.caption + ".",
  ]),
  "## For agents",
  ...AGENT_CARDS.flatMap((c) => [`### ${c.title}`, c.body, ...(c.code ? ["```bash\n" + c.code + "\n```"] : []), `[${c.cta}](${c.href})`]),
  "## Ways to run it",
  ...PLANS.flatMap((p) => [
    `### ${p.name} — ${p.price}`,
    `${p.blurb} ${p.note}.`,
    p.features.map((f) => `- ${f}`).join("\n"),
    p.cta.copy ? "```bash\n" + p.cta.copy + "\n```" : `[${p.cta.label}](${p.cta.href})`,
  ]),
  "## Examples",
  EXAMPLES.map((e) => `- [${e.name}](${RAW}/examples/${e.file}.js): ${e.why}. ${e.stat}`).join("\n"),
  "## FAQ",
  ...FAQS.flatMap((f) => [`### ${f.q}`, ...f.a]),
].join("\n\n");

const llms = [
  "# ParCAD",
  `> ${DESCRIPTION}`,
  [
    "A part is a JavaScript script in ParCAD's DSL, built by an exact OpenCASCADE B-rep kernel. Units are millimetres; primitives are centred on the origin and placed with `.at(x, y, z)`; a script returns a shape, or `{ base, lid }` for several bodies.",
    "Edges are selected by description, never by index, and `.expect({ count })` stops the build when a selector drifts. Every reply reports measured values: volume, size, wall thickness, clearance between bodies.",
    "An agent drives it over MCP: the desktop app and `parcad serve` serve it at http://127.0.0.1:4242/mcp, `parcad mcp` is the stdio relay, and ParCAD web hands out a link with nothing installed. Start on the `read_docs` tool, which serves the documents below.",
  ].join("\n\n"),
  "## Docs",
  topics.map((t) => `- [${t.name}](${SITE}docs/${t.name}.md): ${t.answers}`).join("\n"),
  "## Use it",
  [
    `- [ParCAD web](${WEB}): the editor and kernel in a browser tab; Connect your AI gives an MCP link`,
    `- [Install and connect an agent](${GH}#use-it-from-an-agent): Homebrew, winget, the desktop app, the Claude Code and Codex plugin, Cursor and VS Code`,
    `- [Getting started](${RAW}/docs/GETTING_STARTED.md): five minutes, from the browser to your own agent`,
    `- [What an agent sees](${RAW}/docs/PERCEPTION.md): views, sections, regions and probes, and what to ask for instead of a picture`,
  ].join("\n"),
  "## Examples",
  EXAMPLES.map((e) => `- [${e.name}](${RAW}/examples/${e.file}.js): ${e.why}`).join("\n"),
  "## Optional",
  [
    `- [The landing page](${SITE}index.html.md): this site as Markdown`,
    `- [Everything](${SITE}llms-full.txt): the page, every document above and the example parts, in one file`,
    `- [Architecture](${RAW}/docs/ARCHITECTURE.md): how the pieces fit, and why`,
    `- [Writing for models](${RAW}/docs/WRITING_FOR_MODELS.md): how the tool replies are written and measured on small models`,
    `- [README](${RAW}/README.md)`,
  ].join("\n"),
].join("\n\n");

const examples = [
  "# Example parts",
  "Each is a whole part, as it ships in examples/.",
  ...EXAMPLES.map((e) => `## ${e.name}\n\n\`examples/${e.file}.js\`\n\n\`\`\`js\n${readFileSync(join(REPO, "examples", `${e.file}.js`), "utf8").trimEnd()}\n\`\`\``),
].join("\n\n");

const full = [page, ...topics.map((t) => t.text), examples].join("\n\n---\n\n");

const sitemap = `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>${SITE}</loc></url>
  <url><loc>${WEB}</loc></url>
</urlset>
`;

writeFileSync(join(out, "llms.txt"), llms + "\n");
writeFileSync(join(out, "index.html.md"), page + "\n");
writeFileSync(join(out, "llms-full.txt"), full + "\n");
writeFileSync(join(out, "sitemap.xml"), sitemap);
console.log(`llms.txt, index.html.md, llms-full.txt (${Math.round(full.length / 1024)} KB), sitemap.xml and ${topics.length} topics in ${out}`);

/** Every read_docs topic, written to `dir` by the Rust that serves the tool. */
function readDocsTopics(dir: string) {
  mkdirSync(dir, { recursive: true });
  let listed: string;
  try {
    listed = execFileSync("cargo", ["run", "--locked", "-q", "-p", "parcad-host", "--example", "docs_pages", "--", dir], {
      cwd: REPO,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "inherit"],
    });
  } catch {
    throw new Error("could not generate the read_docs pages: llms.ts needs the Rust toolchain from rust-toolchain.toml; run `cargo run -p parcad-host --example docs_pages -- /tmp/docs` to see why");
  }
  return listed
    .trim()
    .split("\n")
    .map((line: string) => {
      const [name, source, answers] = line.split("\t");
      return { name, source, answers, text: readFileSync(join(dir, `${name}.md`), "utf8").trimEnd() };
    });
}
