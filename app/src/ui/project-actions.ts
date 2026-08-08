/**
 * The verbs: what happens between asking the user something and the host
 * having done it. No markup, so the picker's components stay about layout.
 *
 * Each takes the dialogs it needs rather than reaching for them, which is what
 * lets these be read — and tested — without a browser.
 */

import * as backend from "../backend";
import { freePath, join, leafOf, STARTER } from "../projects";
import * as S from "../state";
import type { Ask, Failed } from "./components/Dialog";

export async function newPart(
  folder: string,
  ask: Ask,
  failed: Failed,
  refresh: () => Promise<void>,
  choose: (path: string) => Promise<void>,
) {
  const projects = S.projects.peek();
  if (!projects) return;
  const taken = projects.parts.map((part) => part.path);
  const name = await ask({
    title: "New part",
    label: `Name, in ${folder || "the project folder"}`,
    value: leafOf(freePath(taken, folder, "part")),
    confirm: "Create",
    // Copying an existing part is how most parts actually start, and the seeded
    // ones are the parts the eval corpus measures — so "from" is a list of
    // things known to build, not a gallery of templates.
    from: projects.parts,
  });
  if (!name) return;

  const path = join(folder, name.value);
  const script = name.from ? await backend.readProject(name.from) : STARTER;
  try {
    await backend.createProject(path, script);
    await refresh();
    await choose(path);
  } catch (e) {
    await failed(e);
  }
}

export async function newFolder(
  folder: string,
  ask: Ask,
  failed: Failed,
  refresh: () => Promise<void>,
) {
  const name = await ask({
    title: "New folder",
    label: `Name, in ${folder || "the project folder"}`,
    value: "Parts",
    confirm: "Create",
  });
  if (!name) return;
  try {
    await backend.createFolder(join(folder, name.value));
    await refresh();
  } catch (e) {
    await failed(e);
  }
}
