# Knowing the language — what the editor is told, and what the agent is told

Everything else in `docs/` is about geometry: what the kernel can make
([OP_ROADMAP.md](OP_ROADMAP.md)), what the language makes hard
([DSL_GAPS.md](DSL_GAPS.md)), what a model can see of a finished part
([PERCEPTION.md](PERCEPTION.md)). This file is about the step before all three —
**writing the script at all**, for the person typing it and for the model
authoring one over MCP.

The argument is that they have one answer. `dsl.ts` is already the single
authoritative description of the language — full TypeScript types, JSDoc on the
parts that earned it, and `build.rs` bundles it into the sandbox rather than
committing a copy, *because a committed copy would quietly describe an older
DSL*. Every feature below should be derived from that same file. Anything
hand-kept is a fourth copy of the language's meaning, and we already have three.

---

## Where we stand, measured

### The editor

`ui/editor.tsx` builds CodeMirror from `basicSetup + javascript() + oneDark`,
plus four things this project wrote: `selectorLinter`, `treatmentHover`,
`treatmentHoverField`, `insertFlashField`.

| capability | state | note |
|---|---|---|
| Syntax highlighting | ✅ generic JS | Nothing distinguishes a DSL name from a local, or a selector string from any other string. |
| Bracket matching, folding, history, search | ✅ | `basicSetup`, free. |
| Autocomplete of DSL names | ❌ | `basicSetup` turns `autocompletion()` on, but the only sources registered are `lang-javascript`'s own: the JS keyword list, and `localCompletionSource` — *identifiers already in this document*. So `box` completes after you have typed `box` once, and **none of the 28 exports complete on an empty file**. |
| Hover documentation | ~ partial | `treatmentHover` only fires over a `fillet`/`chamfer` call that the last successful evaluation produced. Hovering `torus` — which carries a genuinely useful doc comment about quoting O-rings by cord diameter — shows nothing. |
| Signature help | ❌ | Nothing tells you `cone` is `(bottomRadius, topRadius, height)` while your cursor is inside its parentheses. The palette knows; it is at the top of the pane, not at the caret. |
| Diagnostics | ~ one | `selectorLinter` marks bad selector syntax on the keystroke that types it, which is the model for everything else. Every other failure is a string in the `#error` banner: no line, no gutter mark, no span. |
| Type checking of a part | ❌ | `bun run build` runs `tsc --noEmit` over `app/src`. A `part.js` is never typechecked by anything — it is `new Function`, and its first check is the kernel. |
| Go-to-definition, rename | ❌ | And barely wanted: a part is one file with no imports. |

### The agent

Fifteen MCP tools — `field/field.toml` lists them, and is the one place that
does. Two answer a question about *the language* rather than about a part, and
both settle a guess without touching geometry: `check_selector`, and `read_docs`
(§2 below).

What motivated `read_docs` was a measured hole in the alternative. Counting calls
across all 20 seed parts:

| export | appears in |
|---|---|
| `sweep` | no example |
| `counterbore` | no example |
| `clearance` | no example |
| `METRIC_FASTENERS` | no example |
| `revolve` | only `fusion360/untitled2-v1.js` |
| `loft` | only `fusion360/untriangle-v3.js` |

A model whose only route to the vocabulary is reading example parts cannot
discover any of those. Two sting in particular. `loft` and `sweep` are the ops
CLAUDE.md holds up as the trap — shipped, measured exact, and moving nothing
closer to building; an op no example uses is invisible on the agent surface,
which is one concrete reason why. And `clearance` / `counterbore` /
`METRIC_FASTENERS` are the fastener table CLAUDE.md says must *never* be a
literal in a part: the rule existed while the agent surface never mentioned the
table it points at.

---

## What to build, in order

### 1. Resolved counts in the editor, next to the selector that produced them

Not an LSP feature anywhere else, and the highest-value one here. After each
evaluation, draw the *resolved* edge count beside every selector — the CAD
version of an inlay hint:

```js
.edges(">Z and |X")      // 4 edges
```

This is "report measured values, not requested ones" applied to the editor, and
what `.expect({ count: n })` exists to protect. The data is already in hand:
`engine.ts` holds `resolvedTargets`, `lastTreatments` and `treatmentRange()`, and
`selector-lint.ts` already walks every literal selector in the document to find
its exact span. The missing piece is a decoration, not a measurement.

Two constraints, both already solved for `treatmentHover`: the hint must
disappear the moment the document stops matching `lastSource`, or it reports the
previous part's counts; and it must map through changes rather than pin to
offsets.

### 2. One generated language index — **the agent half is done**

`app/src-tauri/src/docs.rs` derives the reference from `dsl.ts` on every request:
every export and every method of `Shape`, `EdgeSelection` and `VertexSelection`,
with real signatures, parameter names and JSDoc, grouped and prefaced by the
file's own module comment. It is served as `read_docs`, whose `dsl` topic is that
index and whose other topics `include_str!` the prose documents, so moving or
deleting one breaks the build rather than the reader.

Two things were written into it rather than discovered later. MCP `resources` are
the protocol-correct home for a document like this, but tool support is universal
and resource support is not — [NEXT.md](NEXT.md) records a client that cannot
complete the handshake at all — so a tool is the reachable form. And adding the
tool is not the same as a model reading it:
`eval/field/say-the-symmetry-once.md` requires `reach: read_docs` so a model that
finds `mirror` some other way grades LUCKY, and two rounds are recorded in
docs/PERCEPTION.md §15.

**The editor half of the same index is still to build**, and the point of this
entry is that it should be the same artifact rather than a second one:
`tsc --emitDeclarationOnly` preserves JSDoc into a `.d.ts`, and the index is that
plus a small JSON walk. It feeds a CodeMirror completion source registered on
`javascriptLanguage.data`, so the vocabulary completes on an empty file — minus
`__parcadTreatmentSource`, which is injected for the instrumentation and is not
something anyone should be offered — a `hoverTooltip` over any DSL name, and
signature help while the caret is inside the parentheses, which CodeMirror has no
built-in for but is a tooltip over the same data. Method completion after a `.`
can be approximated honestly rather than typed: nearly every value in a part *is*
a `Shape`, and `.edges(…)` returns the one other thing, so offering the `Shape`
methods after any dot is right almost always and wrong in a way that costs a
keystroke.

### 3. `check_script` — the parse-and-build half of `evaluate_part`, without the kernel

`check_selector` exists because settling a selector without geometry is cheap and
stops a guess from becoming a retry. The same argument runs one level up: a model
that has misspelled an option key pays a full kernel evaluation to find out.

The split already exists — `script::build_graph` runs the sandbox and returns the
intent graph; the backend is what happens after. A tool that stops there answers
"does this parse, does it build, and what did it build" in milliseconds, and its
refusals are the DSL's own, already worded for whoever caused them. Cheap enough
to be worth doing even if it only saves latency. Whether a model *uses* it before
`evaluate_part` is, again, a field question.

### 4. Put the error on the line that caused it

An error that names the fix should also name the place; today a script that
throws produces a banner and an unmarked document. One measurement makes the
mapping tractable: **`instrumentTreatmentCalls` inserts only single-line text**
(`__parcadTreatmentSource({…}, () => ` and `)`), so it preserves line numbers
exactly and shifts columns only on lines carrying a treatment call — and it
already builds the list of inserts it made, which is a near-free offset map.

The `new Function` wrapper adds a constant line offset on top. That constant is
engine-specific and must be **measured** in a test rather than assumed; the
webview is WKWebView and the sandbox is QuickJS, and this repo has a standing
rule about which of those two habits produces a confident wrong answer. Kernel
refusals should mark their call too — "that fillet radius does not fit" belongs
on the `.fillet()` that asked for it, and `treatmentRange()` returns the span.

### 5. Highlight the language, not just JavaScript

Small, and it shows what no generic theme can: which identifiers are vocabulary
and which are the author's. A `ViewPlugin` over the syntax tree, marking any
`VariableName` in the generated index, plus a distinct token for selector strings
— `selector-lint.ts` already finds those spans. Colours are `@theme` entries in
`style.css` like everything else.

### 6. A real TypeScript service — only after 2 and 4, and only if measured

`typescript` plus `@typescript/vfs` in a web worker, fed the generated `.d.ts` and
the part wrapped as a function body, buys real type flow: completions that know
what `.edges()` returned, quick info, diagnostics on a wrong argument type before
the kernel runs. It costs a few megabytes and a worker, but the reason it is last
is the risk: **the wrapper must mirror `new Function(...names, source)` exactly**
— same names, same order, same implicit return — or the editor confidently
reports errors the runtime does not have and misses ones it does. That is this
project's characteristic failure mode, in a component whose whole job is to be
trusted. Worth it only on evidence of mistakes (2) and (4) do not catch.

### Not proposed: an actual LSP server

Nothing consumes it — the editor is CodeMirror inside this app, and an external
editor pointed at a `part.js` is not a use case anyone has asked for. What is
wanted is the *feature set*, and each of those is a CodeMirror extension over the
index in (2). If a second editor ever appears, the index is already the hard part
and an LSP shim over it is small.

---

## The order, and why

**1, 2, 3 first.** (1) is measured data the editor already holds and does not
show; (2) is one generated artifact closing the biggest hole on each side at
once, and its agent half is the item that made this file worth writing; (3) is
hours. **4 next**, because it is the standing rule ("errors name the fix")
applied to the one surface where it is not honoured. **5 whenever there is room.
6 only on evidence.**

Nothing here is finished when it works. (2) and (3) are agent-facing, so they are
finished when `eval/field/` says a model reads them — and per that suite's own
README, read `reach` before `sound`.
