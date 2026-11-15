/**
 * The open part as a tree: its bodies, and under each the features its script
 * tagged. The sidebar's first tab, because a person spends nearly all their
 * time inside one part and the list of every other one is for the moment of
 * leaving it.
 *
 * Read from what the last build produced: the bodies the kernel measured, the
 * tags the graph carries. Pointing at a row singles its body out in the view;
 * pressing it puts the caret where the script writes it; the eye hides it. A
 * dot says the body has a problem, so the Problems tab is opened when there is
 * a reason to.
 */

import { useSignal } from "@preact/signals";
import type { ComponentChildren } from "preact";

import type * as dsl from "../dsl";
import * as engine from "../engine";
import { fmt } from "../engine";
import { bodyNameAt } from "../hidden-marks";
import * as S from "../state";
import { Icon } from "./icons";
import { problemsOf, type Problem } from "./problems";
import { tip } from "./tooltip";

type Node = Record<string, unknown>;

/** Every node a node is built from: the arms of `children_of` in graph.rs. */
function childrenOf(node: Node): number[] {
  const out: number[] = [];
  for (const key of ["child", "base", "tool"]) if (typeof node[key] === "number") out.push(node[key] as number);
  for (const key of ["children", "tools"]) if (Array.isArray(node[key])) out.push(...(node[key] as number[]));
  if (Array.isArray(node.bodies)) for (const body of node.bodies as Node[]) out.push(body.child as number);
  return out;
}

/** The tags under a node, the ones built first first, each once. */
function tagsUnder(doc: dsl.Doc, from: number): string[] {
  const tags: string[] = [];
  const seen = new Set<number>();
  const walk = (id: number) => {
    if (seen.has(id)) return;
    seen.add(id);
    const node = doc.nodes[id];
    if (!node) return;
    childrenOf(node).forEach(walk);
    if (typeof node.tag === "string" && !tags.includes(node.tag)) tags.push(node.tag);
  };
  walk(from);
  return tags;
}

/** Each named body's root node, or the part's own root for a part that is one solid. */
function rootsOf(doc: dsl.Doc): Map<string, number> {
  const root = doc.nodes[doc.root];
  const roots = new Map<string, number>();
  if (root?.op === "bodies" && Array.isArray(root.bodies)) {
    for (const body of root.bodies as Node[]) roots.set(body.name as string, body.child as number);
  }
  return roots;
}

const escaped = (name: string) => name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
/** Where the script names a tag: its `.tag("…")` call. */
const tagAt = (tag: string) => (source: string) =>
  new RegExp(`\\.tag\\(\\s*["'\`]${escaped(tag)}["'\`]\\s*\\)`).exec(source)?.index;

export function PartTree() {
  const snapshot = S.snapshot.value;
  const doc = S.lastGraph.value;
  const path = S.openPath.value;

  if (!path) return <p class="m-0 px-4 py-4 text-ink-dim">No part is open.</p>;

  const problems = snapshot ? problemsOf(snapshot) : [];
  const roots = doc ? rootsOf(doc) : new Map<string, number>();
  const named = snapshot?.named_bodies;

  return (
    <div class="flex-1 min-h-0 overflow-auto pb-3">
      <header class="px-4 pt-3 pb-2.5">
        <h2 class="m-0 text-[15px] font-semibold leading-tight break-words">{engine.titleFor(path)}</h2>
        {snapshot && (
          <p class="m-0 mt-1 text-small tabular-nums text-ink-dim">
            {snapshot.size.map((d) => fmt(d)).join(" × ")} mm
          </p>
        )}
      </header>

      {!snapshot ? (
        <p class="m-0 px-4 text-ink-dim">Building…</p>
      ) : named ? (
        <>
          <Heading count={named.length}>Bodies</Heading>
          {named.map((body) => {
            const root = roots.get(body.name);
            const tags = doc && root !== undefined ? tagsUnder(doc, root).filter((tag) => tag !== body.name) : [];
            return (
              <BodyRow
                key={body.name}
                name={body.name}
                reference={body.reference === true}
                pieces={body.pieces}
                problems={problems.filter((p) => p.bodies.includes(body.name))}
                tags={tags}
              />
            );
          })}
        </>
      ) : (
        <Features tags={doc ? tagsUnder(doc, doc.root) : snapshot.tags} />
      )}
    </div>
  );
}

function Heading({ children, count }: { children: ComponentChildren; count?: number }) {
  return (
    <h3 class="flex items-center gap-2 m-0 px-4 pt-2 pb-1 text-tiny font-medium uppercase tracking-[0.06em] text-ink-dim">
      {children}
      {count !== undefined && <span class="font-mono normal-case tracking-normal opacity-70">{count}</span>}
    </h3>
  );
}

/** The features of a part that is one solid, straight under its name. */
function Features({ tags }: { tags: string[] }) {
  if (!tags.length) {
    return <p class="m-0 px-4 py-1 text-ink-dim">No tagged features. A <code class="font-mono text-small">.tag("name")</code> lists one here.</p>;
  }
  return (
    <>
      <Heading count={tags.length}>Features</Heading>
      {tags.map((tag) => (
        <TagRow key={tag} tag={tag} depth={0} />
      ))}
    </>
  );
}

const ROW = "group flex items-center gap-1.5 h-7 pr-2 cursor-pointer hover:bg-panel-2";

function BodyRow({
  name,
  reference,
  pieces,
  problems,
  tags,
}: {
  name: string;
  reference: boolean;
  pieces: number;
  problems: Problem[];
  tags: string[];
}) {
  const open = useSignal(false);
  const hidden = S.hiddenBodies.value.has(name);
  const worst = problems.some((p) => p.tone === "bad") ? "bad" : problems.length ? "note" : undefined;
  const focused = S.focusedBodies.value.includes(name);

  return (
    <>
      <div
        class={`${ROW} pl-2 ${focused ? "bg-panel-2" : ""}`}
        onMouseEnter={() => engine.focusBodies([name])}
        onMouseLeave={() => engine.focusBodies([])}
        onClick={() => engine.revealInCode((source) => bodyNameAt(source, name))}
      >
        {tags.length ? (
          <button
            type="button"
            aria-expanded={open.value}
            aria-label={open.value ? `Fold ${name}` : `Unfold ${name}`}
            class="flex items-center justify-center size-5 shrink-0 rounded-sm text-ink-dim hover:text-ink cursor-pointer"
            onClick={(e) => {
              e.stopPropagation();
              open.value = !open.value;
            }}
          >
            <Icon name="chevron" class={`size-3.5 transition-transform ${open.value ? "" : "-rotate-90"}`} />
          </button>
        ) : (
          <span class="size-5 shrink-0" />
        )}
        <span class={`truncate ${hidden ? "text-ink-dim/50" : "text-ink"}`}>{name}</span>
        {reference && <span class="shrink-0 text-tiny text-reference">reference</span>}
        {pieces > 1 && <span class="shrink-0 text-small tabular-nums text-ink-dim">{pieces} pieces</span>}
        <span class="flex-1" />
        {worst && (
          <span
            class={`shrink-0 size-2 rounded-full ${worst === "bad" ? "bg-bad" : "bg-note"}`}
            role="img"
            aria-label={`${problems.length} ${problems.length === 1 ? "problem" : "problems"}`}
            {...tip({ title: `${problems.length} ${problems.length === 1 ? "problem" : "problems"}`, text: "Listed under Problems, beside Code." })}
          />
        )}
        <button
          type="button"
          aria-pressed={hidden}
          aria-label={hidden ? `Show ${name}` : `Hide ${name}`}
          class={[
            "flex items-center justify-center size-6 shrink-0 rounded-sm cursor-pointer hover:text-ink",
            // Always there, so a glance at the tree says bodies can be hidden; quiet until pointed at.
            hidden ? "text-ink-dim" : "text-ink-dim/50 group-hover:text-ink-dim",
          ].join(" ")}
          onClick={(e) => {
            e.stopPropagation();
            engine.setBodyHidden(name, !hidden);
          }}
        >
          <Icon name={hidden ? "hidden" : "eye"} class="size-4" />
        </button>
      </div>
      {open.value && tags.map((tag) => <TagRow key={tag} tag={tag} depth={1} body={name} />)}
    </>
  );
}

function TagRow({ tag, depth, body }: { tag: string; depth: number; body?: string }) {
  return (
    <div
      class={ROW}
      style={{ paddingLeft: `${16 + depth * 22}px` }}
      onMouseEnter={() => engine.focusBodies(body ? [body] : [])}
      onMouseLeave={() => engine.focusBodies([])}
      onClick={() => engine.revealInCode(tagAt(tag))}
    >
      <Icon name="tag" class="size-4 shrink-0 text-ink-dim" />
      <span class="truncate text-ink-dim group-hover:text-ink">{tag}</span>
    </div>
  );
}
