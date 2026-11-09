/**
 * The entry point, and nothing else.
 *
 * Everything that used to be in `main.ts` moved: the components into `ui/`, the
 * evaluation cycle and the live session into `engine.ts`, the state they share
 * into `state.ts`. What is left here is the one line that is genuinely about
 * starting the program.
 */

import { render } from "preact";

// Bundled rather than fetched: the desktop app has to look the same offline.
import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "./style.css";
import { App } from "./app";

render(<App />, document.getElementById("app")!);
