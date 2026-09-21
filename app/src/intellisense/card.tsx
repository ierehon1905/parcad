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
          <code key={i} class="font-mono text-ink">
            {part}
          </code>
        ),
      )}
    </>
  );
}

export function InfoCard({ info }: { info: Info }) {
  const documentation = text(info.documentation).trim();
  const examples = info.tags.filter((tag) => tag.name === "example");
  const params = info.tags.filter((tag) => tag.name === "param");
  const returns = info.tags.filter((tag) => tag.name === "returns");

  return (
    <div class="font-mono text-small leading-normal">
      <Code spans={info.signature} />
      {paragraphs(documentation).map((paragraph, i) => (
        <p key={i} class="m-0 mt-1.5 font-sans text-ink-dim [overflow-wrap:anywhere]">
          <Prose text={paragraph} />
        </p>
      ))}
      {params.length > 0 && (
        <dl class="grid m-0 mt-1.5 gap-x-2.5 gap-y-px grid-cols-[max-content_1fr]">
          {params.map((param) => {
            const [name, ...rest] = text(param.text).split(/\s+/);
            return [
              <dt key={`${name}-t`} class="text-ink-dim">
                {name}
              </dt>,
              <dd key={`${name}-d`} class="m-0 font-sans [overflow-wrap:anywhere]">
                {rest.join(" ")}
              </dd>,
            ];
          })}
        </dl>
      )}
      {returns.map((tag, i) => (
        <p key={i} class="m-0 mt-1.5 font-sans text-ink-dim">
          returns {text(tag.text)}
        </p>
      ))}
      {examples.map((tag, i) => (
        <div key={i} class="mt-1.5 px-1.5 py-1 bg-well border border-line rounded-xs">
          <Example code={text(tag.text).replace(/^```\w*\n?|```$/g, "").trim()} />
        </div>
      ))}
    </div>
  );
}
