/**
 * The way from the playground to an agent that builds parts.
 *
 * A page with its kernel in the tab has no MCP endpoint, so everything a model
 * can do with parcad starts with installing it. Where there is a host this
 * renders nothing: the agent chip in the titlebar already says who is connected.
 */

import { useSignal } from "@preact/signals";

import { mcpServedHere } from "../backend";
import { Banner } from "./components/Banner";
import { tip } from "./tooltip";

const REPO = "https://github.com/ierehon1905/parcad";
const ADD_TO_CLAUDE = "claude mcp add parcad -- parcad mcp";
const DISMISSED = "parcad.install-bar.dismissed";

function wasDismissed(): boolean {
  try {
    return localStorage.getItem(DISMISSED) === "1";
  } catch {
    return false;
  }
}

export function InstallBar() {
  const hidden = useSignal(mcpServedHere || wasDismissed());
  const copied = useSignal(false);
  if (hidden.value) return null;

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(ADD_TO_CLAUDE);
      copied.value = true;
      window.setTimeout(() => (copied.value = false), 1500);
    } catch {
      // No clipboard permission: the command is on screen to select by hand.
    }
  };

  const dismiss = () => {
    hidden.value = true;
    try {
      localStorage.setItem(DISMISSED, "1");
    } catch {
      // Private mode: it comes back next visit, which is harmless.
    }
  };

  return (
    <Banner id="install-bar" onDismiss={dismiss}>
      <span>
        Install ParCAD and your agent can build parts for you:
      </span>
      <button
        type="button"
        class="font-mono px-1.5 py-0.5 bg-panel-2 border border-line rounded-xs cursor-pointer hover:border-accent"
        onClick={copy}
        {...tip({
          title: "Connect Claude Code",
          text: "Copies the command. Run it after installing ParCAD with Homebrew, winget or a release download.",
        })}
      >
        {copied.value ? "copied" : ADD_TO_CLAUDE}
      </button>
      <a class="text-accent hover:underline" href={`${REPO}#install-it`} target="_blank" rel="noopener">
        Install
      </a>
      <a class="text-accent hover:underline" href={`${REPO}#use-it-from-an-agent`} target="_blank" rel="noopener">
        Other agents
      </a>
      <span class="flex-1" />
      <a class="text-ink-dim hover:text-ink" href={REPO} target="_blank" rel="noopener">
        Source
      </a>
      <a class="text-ink-dim hover:text-ink" href="licenses/NOTICE.md" target="_blank" rel="noopener">
        Built on Open CASCADE Technology
      </a>
    </Banner>
  );
}
