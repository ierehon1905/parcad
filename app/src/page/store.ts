/**
 * The project folder, for a page with no host: parts kept in this browser.
 *
 * Imported only by `backend.ts`, which is still the one module that knows
 * where a call lands. It answers with the same shapes `projects.rs` does —
 * folders first, then parts, each sorted by lowercased name; titles from the
 * stem unless one was set; a part seeded once and deleted stays deleted — so
 * the picker cannot tell the difference, and must not need to.
 *
 * IndexedDB rather than localStorage: a thumbnail is a PNG of the viewport,
 * and two dozen of them are past what localStorage holds.
 */

import type { ProjectEntry, ProjectList, ProjectPart } from "../backend";

interface StoredPart {
  path: string;
  script: string;
  title?: string;
  tags: string[];
  /** Seconds since the epoch, as the host reports a file's. */
  modified: number;
  preview?: string;
}

const DB = "parcad-playground";
const PARTS = "parts";
const FOLDERS = "folders";
const META = "meta";

/** Where a visitor's parts are, in the picker's words. */
export const DIRECTORY =
  "this browser's storage — parts here stay on this device. The installed app keeps them as files and serves MCP.";

let opened: Promise<IDBDatabase> | undefined;

function database(): Promise<IDBDatabase> {
  opened ??= new Promise<IDBDatabase>((resolve, reject) => {
    const request = indexedDB.open(DB, 1);
    request.onupgradeneeded = () => {
      const db = request.result;
      db.createObjectStore(PARTS, { keyPath: "path" });
      db.createObjectStore(FOLDERS);
      db.createObjectStore(META);
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () =>
      reject(new Error(`this browser refused the playground its storage (${request.error?.message}); a private window may not allow it`));
  }).then(async (db) => {
    await seed(db);
    return db;
  });
  return opened;
}

function done<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

async function all<T>(store: string): Promise<T[]> {
  const db = await database();
  return done(db.transaction(store).objectStore(store).getAll() as IDBRequest<T[]>);
}

async function folderKeys(): Promise<string[]> {
  const db = await database();
  return (await done(db.transaction(FOLDERS).objectStore(FOLDERS).getAllKeys())) as string[];
}

async function write(store: string, apply: (s: IDBObjectStore) => void): Promise<void> {
  const db = await database();
  const tx = db.transaction(store, "readwrite");
  apply(tx.objectStore(store));
  await new Promise<void>((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });
}

async function get(path: string): Promise<StoredPart | undefined> {
  const db = await database();
  return done(db.transaction(PARTS).objectStore(PARTS).get(path) as IDBRequest<StoredPart | undefined>);
}

const now = () => Math.floor(Date.now() / 1000);

/**
 * The parts that ship, bundled at build time from `examples/` — the same
 * folder the host seeds from, walked the same way into the same names.
 */
const SEEDS = import.meta.glob("../../../examples/**/*.js", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

async function seed(db: IDBDatabase) {
  const tx = db.transaction([PARTS, META], "readwrite");
  const meta = tx.objectStore(META);
  const parts = tx.objectStore(PARTS);
  const seeded = new Set<string>(((await done(meta.get("seeded"))) as string[] | undefined) ?? []);
  for (const [file, script] of Object.entries(SEEDS).sort(([a], [b]) => a.localeCompare(b))) {
    const name = file.replace(/^.*\/examples\//, "").replace(/\.js$/, "");
    if (seeded.has(name)) continue;
    parts.put({ path: name, script, tags: [], modified: now() } satisfies StoredPart);
    seeded.add(name);
  }
  meta.put([...seeded], "seeded");
  await new Promise<void>((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });
}

const leaf = (path: string) => path.split("/").pop() ?? path;
const readable = (stem: string) => stem.replace(/[-_]/g, " ");
const byName = (a: { name: string }, b: { name: string }) =>
  a.name.toLowerCase() < b.name.toLowerCase() ? -1 : a.name.toLowerCase() > b.name.toLowerCase() ? 1 : 0;

function tree(parts: StoredPart[], folders: string[], prefix = ""): ProjectEntry[] {
  const inside = (path: string) => (prefix ? path.startsWith(`${prefix}/`) : true);
  const rest = (path: string) => (prefix ? path.slice(prefix.length + 1) : path);
  const childFolders = new Set<string>();
  for (const path of [...folders, ...parts.map((p) => p.path)]) {
    if (!inside(path)) continue;
    const segments = rest(path).split("/");
    if (segments.length > 1 || folders.includes(path)) childFolders.add(segments[0]);
  }
  const folderEntries: ProjectEntry[] = [...childFolders]
    .map((name) => {
      const path = prefix ? `${prefix}/${name}` : name;
      return { kind: "folder" as const, name, path, children: tree(parts, folders, path) };
    })
    .sort(byName);
  const partEntries: ProjectPart[] = parts
    .filter((part) => inside(part.path) && !rest(part.path).includes("/"))
    .map((part) => ({
      kind: "part" as const,
      name: leaf(part.path),
      path: part.path,
      title: part.title ?? readable(leaf(part.path)),
      bundle: true,
      thumbnail: part.preview !== undefined,
      tags: part.tags,
      modified: part.modified,
    }))
    .sort(byName);
  return [...folderEntries, ...partEntries];
}

/** The same refusals `safe()` in `projects.rs` makes, for the same reasons. */
function safe(path: string): string {
  const segments = path.split("/");
  for (const segment of segments) {
    if (!segment || segment.startsWith(".") || /\.(js|parcad)$/.test(segment) || /[\\:]/.test(segment) || [...segment].some((c) => c < " ")) {
      throw new Error(`${JSON.stringify(path)} is not a project name: each part of it must be a plain name, with no leading dot, extension, backslash or colon.`);
    }
  }
  return path;
}

export async function list(): Promise<ProjectList> {
  const [parts, folders] = await Promise.all([all<StoredPart>(PARTS), folderKeys()]);
  return {
    projects: parts.map((p) => p.path).sort(),
    tree: tree(parts, folders),
    directory: DIRECTORY,
    preferred: "twisted-planter",
  };
}

async function existing(path: string): Promise<StoredPart> {
  const part = await get(safe(path));
  if (!part) throw new Error(`no project called ${JSON.stringify(path)} in this browser.`);
  return part;
}

export async function read(path: string): Promise<{ script: string }> {
  return { script: (await existing(path)).script };
}

export async function save(path: string, script: string, preview?: string): Promise<{ path: string }> {
  const part = (await get(safe(path))) ?? { path, script, tags: [], modified: now() };
  await write(PARTS, (s) => s.put({ ...part, script, modified: now(), preview: preview ?? part.preview }));
  return { path };
}

export async function create(path: string, script: string): Promise<{ path: string }> {
  if (await get(safe(path))) throw new Error(`${JSON.stringify(path)} already exists.`);
  await write(PARTS, (s) => s.put({ path, script, tags: [], modified: now() } satisfies StoredPart));
  return { path };
}

export async function createFolder(path: string): Promise<{ path: string }> {
  const folders = await folderKeys();
  if (folders.includes(safe(path))) throw new Error(`${JSON.stringify(path)} already exists.`);
  await write(FOLDERS, (s) => s.put(true, path));
  return { path };
}

export async function rename(from: string, to: string): Promise<{ path: string }> {
  safe(from);
  safe(to);
  const parts = await all<StoredPart>(PARTS);
  const folders = await folderKeys();
  if (parts.some((p) => p.path === to) || folders.includes(to)) throw new Error(`${JSON.stringify(to)} already exists.`);
  const under = (path: string) => path === from || path.startsWith(`${from}/`);
  const moved = (path: string) => to + path.slice(from.length);
  const movingParts = parts.filter((p) => under(p.path));
  const movingFolders = folders.filter(under);
  if (!movingParts.length && !movingFolders.length) throw new Error(`nothing at ${JSON.stringify(from)} to rename.`);
  await write(PARTS, (s) => {
    for (const part of movingParts) {
      s.delete(part.path);
      s.put({ ...part, path: moved(part.path) });
    }
  });
  await write(FOLDERS, (s) => {
    for (const folder of movingFolders) {
      s.delete(folder);
      s.put(true, moved(folder));
    }
  });
  return { path: to };
}

export async function setTitle(path: string, title: string): Promise<void> {
  const part = await existing(path);
  await write(PARTS, (s) => s.put({ ...part, title: title.trim() || undefined }));
}

/**
 * Removed from the picker. The host moves a file to `.trash`; here there is no
 * folder to open, so the answer says where it went in words that are true.
 */
export async function remove(path: string): Promise<{ trashed: string }> {
  const part = await existing(path);
  await write(PARTS, (s) => s.delete(part.path));
  return { trashed: "removed from this browser's storage" };
}

export async function setPreview(path: string, preview: string): Promise<void> {
  const part = await existing(path);
  await write(PARTS, (s) => s.put({ ...part, preview }));
}

export async function preview(path: string): Promise<string | null> {
  return (await get(path))?.preview ?? null;
}
