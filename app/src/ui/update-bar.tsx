/**
 * The prompt that a newer ParCAD is out, with the one button that installs it.
 *
 * Installing replaces the app and relaunches it, which also restarts the MCP
 * host every connected agent is using — so it waits for the user to press it.
 */

import { useSignal } from "@preact/signals";

import * as backend from "../backend";
import * as engine from "../engine";
import * as S from "../state";
import { Banner } from "./components/Banner";
import { Button } from "./components/Button";

export function UpdateBar() {
  const installing = useSignal(false);
  const failure = useSignal<string | undefined>(undefined);
  const update = S.update.value;
  if (!update) return null;

  const install = async () => {
    installing.value = true;
    failure.value = undefined;
    try {
      await backend.installUpdate();
    } catch (e) {
      failure.value = String(e);
      installing.value = false;
    }
  };

  return (
    <Banner
      id="update-bar"
      onDismiss={installing.value ? undefined : engine.dismissUpdate}
      dismissLabel="Not now"
    >
      <span>
        ParCAD {update.version} is available — you have {update.current}.
      </span>
      <Button variant="primary" disabled={installing.value} onClick={install}>
        {installing.value ? "Downloading…" : "Update and restart"}
      </Button>
      {failure.value && <span class="text-bad whitespace-pre-wrap">{failure.value}</span>}
      <span class="flex-1" />
    </Banner>
  );
}
