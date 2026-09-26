import { cn } from "./frame";

const TOKEN =
  /(\/\/[^\n]*)|("(?:[^"\\\n]|\\.)*"|'(?:[^'\\\n]|\\.)*')|(\b\d+(?:\.\d+)?\b)|\b(const|let|return|true|false|null|new)\b|(\.?[A-Za-z_$][\w$]*)(?=\s*\()|([A-Za-z_$][\w$]*)|(\s+)|([^\w\s])/g;

const STYLE = ["text-ink-faint italic", "text-good", "text-gold", "text-tag", "text-accent", "text-ink", "", "text-ink-dim"];
const JSON_KEY = "text-accent";

/** A few dozen lines of highlighting for the snippets this page shows; not a parser. */
function tokens(src: string, lang: "js" | "json") {
  const out: { text: string; cls: string }[] = [];
  for (const m of src.matchAll(TOKEN)) {
    const group = m.slice(1).findIndex((g) => g !== undefined);
    let cls = STYLE[group] ?? "";
    if (lang === "json" && group === 1 && /^\s*:/.test(src.slice(m.index! + m[0].length))) cls = JSON_KEY;
    out.push({ text: m[0], cls });
  }
  return out;
}

export function Code({ source, lang, className }: { source: string; lang: "js" | "json"; className?: string }) {
  const lines = source.split("\n");
  return (
    <pre className={cn("overflow-x-auto font-mono text-[12px] leading-[1.7] md:text-[12.5px]", className)}>
      <code className="grid">
        {lines.map((line, i) => (
          <span key={i} className="grid grid-cols-[2.25rem_1fr]">
            <span className="pr-3 text-right text-ink-faint/60 select-none">{i + 1}</span>
            <span className="whitespace-pre">
              {tokens(line, lang).map((t, j) => (
                <span key={j} className={t.cls}>
                  {t.text}
                </span>
              ))}
              {line === "" && " "}
            </span>
          </span>
        ))}
      </code>
    </pre>
  );
}
