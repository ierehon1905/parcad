/**
 * A TypeScript language service that knows what a part is.
 *
 * Everything the editor offers about the language — a signature, a type, a
 * completion, an error — comes from here, and all of it is derived from
 * `dsl.ts` itself: the service compiles the DSL's own source, so a hover shows
 * the signature the kernel will actually be called with and the doc comment
 * `read_docs` serves. There is no second copy of the language to keep in step.
 *
 * Pure, and free of anything a browser supplies: the sources arrive as text and
 * the answers leave as plain data. `worker.ts` is the only thing that knows
 * this runs off the main thread, and `analyzer.test.ts` runs it in bun.
 */

import ts from "typescript";

import {
  GLOBALS_FILE,
  PART_FILE,
  compilerText,
  globalsFor,
  toCompiler,
  toDocument,
} from "./part-file";

export interface AnalyzerSources {
  /** `lib.*.d.ts`, keyed by bare file name. */
  libs: Record<string, string>;
  /** `dsl.ts` and what it imports, keyed by the path the importer writes. */
  modules: Record<string, string>;
  /** Every name a part is handed, which is `Object.keys(dsl)` at run time. */
  names: readonly string[];
}

/** A run of text with the kind the compiler gave it, for the editor to colour. */
export interface Span {
  text: string;
  kind: string;
}

export interface Info {
  /** The declaration, exactly as TypeScript writes it. */
  signature: Span[];
  /** The doc comment's prose, without its tags. */
  documentation: Span[];
  /** `@example`, `@param`, `@returns` — never `@remarks`; see `tagsOf`. */
  tags: { name: string; text: Span[] }[];
}

export interface CompletionItem {
  label: string;
  /** TypeScript's own kind: "method", "const", "keyword", … */
  kind: string;
  /** Where TypeScript would sort it, which the editor turns into a boost. */
  sortText: string;
  /** Set when accepting the completion is not just inserting the label. */
  insert?: string;
}

export interface SignatureInfo {
  signatures: { prefix: Span[]; params: { label: Span[]; doc: Span[] }[]; suffix: Span[] }[];
  selected: number;
  argument: number;
}

export interface Problem {
  from: number;
  to: number;
  severity: "error" | "warning";
  message: string;
}

const COMPILER_OPTIONS: ts.CompilerOptions = {
  target: ts.ScriptTarget.ES2022,
  module: ts.ModuleKind.ESNext,
  moduleResolution: ts.ModuleResolutionKind.Bundler,
  lib: ["lib.es2022.d.ts"],
  // A part is JavaScript, and has to be read as JavaScript: `.ts` would accept
  // type annotations the runtime rejects, and the point of the diagnostics is
  // that what the editor accepts is what `new Function` accepts.
  allowJs: true,
  checkJs: true,
  // Hand-written JavaScript, not this repository's code. An unannotated
  // parameter is ordinary here, and flagging one would be noise on every part.
  strict: false,
  noImplicitAny: false,
  // Except this one, which `dsl.ts` is written under: without it a `??` whose
  // left side cannot be null keeps the right side in the type, and the DSL's
  // own source stops compiling. `agrees_with_its_own_language` holds that.
  strictNullChecks: true,
  skipLibCheck: true,
  noEmit: true,
  allowNonTsExtensions: true,
};

export function createAnalyzer(sources: AnalyzerSources) {
  const files = new Map<string, { text: string; version: number }>();
  const put = (name: string, text: string) => files.set(name, { text, version: 0 });

  for (const [name, text] of Object.entries(sources.libs)) put(`/${name}`, text);
  for (const [name, text] of Object.entries(sources.modules)) put(name, text);
  put(GLOBALS_FILE, globalsFor(sources.names));
  put(PART_FILE, compilerText(""));

  /** The document, which is the only file that ever changes. */
  let document = "";

  const host: ts.LanguageServiceHost = {
    getScriptFileNames: () => [PART_FILE, GLOBALS_FILE, ...Object.keys(sources.modules)],
    getScriptVersion: (name) => String(files.get(name)?.version ?? 0),
    getScriptSnapshot: (name) => {
      const file = files.get(name);
      return file && ts.ScriptSnapshot.fromString(file.text);
    },
    getCurrentDirectory: () => "/",
    getCompilationSettings: () => COMPILER_OPTIONS,
    getDefaultLibFileName: () => "/lib.es2022.d.ts",
    fileExists: (name) => files.has(name),
    readFile: (name) => files.get(name)?.text,
    // Every import in this program is relative and every file is in the one
    // map, so the whole of module resolution is "try the extensions".
    resolveModuleNameLiterals: (literals, containing) =>
      literals.map((literal) => {
        const base = literal.text.replace(/^\./, containing.replace(/\/[^/]*$/, ""));
        for (const extension of [".ts", ".d.ts", ".js"]) {
          const resolved = base.endsWith(extension) ? base : base + extension;
          if (files.has(resolved)) {
            return { resolvedModule: { resolvedFileName: resolved, extension } };
          }
        }
        return { resolvedModule: undefined };
      }),
  };

  const service = ts.createLanguageService(host, ts.createDocumentRegistry());

  /** The names a part must not rebind, for the diagnostic that says so. */
  const reserved = new Set(sources.names);

  return {
    /** Replace the document. Every query below reads whatever was set last. */
    setPart(source: string) {
      document = source;
      const file = files.get(PART_FILE)!;
      file.text = compilerText(source);
      file.version += 1;
    },

    /** The signature, type and doc comment at a position, for a hover. */
    quickInfo(pos: number): (Info & { from: number; to: number }) | undefined {
      const info = service.getQuickInfoAtPosition(PART_FILE, toCompiler(pos));
      if (!info) return undefined;
      return {
        from: toDocument(info.textSpan.start, document.length),
        to: toDocument(info.textSpan.start + info.textSpan.length, document.length),
        signature: spans(info.displayParts),
        documentation: spans(info.documentation),
        tags: tagsOf(info.tags),
      };
    },

    completions(pos: number): CompletionItem[] {
      const list = service.getCompletionsAtPosition(PART_FILE, toCompiler(pos), {
        includeCompletionsForModuleExports: false,
        includeCompletionsWithInsertText: true,
      });
      return (list?.entries ?? [])
        // The DSL's one piece of plumbing, which the editor wraps a treatment
        // call in and a part never writes. It is bound, so rebinding it is
        // still reported below; it is just not something to offer. `docs.rs`
        // leaves the same name out of the reference, by name and for the same
        // reason — a rule rather than a list, so the two cannot drift.
        .filter((entry) => !entry.name.startsWith("__"))
        .map((entry) => ({
        label: entry.name,
        kind: entry.kind,
        sortText: entry.sortText,
        insert: entry.insertText,
      }));
    },

    /** The card for one completion, fetched only when it is highlighted. */
    completionDetail(pos: number, label: string): Info | undefined {
      const details = service.getCompletionEntryDetails(
        PART_FILE,
        toCompiler(pos),
        label,
        undefined,
        undefined,
        undefined,
        undefined,
      );
      if (!details) return undefined;
      return {
        signature: spans(details.displayParts),
        documentation: spans(details.documentation),
        tags: tagsOf(details.tags),
      };
    },

    /** Which parameter the caret is in, and what the call expects. */
    signatureHelp(pos: number): SignatureInfo | undefined {
      const help = service.getSignatureHelpItems(PART_FILE, toCompiler(pos), undefined);
      if (!help || help.items.length === 0) return undefined;
      return {
        selected: help.selectedItemIndex,
        argument: help.argumentIndex,
        signatures: help.items.map((item) => ({
          prefix: spans(item.prefixDisplayParts),
          suffix: spans(item.suffixDisplayParts),
          params: item.parameters.map((parameter) => ({
            label: spans(parameter.displayParts),
            doc: spans(parameter.documentation),
          })),
        })),
      };
    },

    /**
     * Everything wrong with the DSL's own source, under the options above.
     *
     * Nothing shows this to a reader — it is the guard that the language the
     * editor compiles is the language the app builds. An error in `dsl.ts`
     * here costs a part the inference below it, and lands in the Problems
     * panel of whatever editor `editor_types.rs` wrote a `jsconfig.json` for.
     */
    languageProblems(): string[] {
      return Object.keys(sources.modules).flatMap((file) =>
        service
          .getSemanticDiagnostics(file)
          .map((d) => `${file}: ${ts.flattenDiagnosticMessageText(d.messageText, " ")}`),
      );
    },

    /** Everything wrong with the document, in document offsets. */
    diagnostics(): Problem[] {
      const raw = [
        ...service.getSyntacticDiagnostics(PART_FILE),
        ...service.getSemanticDiagnostics(PART_FILE),
      ];
      const problems = raw.filter(provable).map((diagnostic) => ({
        from: toDocument(diagnostic.start ?? 0, document.length),
        to: toDocument((diagnostic.start ?? 0) + (diagnostic.length ?? 0), document.length),
        severity:
          diagnostic.category === ts.DiagnosticCategory.Warning
            ? ("warning" as const)
            : ("error" as const),
        message: ts.flattenDiagnosticMessageText(diagnostic.messageText, "\n"),
      }));
      return [...problems, ...rebindings()];
    },
  };

  /**
   * Names the part declares that the DSL already holds.
   *
   * The DSL arrives as function parameters, so `const box = …` in a part is a
   * redeclaration and the script does not parse at all — the failure the
   * conventions warn about, reported where it was typed instead of as a build
   * error with no place. Only `let`, `const` and `class` conflict: `var` and
   * `function` are allowed to redeclare a parameter, and do.
   */
  function rebindings(): Problem[] {
    const file = service.getProgram()?.getSourceFile(PART_FILE);
    if (!file) return [];
    const found: Problem[] = [];
    const check = (name: ts.BindingName) => {
      if (ts.isIdentifier(name)) {
        if (!reserved.has(name.text)) return;
        found.push({
          from: toDocument(name.getStart(file), document.length),
          to: toDocument(name.getEnd(), document.length),
          severity: "error",
          message:
            `\`${name.text}\` is part of the DSL, which a part is handed as arguments, ` +
            `so this declaration would stop the script parsing. Rename it.`,
        });
        return;
      }
      if (ts.isObjectBindingPattern(name) || ts.isArrayBindingPattern(name)) {
        for (const element of name.elements) if (ts.isBindingElement(element)) check(element.name);
      }
    };
    const walk = (node: ts.Node) => {
      if (ts.isVariableStatement(node)) {
        const lexical = node.declarationList.flags & (ts.NodeFlags.Let | ts.NodeFlags.Const);
        if (lexical) for (const declaration of node.declarationList.declarations) check(declaration.name);
      } else if (ts.isClassDeclaration(node) && node.name) {
        check(node.name);
      }
      ts.forEachChild(node, walk);
    };
    ts.forEachChild(file, walk);
    return found;
  }
}

export type Analyzer = ReturnType<typeof createAnalyzer>;

/**
 * Codes that mean "I could not prove the length of this array", which in a part
 * is never provable.
 *
 * A part is JavaScript, so `points.map(([x, y]) => [x, y])` is a `number[][]`
 * and there is no annotation in the file that can make it a `[number, number][]`
 * — every list of points a part computes rather than writes out lands here. The
 * elements themselves are checked: a string among the numbers, or a written-out
 * `[0, 0, 0]` where two are wanted, is a different code and still reported. See
 * docs/GOTCHAS.md, "The editor does not check array lengths".
 */
const UNPROVABLE_LENGTH = new Set([2620, 2621]);

/**
 * Whether a diagnostic says something a part can act on.
 *
 * A message chain is one path of explanation, so what it means is what it
 * bottoms out in — and a branch per union member means every branch has to be
 * unprovable before the whole complaint is.
 */
function provable(diagnostic: ts.Diagnostic): boolean {
  const causes = (message: string | ts.DiagnosticMessageChain): number[] => {
    if (typeof message === "string") return [];
    if (!message.next?.length) return [message.code];
    return message.next.flatMap(causes);
  };
  const roots = causes(diagnostic.messageText);
  return roots.length === 0 || !roots.every((code) => UNPROVABLE_LENGTH.has(code));
}

function spans(parts: ts.SymbolDisplayPart[] | undefined): Span[] {
  return (parts ?? []).map((part) => ({ text: part.text, kind: part.kind }));
}

/**
 * The tags a reader of the editor wants, which are the ones `read_docs` serves
 * without `detail`.
 *
 * `@remarks` is why a rule exists, and docs/WRITING_FOR_MODELS.md keeps it out
 * of the short reply for the same reason it stays out of a tooltip: a hover is
 * read mid-keystroke, and history does not survive that.
 */
function tagsOf(tags: ts.JSDocTagInfo[] | undefined): Info["tags"] {
  return (tags ?? [])
    .filter((tag) => tag.name !== "remarks")
    .map((tag) => ({ name: tag.name, text: spans(tag.text) }));
}
