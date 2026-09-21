/**
 * The language service, off the main thread.
 *
 * Creating the program parses TypeScript's own libraries and the whole of
 * `dsl.ts` — a few hundred milliseconds, once. On the main thread that would be
 * a dropped second in the viewport's render loop, which is the one place in this
 * app a stall is visible. So it runs here, and every answer in `service.ts` is
 * a promise.
 *
 * The sources are compiled in rather than fetched: the DSL's own source, which
 * is what makes a hover show the real signature and the doc comment `read_docs`
 * serves, and the subset of TypeScript's libraries a part can reach. No DOM —
 * a part runs in `new Function`, and offering it `document` would be offering
 * it something the sandboxed agent path does not have.
 */

import * as dsl from "../dsl";
import dslSource from "../dsl.ts?raw";
import selectorsSource from "../selectors.ts?raw";
import { createAnalyzer } from "./analyzer";
import type { Reply, Request } from "./protocol";

const raw = {
  ...import.meta.glob("../../node_modules/typescript/lib/lib.es*.d.ts", {
    query: "?raw",
    import: "default",
    eager: true,
  }),
  ...import.meta.glob("../../node_modules/typescript/lib/lib.decorators*.d.ts", {
    query: "?raw",
    import: "default",
    eager: true,
  }),
} as Record<string, string>;

const libs: Record<string, string> = {};
for (const [path, text] of Object.entries(raw)) {
  const name = path.slice(path.lastIndexOf("/") + 1);
  // `.full` pulls in the DOM, which a part does not have.
  if (!name.includes(".full.")) libs[name] = text;
}

const analyzer = createAnalyzer({
  libs,
  modules: { "/dsl.ts": dslSource, "/selectors.ts": selectorsSource },
  // The same list `engine.ts` binds as parameters, read from the same module,
  // so a name cannot be offered here and missing there.
  names: Object.keys(dsl),
});

self.onmessage = (event: MessageEvent<Request>) => {
  const request = event.data;
  const reply = (value: Reply) => self.postMessage(value);
  try {
    switch (request.kind) {
      case "part":
        return analyzer.setPart(request.text);
      case "quickInfo":
        return reply({ id: request.id, value: analyzer.quickInfo(request.pos) });
      case "completions":
        return reply({ id: request.id, value: analyzer.completions(request.pos) });
      case "completionDetail":
        return reply({
          id: request.id,
          value: analyzer.completionDetail(request.pos, request.label),
        });
      case "signatureHelp":
        return reply({ id: request.id, value: analyzer.signatureHelp(request.pos) });
      case "diagnostics":
        return reply({ id: request.id, value: analyzer.diagnostics() });
    }
  } catch (e) {
    reply({ id: request.id, error: e instanceof Error ? e.message : String(e) });
  }
};
