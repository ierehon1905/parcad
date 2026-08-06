/**
 * The parts the picker offers, and the rules for naming them.
 *
 * Read from parcad's project folder at run time, not globbed from `examples/`
 * at build time. That folder is shared with the user and with agents over MCP,
 * so a part somebody saves — by hand, or through `save_project` — appears in
 * the picker without a rebuild, and the parts that ship are ordinary files in
 * it rather than a separate read-only category.
 *
 * The old build-time glob could only ever show what was compiled in, which made
 * "example" a different kind of object from "a part you made". It is not.
 *
 * A project is a `.parcad` folder holding `part.js`; see `projects.rs` for the
 * layout and why. Nothing above this module needs to know that — everything
 * here addresses a project by its slash-separated path.
 */

// Types only: this module is pure so that its rules can be tested without a
// DOM or a host, the same way `selectors.ts` is. The two functions that talk to
// the backend are one line each and live at the call site.
import type {
  ProjectEntry,
  ProjectFolder,
  ProjectList,
  ProjectPart,
} from "./backend";

export type { ProjectEntry, ProjectFolder, ProjectPart };

/** Opened on first load when it exists — the part the docs walk through. */
const PREFERRED = "bracket";

export interface Projects {
  /** Folders and parts, folders first, as the host walked them. */
  tree: ProjectEntry[];
  /** Every part, flattened, in tree order. */
  parts: ProjectPart[];
  /** Where they live, for the UI to show a user who asks. */
  directory: string;
  /** Which one to open, or undefined when the folder is empty. */
  initial: string | undefined;
}

/** What the picker makes of the host's answer. */
export function describeProjects({ tree, directory }: ProjectList): Projects {
  const parts = flatten(tree);
  return {
    tree,
    parts,
    directory,
    initial: parts.find((part) => part.path === PREFERRED)?.path ?? parts[0]?.path,
  };
}

/** Every part under these entries, depth first, folders in order. */
export function flatten(entries: ProjectEntry[]): ProjectPart[] {
  return entries.flatMap((entry) =>
    entry.kind === "folder" ? flatten(entry.children) : [entry],
  );
}

export function partAt(entries: ProjectEntry[], path: string): ProjectPart | undefined {
  return flatten(entries).find((part) => part.path === path);
}

/**
 * The tree with only the parts that match, and only the folders that still
 * hold one.
 *
 * A folder whose *name* matches keeps all of its contents: typing "mounts"
 * should show you the folder, not empty it. Matching is on the title, the path
 * and the tags, because those are the three things a person might remember.
 */
export function search(entries: ProjectEntry[], query: string): ProjectEntry[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return entries;

  const kept: ProjectEntry[] = [];
  for (const entry of entries) {
    if (entry.kind === "part") {
      const hay = [entry.title, entry.path, ...entry.tags].join(" ").toLowerCase();
      if (hay.includes(needle)) kept.push(entry);
      continue;
    }
    if (entry.name.toLowerCase().includes(needle)) {
      kept.push(entry);
      continue;
    }
    const children = search(entry.children, query);
    if (children.length) kept.push({ ...entry, children });
  }
  return kept;
}

/** Folders, deepest paths included, for a "put it in…" menu. */
export function folderPaths(entries: ProjectEntry[]): string[] {
  return entries.flatMap((entry) =>
    entry.kind === "folder" ? [entry.path, ...folderPaths(entry.children)] : [],
  );
}

/**
 * Why this name cannot be used, or null.
 *
 * The host validates every path it is given and is the only gate that counts —
 * this copy exists so that typing an impossible name says so on the keystroke
 * rather than after a round trip, the same reasoning as the selector grammar
 * being parsed twice. Keep the two in step: `safe()` in `projects.rs` is the
 * one that has to be right.
 */
export function nameProblem(name: string): string | null {
  const trimmed = name.trim();
  if (!trimmed) return "a name is needed";
  if (trimmed.includes("/")) return "no slashes — pick the folder separately";
  if (trimmed.startsWith(".")) return "a name cannot start with a dot";
  if (/\.(js|parcad)$/.test(trimmed)) return "leave the extension off";
  // Backslash, colon and control characters, exactly as `safe()` refuses them.
  // Spaces and hyphens are fine: `pipe-tee` is a real part name.
  if (/[\\:]/.test(trimmed) || [...trimmed].some((c) => c < " ")) {
    return "that character cannot be in a name";
  }
  return null;
}

/** `Mounts` + `bracket` → `Mounts/bracket`; the root is the empty string. */
export const join = (folder: string, name: string) =>
  folder ? `${folder}/${name}` : name;

/** The folder a path sits in, or the empty string for the root. */
export const parentOf = (path: string) => path.split("/").slice(0, -1).join("/");

export const leafOf = (path: string) => path.split("/").slice(-1)[0] ?? path;

/** `bracket`, `bracket-2`, `bracket-3`… — the first that nothing else holds. */
export function freePath(taken: Iterable<string>, folder: string, name: string): string {
  const used = new Set(taken);
  let candidate = join(folder, name);
  for (let nth = 2; used.has(candidate); nth++) candidate = join(folder, `${name}-${nth}`);
  return candidate;
}

/** A label for the picker: the file stem is already the name. */
export const label = (name: string) => name.replace(/[-_]/g, " ");

/** "3 minutes ago", for a card. Seconds since the epoch, as the host reports. */
export function when(modified: number | null, now = Date.now()): string {
  if (modified === null) return "";
  const seconds = Math.max(0, now / 1000 - modified);
  if (seconds < 90) return "just now";
  const minutes = seconds / 60;
  if (minutes < 60) return `${Math.round(minutes)} min ago`;
  const hours = minutes / 60;
  if (hours < 24) return `${Math.round(hours)} h ago`;
  const days = Math.round(hours / 24);
  return days === 1 ? "yesterday" : `${days} days ago`;
}

/**
 * What a new part starts as.
 *
 * It builds. An empty editor would report "the script must return a shape",
 * which is true and a poor first thing to happen after clicking New — the same
 * reason `start()` does not evaluate before the first part has loaded.
 */
export const STARTER = [
  "// A new part. Everything is millimetres, Z is up.",
  "// Primitives are centred on the origin; place them with .at(x, y, z).",
  "// The script must return a shape.",
  "",
  "const plate = box(40, 30, 6).tag(\"plate\");",
  "",
  "// Selectors say what an edge is for, never which index it landed on.",
  "return plate.edges(\"|Z\").expect({ count: 4 }).fillet(3);",
  "",
].join("\n");
