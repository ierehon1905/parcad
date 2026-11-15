/**
 * Which of two views of one place is showing: the sidebar's part and all
 * parts, the editor pane's code and problems.
 *
 * Underlined rather than filled: a filled segment reads as a setting (the
 * section axis), a tab as a place. `aria-selected` says which one is chosen
 * for anything not looking at the colour.
 */

import type { ComponentChildren } from "preact";

export interface Tab<K extends string> {
  key: K;
  label: ComponentChildren;
  /** What a tooltip or a screen reader says about it, when the label alone does not. */
  title?: string;
}

export function Tabs<K extends string>({
  tabs,
  current,
  onChoose,
  children,
}: {
  tabs: Tab<K>[];
  current: K;
  onChoose: (key: K) => void;
  /** Anything after the tabs, pushed to the far end of the row. */
  children?: ComponentChildren;
}) {
  return (
    <div role="tablist" class="flex flex-none items-stretch gap-4 h-9 px-3 border-b border-line">
      {tabs.map((tab) => {
        const chosen = tab.key === current;
        return (
          <button
            key={tab.key}
            type="button"
            role="tab"
            aria-selected={chosen}
            title={tab.title}
            class={[
              "flex items-center gap-1.5 -mb-px border-b-2 cursor-pointer whitespace-nowrap",
              chosen ? "border-accent text-ink" : "border-transparent text-ink-dim hover:text-ink",
            ].join(" ")}
            onClick={() => onChoose(tab.key)}
          >
            {tab.label}
          </button>
        );
      })}
      <span class="flex-1" />
      {children}
    </div>
  );
}

const COUNT_TONES = {
  bad: "bg-bad/15 text-bad",
  note: "bg-note/15 text-note",
} as const;

/** A number on a tab that says, at a glance, how much is waiting behind it. */
export function TabCount({ n, tone }: { n: number; tone: keyof typeof COUNT_TONES }) {
  return <span class={`min-w-5 px-1.5 rounded-full text-center font-mono text-tiny ${COUNT_TONES[tone]}`}>{n}</span>;
}
