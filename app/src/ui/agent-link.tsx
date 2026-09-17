/**
 * ParCAD web's way to an agent: a link to this tab, and how to give it to one.
 *
 * The desktop host has an endpoint a local client already knows how to find.
 * A tab has none until it opens a socket to the relay, so the chip here is a
 * button, the panel under it says what each client needs pasted where, and
 * nothing is opened until the visitor asks — the link lets whoever holds it
 * edit parts in this tab, and the panel says so where the link is.
 */

import { useSignal } from "@preact/signals";
import { useRef } from "preact/hooks";

import type { AgentLink as Link, McpStatus } from "../backend";
import * as engine from "../engine";
import * as S from "../state";
import { Button } from "./components/Button";
import { Glass } from "./components/Glass";
import { Icon } from "./icons";
import { tip } from "./tooltip";
import { useDismiss } from "./use-dismiss";

/** What the chip is doing, as colour. */
const TONE = { live: "text-good", busy: "text-accent", idle: "text-ink-dim", bad: "text-bad" } as const;
/** Below this, the last call is recent enough to call the agent working. */
const ACTIVE_SECS = 20;
const SERVER_NAME = "parcad-web";

export function AgentLink() {
  const open = S.agentPanel;
  const host = useRef<HTMLDivElement>(null);
  useDismiss(host, () => (open.value = false));

  const chip = chipFor(S.agentLink.value, S.mcp.value);
  return (
    <div ref={host} class="relative" id="agent-link">
      <button
        type="button"
        class={
          chip.call
            ? "flex items-center gap-1.5 px-2 py-1 bg-accent-deep text-ink border border-accent-edge rounded-md text-small cursor-pointer hover:border-accent"
            : `flex items-center gap-1.5 px-1 py-1 font-mono text-small cursor-pointer hover:text-ink ${chip.tone}`
        }
        {...(open.value ? {} : tip({ title: "Your AI, in this tab", text: chip.detail }))}
        onClick={() => (open.value = !open.value)}
      >
        <Icon name="agent" class="size-3.5 shrink-0" />
        <span>{chip.text}</span>
      </button>
      {open.value && (
        <Glass variant="menu" layout="absolute top-full left-0 z-40 mt-1.5 w-[min(420px,92vw)] p-3.5">
          <Panel />
        </Glass>
      )}
    </div>
  );
}

function chipFor(link: Link, mcp: McpStatus | undefined): { text: string; tone: string; detail: string; call?: boolean } {
  if (link.kind === "off") {
    return {
      text: "Connect your AI",
      tone: TONE.idle,
      call: true,
      detail: "Give Claude, Cursor or any MCP client a link, and it builds parts here with the app's own tools.",
    };
  }
  if (link.kind === "connecting") return { text: "opening a link…", tone: TONE.busy, detail: "Reaching the relay." };
  if (link.kind === "failed") return { text: "link failed", tone: TONE.bad, detail: link.error };
  if (link.kind === "elsewhere") {
    return { text: "AI in another tab", tone: TONE.idle, detail: "This link is open in a newer ParCAD tab." };
  }
  const idle = mcp?.idle_secs ?? null;
  const client = mcp?.client ? clientName(mcp.client) : "your AI";
  if (mcp && mcp.clients > 0) {
    const working = idle !== null && idle < ACTIVE_SECS;
    const last = mcp.last_tool ? `, last ${mcp.last_tool}` : "";
    return {
      text: working ? `${client} · working` : `${client} · connected`,
      tone: working ? TONE.busy : TONE.live,
      detail: `${mcp.client}\n${mcp.tool_calls} tool call${mcp.tool_calls === 1 ? "" : "s"}${last}`,
    };
  }
  return { text: "waiting for your AI", tone: TONE.idle, detail: "The link is open. Add it to your AI to start." };
}

/** "claude-code 2.1.268" reads as "Claude Code"; anything else as it announced itself. */
function clientName(announced: string): string {
  const name = announced.replace(/\s+[\d.]+\S*$/, "");
  const known: Record<string, string> = {
    "claude-code": "Claude Code",
    "claude-ai": "Claude",
    "cursor-vscode": "Cursor",
    "Visual Studio Code": "VS Code",
  };
  return known[name] ?? name;
}

type Client = "code" | "claude" | "cursor" | "other";

const CLIENTS: { id: Client; label: string }[] = [
  { id: "code", label: "Claude Code" },
  { id: "claude", label: "Claude app" },
  { id: "cursor", label: "Cursor · VS Code" },
  { id: "other", label: "Other" },
];

function Panel() {
  const link = S.agentLink.value;
  return (
    <div class="flex flex-col gap-3 text-small text-ink-dim">
      <div>
        <h3 class="m-0 mb-1 text-sm text-ink font-semibold">Let your AI build here</h3>
        <p class="m-0 leading-[1.5]">
          Your assistant gets the same tools as the ParCAD app and builds in this tab, where you watch every part
          appear. Nothing to install — keep the tab open.
        </p>
      </div>
      {!engine.RELAY ? (
        <p class="m-0 text-bad">This build of ParCAD web has no relay to open a link through.</p>
      ) : link.kind === "off" ? (
        <Offer />
      ) : link.kind === "connecting" ? (
        <p class="m-0 flex items-center gap-2">
          <Spark tone={TONE.busy} /> Opening a link…
        </p>
      ) : link.kind === "failed" ? (
        <div class="flex flex-col gap-2">
          <p class="m-0 text-bad whitespace-pre-wrap">{link.error}</p>
          <Button variant="primary" layout="self-start" onClick={() => void engine.openToAgents()}>
            Try again
          </Button>
        </div>
      ) : link.kind === "elsewhere" ? (
        <div class="flex flex-col gap-2">
          <p class="m-0">Your link is open in a newer ParCAD tab, so agents reach that one.</p>
          <Button variant="primary" layout="self-start" onClick={() => void engine.openToAgents()}>
            Use this tab instead
          </Button>
        </div>
      ) : (
        <Ready link={link.link} />
      )}
    </div>
  );
}

function Offer() {
  return (
    <div class="flex flex-col gap-2">
      <Button variant="primary" layout="self-start" onClick={() => void engine.openToAgents()}>
        Create my link
      </Button>
      <p class="m-0 text-tiny leading-[1.5]">
        The link goes through {new URL(engine.RELAY!).host}, which passes messages between your AI and this tab and
        keeps none of them.
      </p>
    </div>
  );
}

function Ready({ link }: { link: string }) {
  const client = useSignal<Client>("code");
  const mcp = S.mcp.value;
  const connected = !!mcp && mcp.clients > 0;
  const config = JSON.stringify({ url: link });

  return (
    <>
      <div class="flex gap-1 p-0.5 rounded-lg bg-well border border-line" role="tablist">
        {CLIENTS.map((c) => (
          <button
            key={c.id}
            type="button"
            role="tab"
            aria-selected={client.value === c.id}
            class={`flex-1 px-1.5 py-1 rounded-md cursor-pointer text-tiny whitespace-nowrap ${
              client.value === c.id ? "bg-panel-2 text-ink shadow-[0_1px_0_rgb(255_255_255/0.04)]" : "text-ink-dim hover:text-ink"
            }`}
            onClick={() => (client.value = c.id)}
          >
            {c.label}
          </button>
        ))}
      </div>

      {client.value === "code" && (
        <Step text="Run this in your terminal, then start Claude Code:">
          <Copyable text={`claude mcp add --transport http ${SERVER_NAME} ${link}`} />
        </Step>
      )}
      {client.value === "claude" && (
        <>
          <Step text={<>In Claude, open <b class="text-ink font-medium">Settings → Connectors → Add custom connector</b>, name it ParCAD, and paste:</>}>
            <Copyable text={link} />
          </Step>
          <a
            class="self-start text-accent hover:underline"
            href="https://claude.ai/settings/connectors"
            target="_blank"
            rel="noopener"
          >
            Open Claude's connector settings ↗
          </a>
        </>
      )}
      {client.value === "cursor" && (
        <Step text="Add it with one click, then allow it when the editor asks:">
          <div class="flex gap-2">
            <a
              class="px-3 py-1.5 bg-panel-2 text-ink border border-line rounded-md hover:border-accent"
              href={`https://cursor.com/install-mcp?name=${SERVER_NAME}&config=${encodeURIComponent(btoa(config))}`}
              target="_blank"
              rel="noopener"
            >
              Add to Cursor
            </a>
            <a
              class="px-3 py-1.5 bg-panel-2 text-ink border border-line rounded-md hover:border-accent"
              href={`https://vscode.dev/redirect/mcp/install?name=${SERVER_NAME}&config=${encodeURIComponent(
                JSON.stringify({ type: "http", url: link }),
              )}`}
              target="_blank"
              rel="noopener"
            >
              Add to VS Code
            </a>
          </div>
        </Step>
      )}
      {client.value === "other" && (
        <Step text="Any MCP client that connects to a URL (streamable HTTP, no sign-in):">
          <Copyable text={link} />
        </Step>
      )}

      <p class={`m-0 flex items-center gap-2 ${connected ? "text-ink" : ""}`}>
        <Spark tone={connected ? TONE.live : TONE.idle} pulse={!connected} />
        {connected
          ? `${clientName(mcp!.client ?? "your AI")} is connected${mcp!.tool_calls ? ` · ${mcp!.tool_calls} tool call${mcp!.tool_calls === 1 ? "" : "s"}` : ""}`
          : "Waiting for your AI to connect…"}
      </p>

      <div class="flex flex-col gap-2 pt-2.5 border-t border-line">
        <p class="m-0 text-tiny leading-[1.5]">
          Whoever has this link can edit parts in this tab while it is open. The relay passes messages through and
          keeps none.
        </p>
        <div class="flex gap-2">
          <Button
            variant="quiet"
            onClick={() => void engine.renewAgentLink()}
            {...tip({ title: "New link", text: "The old link stops working. Give your AI the new one." })}
          >
            New link
          </Button>
          <span class="flex-1" />
          <Button variant="quiet" onClick={() => void engine.closeToAgents()}>
            Close the link
          </Button>
        </div>
      </div>
    </>
  );
}

function Step({ text, children }: { text: preact.ComponentChildren; children: preact.ComponentChildren }) {
  return (
    <div class="flex flex-col gap-1.5">
      <p class="m-0 leading-[1.5]">{text}</p>
      {children}
    </div>
  );
}

function Copyable({ text }: { text: string }) {
  const copied = useSignal(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      copied.value = true;
      window.setTimeout(() => (copied.value = false), 1500);
    } catch {
      // No clipboard permission: the text is on screen to select by hand.
    }
  };
  return (
    <div class="flex items-stretch gap-1.5">
      <code
        class="flex-1 min-w-0 px-2 py-1.5 bg-well border border-line rounded-md font-mono text-tiny text-ink
               [overflow-wrap:anywhere] select-all leading-[1.45]"
      >
        {text}
      </code>
      <Button variant={copied.value ? "default" : "primary"} layout="shrink-0 self-start" onClick={copy}>
        {copied.value ? "Copied" : "Copy"}
      </Button>
    </div>
  );
}

function Spark({ tone, pulse }: { tone: string; pulse?: boolean }) {
  return (
    <span class={`relative inline-flex size-2 shrink-0 ${tone}`}>
      {pulse && <span class="absolute inset-0 rounded-full bg-current opacity-60 animate-ping" />}
      <span class="relative size-2 rounded-full bg-current" />
    </span>
  );
}
