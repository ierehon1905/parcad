/**
 * The left edge: the open part, or every part.
 *
 * The open part comes first because that is where a person spends the day; the
 * list of every part is for the moment of leaving it, so it is the second tab
 * and choosing a part from it comes back to the first.
 */

import * as engine from "../engine";
import * as S from "../state";
import { Tabs } from "./components/Tabs";
import { PartTree } from "./part-tree";
import { ProjectBrowser } from "./project-browser";
import { tip } from "./tooltip";

export function Sidebar() {
  if (!S.browserOpen.value) return null;
  const tab = S.sidebarTab.value;
  return (
    <aside id="sidebar" aria-label="Sidebar" class="flex flex-col flex-none w-[272px] min-h-0 bg-panel border-r border-line">
      <Tabs
        tabs={[
          { key: "part", label: "Part" },
          { key: "parts", label: "All parts" },
        ]}
        current={tab}
        onChoose={(key) => (key === "parts" ? engine.showParts() : engine.showPartTree())}
      >
        <button
          type="button"
          class="self-center text-small text-ink-dim hover:text-ink cursor-pointer"
          {...tip({ title: "Hide the sidebar", key: "⌘O", text: "The part's name in the titlebar brings it back." })}
          onClick={() => (S.browserOpen.value = false)}
        >
          Hide
        </button>
      </Tabs>
      {tab === "part" ? <PartTree /> : <ProjectBrowser />}
    </aside>
  );
}
