/**
 * The document, and who is authoring it.
 *
 * This row used to also carry the kernel, the mesh detail and the section
 * plane, which pushed the agent's status chip to the far edge of the window.
 * Those three moved onto the viewport (see `view-tools.tsx`) and what is left
 * is one coherent claim: **which part is open, whether it is written down, and
 * who else is editing it.**
 *
 * The agent chip is here rather than in a corner because that is the honest
 * place for it. A model on the MCP endpoint reaches the same `service.rs` this
 * window does and writes to the same project folder; the part you are looking
 * at can be replaced under you by a caller you cannot see. That is a fact about
 * the document, so it sits beside the document's name.
 *
 * Everything the chip says is measured — a request that arrived, a tool that
 * was called — which is why an idle client is reported with the age of its last
 * call rather than as a flat "connected". A client that was killed cannot say
 * goodbye, and claiming it is still there would be the confident wrong answer
 * this codebase refuses everywhere else.
 */

import { useSignal } from "@preact/signals";
import { useEffect, useRef } from "preact/hooks";

import type { McpStatus } from "../backend";
import * as engine from "../engine";
import * as S from "../state";
import { Glass } from "./components/Glass";
import { Icon, type IconName } from "./icons";
import { tip } from "./tooltip";

const BUTTON =
  "flex items-center gap-1.5 px-2 py-1 bg-panel-2 text-ink border border-line " +
  "rounded-md font-mono text-small cursor-pointer hover:border-accent";

export function Titlebar() {
  return (
    <header class="flex flex-none items-center gap-2.5 h-11 px-3 bg-panel border-b border-line">
      <span class="font-semibold tracking-[0.02em] pr-0.5">ParCAD</span>
      <span class="w-px h-[18px] bg-line" />
      <OpenPart />
      <Save />
      <Export />
      <span class="w-px h-[18px] bg-line" />
      <Agent />
      <span class="flex-1" />
      {/* Transient only: saving, exported, copied, and how long the last
          evaluation took. Everything durable about the part is measured, and
          lives in the report over the viewport. */}
      <Status />
    </header>
  );
}

/** The part on screen, and the way to every other one. */
function OpenPart() {
  const path = S.openPath.value;
  return (
    <button
      id="project"
      type="button"
      class="flex items-center gap-2 max-w-[260px] px-2.5 py-1 bg-panel-2 text-ink
             border border-line rounded-md cursor-pointer hover:border-accent"
      {...tip({
        title: "Browse parts",
        key: "⌘O",
        code: path,
        text: "One folder, shared by this window, the Finder and every agent on the MCP endpoint.",
      })}
      onClick={() => (S.browserOpen.value = true)}
    >
      <Icon name="folder" class="size-4 shrink-0 text-ink-dim" />
      <span class="overflow-hidden text-ellipsis whitespace-nowrap">
        {path ? engine.titleFor(path) : "no part"}
      </span>
      <Icon name="chevron" class="size-4 shrink-0 text-ink-dim" />
    </button>
  );
}

/** Unsaved is the state worth marking; disabled means there is nothing to write. */
function Save() {
  const changed = S.isDirty.value;
  return (
    <button
      type="button"
      disabled={!changed}
      class="flex items-center gap-1.5 px-2 py-1 bg-panel-2 border border-line rounded-md
             font-mono text-small cursor-pointer
             disabled:opacity-55 disabled:cursor-default
             enabled:text-accent enabled:border-accent-deep enabled:hover:border-accent"
      {...tip({
        title: "Save this part",
        key: "⌘S",
        text: "Writes part.js, and rewrites the README and thumbnail beside it from what was last measured.",
      })}
      onClick={() => void engine.saveOpenPart()}
    >
      <Icon name="save" class="size-4 shrink-0" />
      <span>{changed ? "save •" : "saved"}</span>
    </button>
  );
}

/**
 * The two files this app can produce, and the difference between them.
 *
 * They were reachable only as ⌘E and ⇧⌘E, which is to say reachable only by
 * someone who had read the source. The modifier key also had to carry the whole
 * distinction between a mesh and an exact solid — the one thing about exporting
 * here that genuinely needs a sentence, because "the viewport is showing a mesh
 * preview" and "the STEP file will be exact anyway" are both true at once and
 * look like a contradiction.
 */
const EXPORTS: { format: "stl" | "step"; icon: IconName; label: string; key: string; detail: string }[] =
  [
    {
      format: "stl",
      icon: "mesh",
      label: "STL mesh",
      key: "⌘E",
      detail:
        "Triangles, meshed by whichever kernel the viewport is showing. For printing, and for a quick look somewhere else.",
    },
    {
      format: "step",
      icon: "kernel",
      label: "STEP solid",
      key: "⇧⌘E",
      detail:
        "Exact surfaces from the B-rep kernel, whatever the viewport happens to be drawing — a mesh cannot be turned back into surfaces after the fact. For a machine shop, and for another CAD program.",
    },
  ];

function Export() {
  const open = useSignal(false);
  const host = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const down = (event: PointerEvent) => {
      const target = event.target as Node | null;
      if (target && !host.current?.contains(target)) open.value = false;
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") open.value = false;
    };
    document.addEventListener("pointerdown", down, true);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("pointerdown", down, true);
      document.removeEventListener("keydown", escape);
    };
  }, []);

  return (
    <div ref={host} class="relative">
      <button
        type="button"
        class={BUTTON}
        {...tip({
          title: "Export",
          key: "⌘E",
          text: "Written beside the part, in its own project folder, and revealed in Finder. In a browser it goes to your downloads instead — a page cannot choose where, or open a file manager.",
        })}
        onClick={() => (open.value = !open.value)}
      >
        <Icon name="export" class="size-4 shrink-0" />
        <span>export</span>
      </button>

      {open.value && (
        <Glass
          variant="menu"
          layout="absolute top-full left-0 z-40 mt-1.5 w-[min(380px,86vw)] p-1.5"
        >
          {EXPORTS.map((entry) => (
            <button
              key={entry.format}
              type="button"
              // Nothing has evaluated cleanly, so there is no graph to write
              // out. Say so by being unpressable rather than by failing after
              // the press.
              disabled={!S.lastGraph.value}
              class="flex w-full items-start gap-2.5 px-2 py-1.5 text-left rounded-lg cursor-pointer
                     border border-transparent text-ink-dim
                     hover:border-accent-edge hover:bg-accent-deep/35
                     disabled:opacity-45 disabled:cursor-default
                     disabled:hover:border-transparent disabled:hover:bg-transparent"
              onClick={() => {
                open.value = false;
                void engine.runExport(entry.format);
              }}
            >
              <Icon name={entry.icon} class="size-6 shrink-0 text-ink" />
              <span class="min-w-0 flex-1">
                <span class="flex items-baseline gap-2 text-ink text-small">
                  <span>{entry.label}</span>
                  <span class="ml-auto font-mono text-ink-dim text-[10.5px]">{entry.key}</span>
                </span>
                <span class="block text-tiny leading-[1.45] mt-0.5">{entry.detail}</span>
              </span>
            </button>
          ))}
        </Glass>
      )}
    </div>
  );
}

/** Below this, the last call is recent enough to call the agent active. */
const MCP_ACTIVE_SECS = 20;
/** The spark's colour is the whole of the state. */
const TONE = { live: "text-good", busy: "text-accent", idle: "text-ink-dim" } as const;

function Agent() {
  const mcp = S.mcp.value;
  // The host answers this window's every other call too, so a failure to reach
  // it is not an MCP fact and must not be shown as one.
  if (!mcp) return null;

  const chip = describe(mcp);
  return (
    <span
      class={`flex items-center gap-1.5 font-mono text-small cursor-default ${chip.tone}`}
      {...tip({ title: "Agent connection", text: chip.detail })}
    >
      <Icon name="agent" class="size-3.5 shrink-0" />
      <span>{chip.text}</span>
    </span>
  );
}

/** One status, as the three things the chip shows. */
function describe(mcp: McpStatus): { text: string; tone: string; detail: string } {
  const idle = mcp.idle_secs;
  const working = idle !== null && idle < MCP_ACTIVE_SECS;

  // The chip says connected or not; the session count stays in the tooltip,
  // where there is room to say what it means. A client that reconnects opens a
  // second session and the endpoint cannot tell that from a second client, so
  // "3 sessions" on the chip would read as three agents.
  let text = "MCP";
  let tone: string = TONE.idle;
  if (mcp.clients > 0) {
    text = working ? "MCP connected · working" : `MCP connected · idle ${age(idle!)}`;
    tone = working ? TONE.busy : TONE.live;
  } else if (idle !== null) {
    text = `MCP last call ${age(idle)} ago`;
  }

  const lines = [
    mcp.clients > 0
      ? `${mcp.clients} open session${mcp.clients === 1 ? "" : "s"} — a client that reconnects opens another`
      : "no agent connected",
    `endpoint ${mcp.url}`,
  ];
  if (mcp.client) lines.push(`client ${mcp.client}`);
  if (mcp.tool_calls === 0) {
    lines.push("no tool calls yet");
  } else {
    const calls = `${mcp.tool_calls} tool call${mcp.tool_calls === 1 ? "" : "s"}`;
    lines.push(mcp.last_tool ? `${calls}, last ${mcp.last_tool}` : calls);
  }
  lines.push("An agent writes to the same project folder this window reads.");

  return { text, tone, detail: lines.join("\n") };
}

function age(seconds: number): string {
  if (seconds < 60) return `${Math.round(seconds)}s`;
  if (seconds < 3600) return `${Math.round(seconds / 60)}m`;
  return `${Math.round(seconds / 3600)}h`;
}

/** The three things the status line can be, as colour. */
const STATUS_TONE = { "": "text-ink-dim", busy: "text-accent", failed: "text-bad" } as const;

function Status() {
  const { text, tone } = S.status.value;
  return <span id="status" class={`font-mono text-small ${STATUS_TONE[tone]}`}>{text}</span>;
}
