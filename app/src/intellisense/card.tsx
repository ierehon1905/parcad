/**
 * What TypeScript knows about a name, drawn.
 *
 * One card, three readers: the hover tooltip, the panel beside a highlighted
 * completion, and the treatment tooltip, which puts this above its measured
 * rows so that hovering `.fillet(2)` answers both "what does this take?" and
 * "what did it actually round?" in one place.
 *
 * The code in it is coloured by the editor's own highlighter rather than by a
 * palette of this file's own — `oneDarkHighlightStyle` is the extension the
 * editor already runs, so a type in a tooltip is the colour that type is two
 * lines below it. Same rule as the design tokens, one layer down.
 */

import { javascriptLanguage } from "@codemirror/lang-javascript";
import { oneDarkHighlightStyle } from "@codemirror/theme-one-dark";
import { highlightCode, tags } from "@lezer/highlight";
import type { Tag } from "@lezer/highlight";

import type { Info, Span } from "./analyzer";
import { wrapSignature } from "./signature";

/**
 * How wide a card may be, and the padding inside it.
 *
 * Wider than Monaco's 500, deliberately. That number is for an editor that
 * fills its window; here the editor is one pane of two and a card is welcome
 * over the viewport beside it, so the width that matters is the one that keeps
 * a signature on one line — breaking one is a last resort, not a way to stay
 * narrow. Prose is capped separately, further down: 96 columns of code is
 * legible and 96 columns of sentence is not.
 */
const CARD_WIDTH = 720;
const CARD_PADDING = 16;

/**
 * How many characters of the card's own font fit across it.
 *
 * Measured rather than assumed: the alternative is a constant that is wrong on
 * the first machine whose monospace font is not the one this was written on,
 * and a signature broken two characters early looks exactly like a bug.
 * Measured once — the font does not change under a running window.
 */
let columns: number | undefined;
function cardColumns(): number {
  if (columns !== undefined) return columns;
  const probe = document.createElement("span");
  probe.className = "font-mono text-small";
  probe.style.cssText = "position:absolute;visibility:hidden;white-space:pre;top:-1000px";
  probe.textContent = "0".repeat(100);
  document.body.append(probe);
  const advance = probe.getBoundingClientRect().width / 100;
  probe.remove();
  // A font that has not loaded measures zero, and a card is better unwrapped
  // than wrapped at column zero.
  columns = advance > 1 ? Math.floor((CARD_WIDTH - CARD_PADDING) / advance) : 60;
  return columns;
}

/** How TypeScript names a run of a signature, as the editor's highlighter names it. */
const KIND: Record<string, Tag> = {
  keyword: tags.keyword,
  operator: tags.operator,
  stringLiteral: tags.string,
  numericLiteral: tags.number,
  className: tags.typeName,
  interfaceName: tags.typeName,
  enumName: tags.typeName,
  typeParameterName: tags.typeName,
  aliasName: tags.typeName,
  moduleName: tags.namespace,
  functionName: tags.function(tags.variableName),
  methodName: tags.function(tags.propertyName),
  propertyName: tags.propertyName,
  parameterName: tags.variableName,
  localName: tags.variableName,
};

function classOf(kind: string): string | undefined {
  const tag = KIND[kind];
  return (tag && oneDarkHighlightStyle.style([tag])) || undefined;
}

/** A signature, or any other run of display parts, in the editor's colours. */
export function Code({ spans, class: extra }: { spans: Span[]; class?: string }) {
  return (
    <code class={`whitespace-pre-wrap [overflow-wrap:anywhere] ${extra ?? ""}`}>
      {spans.map((span, i) => (
        <span key={i} class={classOf(span.kind)}>
          {span.text}
        </span>
      ))}
    </code>
  );
}

/**
 * An `@example` line, highlighted as if it were in the editor.
 *
 * The DSL's examples are the one part of a doc comment that is code, and
 * docs/WRITING_FOR_MODELS.md requires each of them to build. Reading one as
 * prose is the way to miss that it is the answer.
 */
function Example({ code }: { code: string }) {
  const out: preact.ComponentChild[] = [];
  let key = 0;
  highlightCode(
    code,
    javascriptLanguage.parser.parse(code),
    oneDarkHighlightStyle,
    (text: string, style: string) =>
      out.push(
        <span key={key++} class={style || undefined}>
          {text}
        </span>,
      ),
    () => out.push(<br key={key++} />),
  );
  return <code class="block whitespace-pre-wrap [overflow-wrap:anywhere]">{out}</code>;
}

const text = (spans: Span[]) => spans.map((span) => span.text).join("");

/**
 * The rule between two rows of a card, drawn edge to edge.
 *
 * Monaco's own trick, and its numbers: the negative side margins cancel the
 * card's 8px padding so the line reaches the border, and the -4px bottom
 * against the next block's 8px top leaves 4px under the rule and 4px over it.
 * `hoverWidget.css`, `.monaco-hover hr`.
 */
export function Rule() {
  return <hr class="h-px box-border border-0 border-t border-ink/15 mt-1 -mb-1 -mx-2 min-w-full" />;
}

/**
 * A doc comment's prose, as paragraphs rather than as it was typed.
 *
 * A comment in `dsl.ts` is wrapped to 80 columns for the person reading the
 * source; a tooltip is a different width, and honouring those line breaks
 * leaves a sentence ending three words into a line. Blank lines are the
 * author's paragraphs and are kept; the rest is rewrapped by the browser.
 */
function paragraphs(prose: string): string[] {
  return prose
    .split(/\n\s*\n/)
    .map((paragraph) => paragraph.replace(/\s*\n\s*/g, " ").trim())
    .filter(Boolean);
}

/**
 * A paragraph of a doc comment, with its backticked spans set as code.
 *
 * Every rule in `dsl.ts` names the thing it is about that way — "`tapped`
 * drills for a thread a machinist will tap" — and a reader scanning a tooltip
 * for the option they are typing finds it by shape before they find it by
 * word.
 */
function Prose({ text: prose }: { text: string }) {
  return (
    <>
      {prose.split(/`([^`]+)`/).map((part, i) =>
        i % 2 === 0 ? (
          part
        ) : (
          <code key={i} class="font-mono text-ink bg-well/55 rounded-xs px-[3px]">
            {part}
          </code>
        ),
      )}
    </>
  );
}

export function InfoCard({ info }: { info: Info }) {
  const documentation = paragraphs(text(info.documentation).trim());
  const examples = info.tags.filter((tag) => tag.name === "example");
  const params = info.tags.filter((tag) => tag.name === "param");
  const returns = info.tags.filter((tag) => tag.name === "returns");
  const hasProse = documentation.length + params.length + returns.length + examples.length > 0;

  return (
    <>
      {/* The declaration, alone above the rule. A reader who knows the function
          is here for the argument order and nothing else, and the rule is what
          lets them stop reading there. */}
      <div class="font-mono text-small">
        <Code spans={wrapSignature(info.signature, cardColumns())} />
      </div>
      {hasProse && <Rule />}
      {/* The prose is capped at a measure rather than at the card's width: a
          paragraph past about seventy characters loses the reader on the way
          back to the next line, and a doc comment is prose however wide the
          signature above it is. */}
      {hasProse && (
        <div class="font-sans text-small mt-2 max-w-[68ch]">
          {documentation.map((paragraph, i) => (
            <p key={i} class={`m-0 [overflow-wrap:anywhere] ${i > 0 ? "mt-2" : ""}`}>
              <Prose text={paragraph} />
            </p>
          ))}
          {params.length > 0 && (
            <dl class="grid m-0 mt-2 gap-x-2.5 gap-y-px grid-cols-[max-content_1fr]">
              {params.map((param) => {
                const [name, ...rest] = text(param.text).split(/\s+/);
                return [
                  <dt key={`${name}-t`} class="font-mono text-ink">
                    {name}
                  </dt>,
                  <dd key={`${name}-d`} class="m-0 [overflow-wrap:anywhere]">
                    <Prose text={rest.join(" ")} />
                  </dd>,
                ];
              })}
            </dl>
          )}
          {returns.map((tag, i) => (
            <p key={i} class="m-0 mt-2">
              returns <Prose text={text(tag.text)} />
            </p>
          ))}
          {examples.map((tag, i) => (
            <div key={i} class="mt-2 px-1.5 py-1 bg-well rounded-xs">
              <Example code={text(tag.text).replace(/^```\w*\n?|```$/g, "").trim()} />
            </div>
          ))}
        </div>
      )}
    </>
  );
}
