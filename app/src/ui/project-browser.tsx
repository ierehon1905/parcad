/**
 * The parts picker: a folder tree, a grid of parts, and the four things you do
 * to a project that are not editing it.
 *
 * It replaced a `<select>`, which could say a part's name and nothing else. The
 * project folder is shared with the user's filesystem and with agents over MCP,
 * so it is a place with structure in it — folders, parts an agent wrote a
 * minute ago, parts that are a loose `.js` rather than a `.parcad` folder — and
 * a dropdown could show none of that.
 *
 * Two rules this file follows:
 *
 * - **Every label is measured.** A card's timestamp comes from the file, its
 *   thumbnail from the last save, its title from the manifest. Nothing here
 *   displays a value it was asked for rather than one it read.
 * - **The host is the gate.** Names are checked here so a bad one is marked as
 *   it is typed, but `projects.rs` re-checks everything and its refusals are
 *   shown verbatim, because they name the fix.
 */

import { Fragment, type ComponentChildren } from "preact";
import { useSignal, type Signal } from "@preact/signals";
import { useEffect, useRef } from "preact/hooks";

import * as backend from "../backend";
import * as engine from "../engine";
import {
  flatten,
  folderPaths,
  freePath,
  join,
  leafOf,
  nameProblem,
  parentOf,
  search,
  when,
  STARTER,
  type ProjectEntry,
  type ProjectPart,
} from "../projects";
import * as S from "../state";
import { Icon } from "./icons";
import { tip } from "./tooltip";

/**
 * Class strings that more than one element wears.
 *
 * Shape, then colour — and never both from two sources. Two utilities that set
 * the same property do not resolve by the order they appear in the class
 * attribute; they resolve by their order in the generated stylesheet. So a
 * variant cannot be "the base, plus a different background": whichever of the
 * two Tailwind emits last wins, everywhere, silently. Each variant therefore
 * states its own colours over a base that states none.
 */
const BUTTON_SHAPE =
  "px-3 py-1.5 border rounded-md cursor-pointer hover:border-accent " +
  "disabled:opacity-45 disabled:cursor-default";
const BUTTON = `${BUTTON_SHAPE} bg-panel-2 text-ink border-line`;
const BUTTON_PRIMARY = `${BUTTON_SHAPE} bg-accent-deep text-ink border-accent-edge`;
/** For a button that must not compete with what it sits beside. */
const BUTTON_QUIET = `${BUTTON_SHAPE} bg-panel-2 text-ink-dim border-line`;
/** Removal. The only red button in the app. */
const BUTTON_DANGER = `${BUTTON_SHAPE} bg-panel-2 text-bad border-line`;
const FIELD =
  "w-full px-2.5 py-[7px] bg-panel-2 text-ink border border-line rounded-md " +
  "focus:outline-none focus:border-accent";
const CHIP = "border border-line rounded-xs px-1";
/** The uppercase micro-label above a value. */
const CAPTION = "block mb-1.5 text-ink-dim text-small";
/** The card a question is asked on, over the dimmed picker. */
const ASK_BOX =
  "w-[min(420px,84%)] p-[18px] rounded-xl border border-line bg-panel " +
  "shadow-[0_18px_44px_rgb(0_0_0/0.55)]";

/**
 * Thumbnails already fetched, so reopening the dialog is not a reload.
 *
 * Module-level rather than component state: the picker unmounts nothing when it
 * closes, but the cache should survive a remount anyway, and nothing renders
 * from it directly — a card asks for its own picture.
 */
const thumbnails = new Map<string, string | null>();

/** A question waiting for an answer, and the promise it will settle. */
interface AskRequest {
  title: string;
  label: string;
  value: string;
  confirm: string;
  /** Offer to start from an existing part rather than the starter script. */
  from?: ProjectPart[];
  /** Skip path validation — for a title, which is prose rather than a path. */
  free?: boolean;
  settle: (answer: { value: string; from?: string } | null) => void;
}

/** A message, and optionally a confirmation. */
interface TellRequest {
  text: string;
  confirm?: string;
  settle: (ok: boolean) => void;
}

export function ProjectBrowser() {
  const dialog = useRef<HTMLDialogElement>(null);
  /** The folder whose parts the grid shows; "" is everything. */
  const folder = useSignal("");
  const query = useSignal("");
  const asking = useSignal<AskRequest | undefined>(undefined);
  const telling = useSignal<TellRequest | undefined>(undefined);
  const menu = useSignal<{ part: ProjectPart; x: number; y: number } | undefined>(undefined);
  const searchBox = useRef<HTMLInputElement>(null);

  const ask = (request: Omit<AskRequest, "settle">) =>
    new Promise<{ value: string; from?: string } | null>((settle) => {
      asking.value = { ...request, settle };
    });
  const tell = (text: string, confirm?: string) =>
    new Promise<boolean>((settle) => {
      telling.value = { text, confirm, settle };
    });

  const refresh = async () => {
    const projects = await engine.reloadProjects();
    // A folder that was deleted (by the user in Finder, or by an agent) must
    // not leave the grid pinned to something that no longer exists.
    if (folder.value && !folderPaths(projects.tree).includes(folder.value)) folder.value = "";
  };

  /** A host refusal, whole. Its wording names the fix; do not summarise it. */
  const failed = (error: unknown) =>
    tell(error instanceof Error ? error.message : String(error));

  const choose = async (path: string) => {
    try {
      await engine.openProject(path);
      // Closed here rather than by letting the effect below notice the signal.
      // A modal `<dialog>` makes the rest of the page inert, so for as long as
      // it is open the viewport takes no pointer events at all — and routing
      // the close through render-then-effect leaves it open for a frame or two
      // after the part is already loaded and drawn. `bracket.e2e.mjs` moves the
      // mouse onto an edge in exactly that gap and hovers nothing.
      dialog.current?.close();
      S.browserOpen.value = false;
    } catch (e) {
      await failed(e);
    }
  };

  const move = async (from: string, to: string) => {
    try {
      await backend.renameProject(from, to);
      if (S.openPath.peek() === from) S.openPath.value = to;
      await refresh();
    } catch (e) {
      await failed(e);
    }
  };

  // A `<dialog>` is opened by a method rather than an attribute, which is the
  // one thing here that has to be driven imperatively.
  useEffect(() => {
    const open = S.browserOpen.value;
    const element = dialog.current;
    if (!element) return;
    if (open && !element.open) {
      void refresh();
      element.showModal();
      query.value = "";
      searchBox.current?.focus();
    } else if (!open && element.open) {
      element.close();
    }
  }, [S.browserOpen.value]);

  const projects = S.projects.value;
  const matched = projects ? search(projects.tree, query.value) : [];
  const shown = flatten(folder.value ? within(matched, folder.value) : matched);
  // Nothing inside a closed picker is drawn. Not for the render cost — for the
  // network: every card fetches its own thumbnail, and a folder of a hundred
  // parts would otherwise pull a hundred pictures nobody has asked to see.
  const open = S.browserOpen.value;

  return (
    <dialog
      ref={dialog}
      // `m-auto` restores the centring a modal dialog gets from the browser: the
      // preflight zeroes every margin, and the UA's `margin: auto` is what puts
      // a dialog in the middle of the viewport.
      class="m-auto w-[min(1040px,92vw)] h-[min(680px,88vh)] p-0 overflow-hidden
             bg-panel text-ink border border-line rounded-2xl
             backdrop:bg-[rgb(8_9_12/0.62)] backdrop:backdrop-blur-[3px]"
      onClose={() => (S.browserOpen.value = false)}
      // Clicking the backdrop closes it; clicking inside must not.
      onClick={(e) => {
        if (e.target === dialog.current) S.browserOpen.value = false;
      }}
    >
      {open && (
      <>
      <div class="absolute top-3.5 right-[18px]">
        <button
          type="button"
          aria-label="Close"
          class={`${BUTTON_QUIET} font-mono text-[11px]`}
          onClick={() => (S.browserOpen.value = false)}
        >
          esc
        </button>
      </div>

      <header class="flex items-center gap-3 pl-[18px] pr-[62px] py-3.5 border-b border-line">
        <h2 class="m-0 text-[15px] font-semibold">Parts</h2>
        <input
          ref={searchBox}
          class={`flex-1 ${FIELD}`}
          type="search"
          placeholder="Search parts, folders, tags"
          value={query.value}
          onInput={(e) => (query.value = e.currentTarget.value)}
          onKeyDown={(e) => {
            // Enter opens the only thing left, which is what a search box is for.
            if (e.key === "Enter" && shown.length) void choose(shown[0].path);
          }}
        />
        <button
          type="button"
          class={`shrink-0 flex items-center gap-1.5 ${BUTTON}`}
          onClick={() => void newFolder(folder.value, ask, failed, refresh)}
        >
          <Icon name="folder" class="size-4 shrink-0" />
          <span>New folder</span>
        </button>
        <button
          type="button"
          class={`shrink-0 flex items-center gap-1.5 ${BUTTON_PRIMARY}`}
          onClick={() => void newPart(folder.value, ask, failed, refresh, choose)}
        >
          <Icon name="plus" class="size-4 shrink-0" />
          <span>New part</span>
        </button>
      </header>

      <div class="flex h-[calc(100%-108px)]">
        <nav class="w-[220px] flex-none overflow-auto px-2 py-2.5 border-r border-line">
          <FolderButton
            path=""
            name="All parts"
            count={flatten(matched).length}
            depth={0}
            current={folder}
            onDrop={move}
          />
          <Branch entries={matched} depth={1} current={folder} onDrop={move} />
          {/* Without a query the tree is the whole folder, so it is also the
              place to say the folder is empty. */}
          {!query.value && !projects?.parts.length && (
            <p class="mx-1 my-6 text-ink-dim">Nothing here yet.</p>
          )}
        </nav>

        <div class="flex-1 overflow-auto p-4">
          <div class="grid gap-3.5 grid-cols-[repeat(auto-fill,minmax(168px,1fr))]">
            {shown.map((part) => (
              <Card
                key={part.path}
                part={part}
                showFolder={!folder.value}
                onOpen={() => void choose(part.path)}
                onMenu={(x, y) => (menu.value = { part, x, y })}
              />
            ))}
            {!shown.length && (
              <p class="col-span-full mx-1 my-6 text-ink-dim">
                {query.value
                  ? `Nothing matches “${query.value}”.`
                  : "This folder has no parts. New part puts one here."}
              </p>
            )}
          </div>
        </div>
      </div>

      <footer
        class="flex items-center justify-between gap-3 h-10 px-[18px] border-t border-line
               text-ink-dim font-mono text-[11px]"
      >
        <span
          class="overflow-hidden text-ellipsis whitespace-nowrap"
          {...tip({
            title: "The project folder",
            text: "Every part is a file here. The app, you and any agent on MCP read and write the same folder.",
          })}
        >
          {projects?.directory ?? ""}
        </span>
        <span>
          {shown.length} part{shown.length === 1 ? "" : "s"}
        </span>
      </footer>

      {menu.value && (
        <PartMenu
          at={menu.value}
          onClose={() => (menu.value = undefined)}
          choose={choose}
          move={move}
          ask={ask}
          tell={tell}
          failed={failed}
          refresh={refresh}
        />
      )}
      {asking.value && <AskPanel request={asking} />}
      {telling.value && <TellPanel request={telling} />}
      </>
      )}
    </dialog>
  );
}

// ---------------------------------------------------------------- the tree

function Branch({
  entries,
  depth,
  current,
  onDrop,
}: {
  entries: ProjectEntry[];
  depth: number;
  current: Signal<string>;
  onDrop: (from: string, to: string) => Promise<void>;
}) {
  return (
    <>
      {entries.map((entry) =>
        entry.kind !== "folder" ? null : (
          <Fragment key={entry.path}>
            <FolderButton
              path={entry.path}
              name={entry.name}
              count={flatten(entry.children).length}
              depth={depth}
              current={current}
              onDrop={onDrop}
            />
            <Branch entries={entry.children} depth={depth + 1} current={current} onDrop={onDrop} />
          </Fragment>
        ),
      )}
    </>
  );
}

function FolderButton({
  path,
  name,
  count,
  depth,
  current,
  onDrop,
}: {
  path: string;
  name: string;
  count: number;
  depth: number;
  current: Signal<string>;
  onDrop: (from: string, to: string) => Promise<void>;
}) {
  const over = useSignal(false);
  const selected = current.value === path;
  return (
    <button
      type="button"
      // The visible label is two spans of which one is a bare number, so say the
      // whole thing once for anything reading the page rather than looking at it.
      aria-label={`${name}, ${count} part${count === 1 ? "" : "s"}`}
      class={[
        "flex w-full items-center justify-between gap-2 mb-0.5 rounded-md px-2 py-[5px]",
        "text-left border hover:bg-panel-2",
        over.value ? "border-accent text-accent" : "border-transparent",
        selected ? "bg-panel-2 text-ink" : "text-ink-dim",
      ].join(" ")}
      // Indent by depth. A padding utility per level would be a scale invented
      // to avoid writing the arithmetic that is the actual rule.
      style={{ paddingLeft: `${8 + depth * 14}px` }}
      onClick={() => (current.value = path)}
      // Dropping a part on a folder moves it, which is the one gesture people
      // try before looking for a menu.
      onDragOver={(e) => {
        e.preventDefault();
        over.value = true;
      }}
      onDragLeave={() => (over.value = false)}
      onDrop={(e) => {
        e.preventDefault();
        over.value = false;
        const from = e.dataTransfer?.getData("text/parcad-part");
        if (from && parentOf(from) !== path) void onDrop(from, join(path, leafOf(from)));
      }}
    >
      <span class="overflow-hidden text-ellipsis whitespace-nowrap">{name}</span>
      <span class="font-mono text-[11px] opacity-60">{count}</span>
    </button>
  );
}

/** The entries under one folder path, or nothing if it is gone. */
function within(entries: ProjectEntry[], path: string): ProjectEntry[] {
  for (const entry of entries) {
    if (entry.kind !== "folder") continue;
    if (entry.path === path) return entry.children;
    const found = within(entry.children, path);
    if (found.length) return found;
  }
  return [];
}

// ---------------------------------------------------------------- the grid

function Card({
  part,
  showFolder,
  onOpen,
  onMenu,
}: {
  part: ProjectPart;
  showFolder: boolean;
  onOpen: () => void;
  onMenu: (x: number, y: number) => void;
}) {
  const open = S.openPath.value === part.path;
  // The folder is worth showing only when the grid is mixing folders.
  const where = showFolder && parentOf(part.path) ? `${parentOf(part.path)} · ` : "";

  return (
    <article
      draggable
      class={[
        // A query hook, not styling: `browser-card` is how the desktop
        // end-to-end suite finds a part in the grid, and nothing in CSS matches
        // it. Renaming it is a test change, not a refactor.
        "browser-card group relative overflow-hidden rounded-xl bg-panel-2 border",
        open ? "border-accent" : "border-line hover:border-[#38455c]",
      ].join(" ")}
      onDragStart={(e) => e.dataTransfer?.setData("text/parcad-part", part.path)}
    >
      <button
        type="button"
        aria-label={`Open ${part.title} (${part.path})`}
        class="block w-full text-left cursor-pointer"
        onClick={onOpen}
      >
        <Thumbnail part={part} />
        <span class="block px-2.5 pt-2 pb-0.5 overflow-hidden text-ellipsis whitespace-nowrap">
          {part.title}
        </span>
        <span class="block px-2.5 pb-2.5 text-ink-dim font-mono text-[10.5px]">
          {where}
          {when(part.modified)}
          {!part.bundle && (
            <>
              {" "}
              {/* Amber, not the accent: it marks a part that cannot hold a
                  title or a thumbnail, which is a limitation rather than a
                  state. */}
              <span
                class={`${CHIP} text-[#d8a657] border-[#4a3c1f]`}
                {...tip({
                  title: "A loose script",
                  text: "A .js file rather than a .parcad folder, so it has no title, description or thumbnail of its own. Convert it from the ··· menu.",
                })}
              >
                .js
              </span>
            </>
          )}
          {part.tags.map((tag) => (
            <Fragment key={tag}>
              {" "}
              <span class={CHIP}>{tag}</span>
            </Fragment>
          ))}
        </span>
      </button>
      <button
        type="button"
        aria-label={`More actions for ${part.title}`}
        class="absolute top-2 right-2 px-[7px] leading-5 rounded-md border border-line
               bg-[rgb(16_18_22/0.78)] text-ink cursor-pointer
               opacity-0 group-hover:opacity-100 focus:opacity-100"
        onClick={(e) => {
          e.stopPropagation();
          onMenu(e.clientX, e.clientY);
        }}
      >
        ···
      </button>
    </article>
  );
}

/**
 * A card's picture, or the part's initials.
 *
 * One request per card rather than base64 in the listing: a folder of a hundred
 * parts would otherwise be a megabyte of JSON to draw a dozen tiles.
 */
function Thumbnail({ part }: { part: ProjectPart }) {
  const url = useSignal<string | null | undefined>(
    part.thumbnail ? thumbnails.get(part.path) : null,
  );

  useEffect(() => {
    if (!part.thumbnail || url.value !== undefined) return;
    let live = true;
    void backend.projectPreview(part.path).then((got) => {
      thumbnails.set(part.path, got);
      if (live) url.value = got;
    });
    return () => {
      live = false;
    };
  }, [part.path, part.thumbnail]);

  return (
    <span class="flex h-[108px] items-center justify-center overflow-hidden bg-well border-b border-line">
      {url.value ? (
        <img src={url.value} alt="" class="w-full h-full object-cover" />
      ) : (
        <span class="text-ink-dim text-[22px] uppercase tracking-[0.06em] opacity-45">
          {part.title.slice(0, 2)}
        </span>
      )}
    </span>
  );
}

// --------------------------------------------------------------- the verbs

type Ask = (request: Omit<AskRequest, "settle">) => Promise<{ value: string; from?: string } | null>;
type Tell = (text: string, confirm?: string) => Promise<boolean>;
type Failed = (error: unknown) => Promise<boolean>;

async function newPart(
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

async function newFolder(
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

function PartMenu({
  at,
  onClose,
  choose,
  move,
  ask,
  tell,
  failed,
  refresh,
}: {
  at: { part: ProjectPart; x: number; y: number };
  onClose: () => void;
  choose: (path: string) => Promise<void>;
  move: (from: string, to: string) => Promise<void>;
  ask: Ask;
  tell: Tell;
  failed: Failed;
  refresh: () => Promise<void>;
}) {
  const part = at.part;

  // One click anywhere else closes it. Registered on the next frame so the
  // click that opened the menu is not the click that closes it.
  useEffect(() => {
    const frame = requestAnimationFrame(() =>
      document.addEventListener("pointerdown", onClose, { once: true }),
    );
    return () => {
      cancelAnimationFrame(frame);
      document.removeEventListener("pointerdown", onClose);
    };
  }, []);

  const item = (text: string, run: () => void | Promise<void>, danger = false) => (
    <button
      type="button"
      class={`rounded-md px-2.5 py-[7px] text-left cursor-pointer hover:bg-panel-2 ${danger ? "text-bad" : "text-ink"}`}
      onClick={async () => {
        onClose();
        await run();
      }}
    >
      {text}
    </button>
  );

  return (
    <div
      class="fixed z-10 flex flex-col min-w-[190px] p-[5px] rounded-[9px] border border-line
             bg-panel shadow-[0_12px_34px_rgb(0_0_0/0.5)]"
      style={{ left: `${at.x}px`, top: `${at.y}px` }}
    >
      {item("Open", () => choose(part.path))}
      {item("Rename…", async () => {
        const name = await ask({
          title: "Rename",
          label: "New name — the file on disk is renamed too",
          value: part.name,
          confirm: "Rename",
        });
        if (!name || name.value === part.name) return;
        await move(part.path, join(parentOf(part.path), name.value));
      })}
      {part.bundle &&
        item("Retitle…", async () => {
          const title = await ask({
            title: "Title",
            label: "What the picker shows. The file keeps its name.",
            value: part.title,
            confirm: "Set",
            // A title is prose: it may contain anything, unlike a path.
            free: true,
          });
          if (!title) return;
          try {
            await backend.setProjectTitle(part.path, title.value);
            await refresh();
          } catch (e) {
            await failed(e);
          }
        })}
      {item("Duplicate", async () => {
        const projects = S.projects.peek();
        if (!projects) return;
        const taken = projects.parts.map((p) => p.path);
        const path = freePath(taken, parentOf(part.path), part.name);
        try {
          await backend.createProject(path, await backend.readProject(part.path));
          await refresh();
        } catch (e) {
          await failed(e);
        }
      })}
      {!part.bundle &&
        item("Convert to project folder", async () => {
          try {
            await backend.convertProject(part.path);
            await refresh();
          } catch (e) {
            await failed(e);
          }
        })}
      {item(
        "Move to trash",
        async () => {
          // Says where it goes, because it goes somewhere. `remove` moves a
          // project into the folder's `.trash` rather than unlinking it, and a
          // warning that implies otherwise would be the wrong warning.
          const ok = await tell(
            `Move “${part.title}” to the trash?\n\n` +
              "It goes to the .trash folder inside your project folder, so you can " +
              "get it back by hand.",
            "Move to trash",
          );
          if (!ok) return;
          try {
            await backend.deleteProject(part.path);
            if (S.openPath.peek() === part.path) S.openPath.value = undefined;
            thumbnails.delete(part.path);
            await refresh();
          } catch (e) {
            await failed(e);
          }
        },
        true,
      )}
    </div>
  );
}

// ------------------------------------------------------- asking for a name

/**
 * A name, or null if the user backed out.
 *
 * Not `window.prompt`: the desktop webview does not have one, and a browser's
 * cannot mark a name invalid while it is being typed.
 */
function AskPanel({ request }: { request: Signal<AskRequest | undefined> }) {
  const value = useSignal(request.value!.value);
  const from = useSignal("");
  const input = useRef<HTMLInputElement>(null);
  const ask = request.value!;

  useEffect(() => {
    input.current?.focus();
    input.current?.select();
  }, []);

  const problem = ask.free ? null : nameProblem(value.value);
  const done = (answer: { value: string; from?: string } | null) => {
    request.value = undefined;
    ask.settle(answer);
  };
  const submit = () => {
    if (problem === null) done({ value: value.value.trim(), from: from.value || undefined });
  };

  return (
    <Scrim>
      <div class={ASK_BOX}>
        <h3 class="m-0 mb-2.5 text-sm">{ask.title}</h3>
        <label class={CAPTION}>{ask.label}</label>
        <input
          ref={input}
          class={FIELD}
          type="text"
          spellcheck={false}
          value={value.value}
          onInput={(e) => (value.value = e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              submit();
            }
            // Escape closes the question, not the whole dialog behind it.
            if (e.key === "Escape") {
              e.preventDefault();
              e.stopPropagation();
              done(null);
            }
          }}
        />
        <p class="min-h-4 mt-1.5 mb-1 text-bad text-[11.5px]">{problem ?? ""}</p>
        {ask.from && (
          <label class={CAPTION}>
            Start from
            <select
              class="w-full mt-1"
              value={from.value}
              onChange={(e) => (from.value = e.currentTarget.value)}
            >
              <option value="">an empty part</option>
              {ask.from.map((part) => (
                <option key={part.path} value={part.path}>
                  {part.path}
                </option>
              ))}
            </select>
          </label>
        )}
        <div class="flex justify-end gap-2 mt-2.5">
          <button type="button" class={BUTTON} onClick={() => done(null)}>
            Cancel
          </button>
          <button type="button" class={BUTTON_PRIMARY} disabled={problem !== null} onClick={submit}>
            {ask.confirm}
          </button>
        </div>
      </div>
    </Scrim>
  );
}

function TellPanel({ request }: { request: Signal<TellRequest | undefined> }) {
  const ask = request.value!;
  const focus = useRef<HTMLButtonElement>(null);
  useEffect(() => focus.current?.focus(), []);

  const done = (ok: boolean) => {
    request.value = undefined;
    ask.settle(ok);
  };

  return (
    <Scrim>
      <div class={ASK_BOX}>
        <p class="m-0 mb-3.5 whitespace-pre-wrap">{ask.text}</p>
        <div class="flex justify-end gap-2 mt-2.5">
          <button
            type="button"
            ref={ask.confirm ? undefined : focus}
            class={BUTTON}
            onClick={() => done(false)}
          >
            {ask.confirm ? "Cancel" : "OK"}
          </button>
          {ask.confirm && (
            <button type="button" ref={focus} class={BUTTON_DANGER} onClick={() => done(true)}>
              {ask.confirm}
            </button>
          )}
        </div>
      </div>
    </Scrim>
  );
}

const Scrim = ({ children }: { children: ComponentChildren }) => (
  <div class="absolute inset-0 flex items-center justify-center bg-[rgb(8_9_12/0.66)]">
    {children}
  </div>
);
