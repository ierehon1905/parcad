/**
 * The parts the picker offers.
 *
 * Read from parcad's project folder at run time, not globbed from `examples/`
 * at build time. That folder is shared with the user and with agents over MCP,
 * so a part somebody saves — by hand, or through `save_project` — appears in
 * the picker without a rebuild, and the parts that ship are ordinary files in
 * it rather than a separate read-only category.
 *
 * The old build-time glob could only ever show what was compiled in, which made
 * "example" a different kind of object from "a part you made". It is not.
 */

import { listProjects, readProject } from "./backend";

/** Opened on first load when it exists — the part the docs walk through. */
const PREFERRED = "bracket";

export interface Projects {
  names: string[];
  /** Where they live, for the UI to show a user who asks. */
  directory: string;
  /** Which one to open, or undefined when the folder is empty. */
  initial: string | undefined;
}

export async function loadProjects(): Promise<Projects> {
  const { projects, directory } = await listProjects();
  return {
    names: projects,
    directory,
    initial: projects.includes(PREFERRED) ? PREFERRED : projects[0],
  };
}

export { readProject };

/** A label for the picker: the file stem is already the name. */
export const label = (name: string) => name.replace(/-/g, " ");
