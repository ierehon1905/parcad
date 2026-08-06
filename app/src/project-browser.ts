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

import * as backend from "./backend";
import {
  describeProjects,
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
  type Projects,
} from "./projects";

/**
 * Class strings that more than one element wears.
 *
 * Named constants rather than CSS classes: a utility list is still the styling,
 * and putting it in a `.css` file would split one component's appearance across
 * two languages again. Anything worn once stays inline on its element.
 */
/**
 * Shape, then colour — and never both from two sources.
 *
 * Two utilities that set the same property do not resolve by the order they
 * appear in the class attribute; they resolve by their order in the generated
 * stylesheet. So a variant cannot be "the base, plus a different background":
 * whichever of the two Tailwind emits last wins, everywhere, silently. Each
 * variant therefore states its own colours over a base that states none.
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

export interface Hooks {
  /** Load a part into the editor. The browser closes once this resolves. */
  open(path: string): Promise<void>;
  /** The part on screen, so the grid can mark it and rename can follow it. */
  current(): string | undefined;
  /** A part was renamed, moved or removed; the titlebar may need to change. */
  changed(path: string | undefined): void;
}

export interface ProjectBrowser {
  /** Open the dialog, reading the folder fresh. */
  show(): Promise<void>;
  /** The parts as of the last read — the titlebar uses this for a title. */
  known(): Projects | undefined;
  /** Re-read the folder without opening anything. */
  reload(): Promise<Projects>;
}

export function mountProjectBrowser(hooks: Hooks): ProjectBrowser {
  const dialog = document.createElement("dialog");
  dialog.id = "browser";
  // The `browser-*` classes below are query hooks, not styling: every one of
  // them is how this file finds an element again, and nothing in CSS matches
  // them. Appearance is the utilities beside them.
  dialog.className = [
    // `m-auto` restores the centring a modal dialog gets from the browser: the
    // preflight zeroes every margin, and the UA's `margin: auto` is what puts a
    // dialog in the middle of the viewport.
    "browser m-auto w-[min(1040px,92vw)] h-[min(680px,88vh)] p-0 overflow-hidden",
    "bg-panel text-ink border border-line rounded-2xl",
    "backdrop:bg-[rgb(8_9_12/0.62)] backdrop:backdrop-blur-[3px]",
  ].join(" ");
  dialog.innerHTML = `
    <form method="dialog" class="absolute top-3.5 right-[18px] m-0">
      <button
        value="cancel"
        aria-label="Close"
        class="${BUTTON_QUIET} font-mono text-[11px]"
      >esc</button>
    </form>
    <header class="flex items-center gap-3 pl-[18px] pr-[62px] py-3.5 border-b border-line">
      <h2 class="m-0 text-[15px] font-semibold">Parts</h2>
      <input
        class="browser-search flex-1 ${FIELD}"
        type="search"
        placeholder="Search parts, folders, tags"
      />
      <button type="button" class="browser-new-folder shrink-0 ${BUTTON}">New folder</button>
      <button type="button" class="browser-new shrink-0 ${BUTTON_PRIMARY}">New part</button>
    </header>
    <div class="flex h-[calc(100%-108px)]">
      <nav class="browser-tree w-[220px] flex-none overflow-auto px-2 py-2.5 border-r border-line"></nav>
      <div class="flex-1 overflow-auto p-4">
        <div class="browser-grid grid gap-3.5 grid-cols-[repeat(auto-fill,minmax(168px,1fr))]"></div>
      </div>
    </div>
    <footer
      class="flex items-center justify-between gap-3 h-10 px-[18px] border-t border-line
             text-ink-dim font-mono text-[11px]"
    >
      <span class="browser-where overflow-hidden text-ellipsis whitespace-nowrap"></span>
      <span class="browser-count"></span>
    </footer>
    <div class="browser-ask absolute inset-0 flex items-center justify-center bg-[rgb(8_9_12/0.66)]" hidden></div>`;
  document.body.append(dialog);

  const el = <T extends Element>(selector: string) => dialog.querySelector<T>(selector)!;
  const searchBox = el<HTMLInputElement>(".browser-search");
  const treeEl = el<HTMLElement>(".browser-tree");
  const gridEl = el<HTMLElement>(".browser-grid");
  const whereEl = el<HTMLElement>(".browser-where");
  const countEl = el<HTMLElement>(".browser-count");

  let projects: Projects | undefined;
  /** The folder whose parts the grid shows; "" is everything. */
  let folder = "";
  /** Thumbnails already fetched, so reopening the dialog is not a reload. */
  const thumbnails = new Map<string, string | null>();

  // -------------------------------------------------------------- reading

  async function reload(): Promise<Projects> {
    projects = describeProjects(await backend.listProjects());
    // A folder that was deleted (by the user in Finder, or by an agent) must
    // not leave the grid pinned to something that no longer exists.
    if (folder && !folderPaths(projects.tree).includes(folder)) folder = "";
    return projects;
  }

  async function refresh() {
    await reload();
    draw();
  }

  // -------------------------------------------------------------- drawing

  function draw() {
    if (!projects) return;
    const query = searchBox.value;
    const matched = search(projects.tree, query);

    drawTree(matched, query);
    drawGrid(matched, query);

    whereEl.textContent = projects.directory;
    whereEl.title =
      "Every part is a file here. The app, you and any agent on MCP read and " +
      "write the same folder.";
  }

  function drawTree(matched: ProjectEntry[], query: string) {
    treeEl.replaceChildren();
    treeEl.append(
      folderButton("", "All parts", flatten(matched).length, 0),
      ...branch(matched, 1),
    );
    if (query) return;

    // Without a query the tree is the whole folder, so it is also the place to
    // say the folder is empty.
    if (!projects?.parts.length) {
      const empty = document.createElement("p");
      empty.className = "mx-1 my-6 text-ink-dim";
      empty.textContent = "Nothing here yet.";
      treeEl.append(empty);
    }
  }

  function branch(entries: ProjectEntry[], depth: number): HTMLElement[] {
    return entries.flatMap((entry) => {
      if (entry.kind !== "folder") return [];
      const count = flatten(entry.children).length;
      return [
        folderButton(entry.path, entry.name, count, depth),
        ...branch(entry.children, depth + 1),
      ];
    });
  }

  function folderButton(path: string, name: string, count: number, depth: number) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = [
      "browser-folder flex w-full items-center justify-between gap-2 mb-0.5",
      "rounded-md px-2 py-[5px] text-left border border-transparent",
      "hover:bg-panel-2",
      folder === path ? "bg-panel-2 text-ink" : "text-ink-dim",
    ].join(" ");
    // Indent by depth. A padding utility per level would be a scale invented to
    // avoid writing the arithmetic that is the actual rule.
    button.style.paddingLeft = `${8 + depth * 14}px`;
    button.innerHTML = `
      <span class="browser-folder-name overflow-hidden text-ellipsis whitespace-nowrap"></span>
      <span class="browser-folder-count font-mono text-[11px] opacity-60"></span>`;
    // The visible label is two spans of which one is a bare number, so say the
    // whole thing once for anything reading the page rather than looking at it.
    button.setAttribute(
      "aria-label",
      `${name}, ${count} part${count === 1 ? "" : "s"}`,
    );
    button.querySelector(".browser-folder-name")!.textContent = name;
    button.querySelector(".browser-folder-count")!.textContent = String(count);
    button.addEventListener("click", () => {
      folder = path;
      draw();
    });
    // Dropping a part on a folder moves it, which is the one gesture people try
    // before looking for a menu.
    button.addEventListener("dragover", (e) => {
      e.preventDefault();
      button.classList.add("border-accent", "text-accent");
    });
    button.addEventListener("dragleave", () =>
      button.classList.remove("border-accent", "text-accent"),
    );
    button.addEventListener("drop", async (e) => {
      e.preventDefault();
      button.classList.remove("border-accent", "text-accent");
      const from = e.dataTransfer?.getData("text/parcad-part");
      if (from && parentOf(from) !== path) await move(from, join(path, leafOf(from)));
    });
    return button;
  }

  function drawGrid(matched: ProjectEntry[], query: string) {
    const shown = flatten(folder ? within(matched, folder) : matched);
    gridEl.replaceChildren(...shown.map(card));
    countEl.textContent = `${shown.length} part${shown.length === 1 ? "" : "s"}`;

    if (shown.length) return;
    const empty = document.createElement("p");
    empty.className = "col-span-full mx-1 my-6 text-ink-dim";
    empty.textContent = query
      ? `Nothing matches “${query}”.`
      : "This folder has no parts. New part puts one here.";
    gridEl.append(empty);
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

  function card(part: ProjectPart): HTMLElement {
    const card = document.createElement("article");
    card.className = [
      "browser-card group relative overflow-hidden rounded-xl bg-panel-2 border",
      hooks.current() === part.path ? "border-accent" : "border-line hover:border-[#38455c]",
    ].join(" ");
    card.draggable = true;
    card.innerHTML = `
      <button type="button" class="browser-open block w-full text-left cursor-pointer">
        <span
          class="browser-thumb flex h-[108px] items-center justify-center overflow-hidden
                 bg-well border-b border-line"
        ></span>
        <span class="browser-name block px-2.5 pt-2 pb-0.5 overflow-hidden text-ellipsis whitespace-nowrap"></span>
        <span class="browser-meta block px-2.5 pb-2.5 text-ink-dim font-mono text-[10.5px]"></span>
      </button>
      <button
        type="button"
        class="browser-more absolute top-2 right-2 px-[7px] leading-5 rounded-md border border-line
               bg-[rgb(16_18_22/0.78)] text-ink cursor-pointer
               opacity-0 group-hover:opacity-100 focus:opacity-100"
      >···</button>`;

    card.querySelector(".browser-name")!.textContent = part.title;
    card
      .querySelector(".browser-open")!
      .setAttribute("aria-label", `Open ${part.title} (${part.path})`);
    card.querySelector(".browser-more")!.setAttribute(
      "aria-label",
      `More actions for ${part.title}`,
    );
    const meta = card.querySelector(".browser-meta")!;
    // The folder is worth showing only when the grid is mixing folders.
    const where = !folder && parentOf(part.path) ? `${parentOf(part.path)} · ` : "";
    meta.textContent = `${where}${when(part.modified)}`;
    if (!part.bundle) {
      const loose = document.createElement("span");
      // Amber, not the accent: it marks a part that cannot hold a title or a
      // thumbnail, which is a limitation rather than a state.
      loose.className = `${CHIP} text-[#d8a657] border-[#4a3c1f]`;
      loose.textContent = ".js";
      loose.title =
        "A loose script rather than a .parcad folder, so it has no title, " +
        "description or thumbnail of its own. Convert it from the ··· menu.";
      meta.append(" ", loose);
    }
    for (const tag of part.tags) {
      const chip = document.createElement("span");
      chip.className = CHIP;
      chip.textContent = tag;
      meta.append(" ", chip);
    }

    thumbnail(part, card.querySelector<HTMLElement>(".browser-thumb")!);

    card.querySelector(".browser-open")!.addEventListener("click", () => choose(part.path));
    card.querySelector(".browser-more")!.addEventListener("click", (e) => {
      e.stopPropagation();
      menu(part, e as MouseEvent);
    });
    card.addEventListener("dragstart", (e) => {
      e.dataTransfer?.setData("text/parcad-part", part.path);
    });
    return card;
  }

  /**
   * Fill a card's thumbnail, or leave it showing the part's initial.
   *
   * One request per card rather than base64 in the listing: a folder of a
   * hundred parts would otherwise be a megabyte of JSON to draw a dozen tiles.
   */
  /** What a part with no picture shows instead: the first two letters. */
  function initial(part: ProjectPart, into: HTMLElement) {
    into.textContent = part.title.slice(0, 2);
    into.classList.add("text-ink-dim", "text-[22px]", "uppercase", "tracking-[0.06em]", "opacity-45");
  }

  async function thumbnail(part: ProjectPart, into: HTMLElement) {
    if (!part.thumbnail) {
      initial(part, into);
      return;
    }
    let url = thumbnails.get(part.path);
    if (url === undefined) {
      url = await backend.projectPreview(part.path);
      thumbnails.set(part.path, url);
    }
    if (!url) {
      initial(part, into);
      return;
    }
    const img = document.createElement("img");
    img.src = url;
    img.alt = "";
    img.className = "w-full h-full object-cover";
    into.replaceChildren(img);
  }

  // ------------------------------------------------------------- the verbs

  async function choose(path: string) {
    try {
      await hooks.open(path);
      dialog.close();
    } catch (e) {
      await tell(message(e));
    }
  }

  async function newPart() {
    if (!projects) return;
    const taken = projects.parts.map((part) => part.path);
    const name = await ask({
      title: "New part",
      label: `Name, in ${folder || "the project folder"}`,
      value: leafOf(freePath(taken, folder, "part")),
      confirm: "Create",
      // Copying an existing part is how most parts actually start, and the
      // seeded ones are the parts the eval corpus measures — so "from" is a
      // list of things known to build, not a gallery of templates.
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
      await tell(message(e));
    }
  }

  async function newFolder() {
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
      await tell(message(e));
    }
  }

  async function move(from: string, to: string) {
    try {
      await backend.renameProject(from, to);
      if (hooks.current() === from) hooks.changed(to);
      await refresh();
    } catch (e) {
      await tell(message(e));
    }
  }

  function menu(part: ProjectPart, at: MouseEvent) {
    const menu = document.createElement("div");
    menu.className = [
      "browser-menu fixed z-10 flex flex-col min-w-[190px] p-[5px]",
      "rounded-[9px] border border-line bg-panel shadow-[0_12px_34px_rgb(0_0_0/0.5)]",
    ].join(" ");
    menu.style.left = `${at.clientX}px`;
    menu.style.top = `${at.clientY}px`;

    const item = (text: string, run: () => void | Promise<void>, danger = false) => {
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = text;
      button.className = [
        "rounded-md px-2.5 py-[7px] text-left cursor-pointer hover:bg-panel-2",
        danger ? "text-bad" : "text-ink",
      ].join(" ");
      button.addEventListener("click", async () => {
        menu.remove();
        await run();
      });
      menu.append(button);
    };

    item("Open", () => choose(part.path));
    item("Rename…", () => rename(part));
    if (part.bundle) item("Retitle…", () => retitle(part));
    item("Duplicate", () => duplicate(part));
    if (!part.bundle) item("Convert to project folder", () => convert(part));
    item("Move to trash", () => remove(part), true);

    document.body.append(menu);
    // One click anywhere else closes it. Registered on the next frame so the
    // click that opened the menu is not the click that closes it.
    requestAnimationFrame(() =>
      document.addEventListener("pointerdown", () => menu.remove(), { once: true }),
    );
  }

  async function rename(part: ProjectPart) {
    const name = await ask({
      title: "Rename",
      label: "New name — the file on disk is renamed too",
      value: part.name,
      confirm: "Rename",
    });
    if (!name || name.value === part.name) return;
    await move(part.path, join(parentOf(part.path), name.value));
  }

  async function retitle(part: ProjectPart) {
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
      await tell(message(e));
    }
  }

  async function duplicate(part: ProjectPart) {
    if (!projects) return;
    const taken = projects.parts.map((p) => p.path);
    const path = freePath(taken, parentOf(part.path), part.name);
    try {
      await backend.createProject(path, await backend.readProject(part.path));
      await refresh();
    } catch (e) {
      await tell(message(e));
    }
  }

  async function convert(part: ProjectPart) {
    try {
      await backend.convertProject(part.path);
      await refresh();
    } catch (e) {
      await tell(message(e));
    }
  }

  async function remove(part: ProjectPart) {
    // Says where it goes, because it goes somewhere. `remove` moves a project
    // into the folder's `.trash` rather than unlinking it, and a warning that
    // implies otherwise would be the wrong warning.
    const ok = await tell(
      `Move “${part.title}” to the trash?\n\n` +
        "It goes to the .trash folder inside your project folder, so you can " +
        "get it back by hand.",
      "Move to trash",
    );
    if (!ok) return;
    try {
      await backend.deleteProject(part.path);
      if (hooks.current() === part.path) hooks.changed(undefined);
      thumbnails.delete(part.path);
      await refresh();
    } catch (e) {
      await tell(message(e));
    }
  }

  // ------------------------------------------------------- asking for a name

  interface Ask {
    title: string;
    label: string;
    value: string;
    confirm: string;
    /** Offer to start from an existing part rather than the starter script. */
    from?: ProjectPart[];
    /** Skip path validation — for a title, which is prose rather than a path. */
    free?: boolean;
  }

  /**
   * A name, or null if the user backed out.
   *
   * Not `window.prompt`: the desktop webview does not have one, and a browser's
   * cannot mark a name invalid while it is being typed.
   */
  function ask(request: Ask): Promise<{ value: string; from?: string } | null> {
    const panel = el<HTMLElement>(".browser-ask");
    panel.hidden = false;
    panel.innerHTML = `
      <div class="${ASK_BOX}">
        <h3 class="m-0 mb-2.5 text-sm"></h3>
        <label class="browser-ask-label ${CAPTION}"></label>
        <input class="browser-ask-input ${FIELD}" type="text" spellcheck="false" />
        <p class="browser-ask-problem min-h-4 mt-1.5 mb-1 text-bad text-[11.5px]"></p>
        <label class="browser-ask-from ${CAPTION}" hidden>Start from
          <select class="browser-ask-from-select w-full mt-1"></select>
        </label>
        <div class="flex justify-end gap-2 mt-2.5">
          <button type="button" class="browser-ask-cancel ${BUTTON}">Cancel</button>
          <button type="button" class="browser-ask-ok ${BUTTON_PRIMARY}"></button>
        </div>
      </div>`;

    panel.querySelector("h3")!.textContent = request.title;
    panel.querySelector(".browser-ask-label")!.textContent = request.label;
    const input = panel.querySelector<HTMLInputElement>(".browser-ask-input")!;
    const problemEl = panel.querySelector<HTMLElement>(".browser-ask-problem")!;
    const ok = panel.querySelector<HTMLButtonElement>(".browser-ask-ok")!;
    ok.textContent = request.confirm;
    input.value = request.value;

    const fromLabel = panel.querySelector<HTMLElement>(".browser-ask-from")!;
    const fromSelect = panel.querySelector<HTMLSelectElement>(".browser-ask-from-select")!;
    if (request.from) {
      fromLabel.hidden = false;
      const blank = document.createElement("option");
      blank.value = "";
      blank.textContent = "an empty part";
      fromSelect.append(blank);
      for (const part of request.from) {
        const option = document.createElement("option");
        option.value = part.path;
        option.textContent = part.path;
        fromSelect.append(option);
      }
    }

    return new Promise((resolve) => {
      const check = () => {
        const problem = request.free ? null : nameProblem(input.value);
        problemEl.textContent = problem ?? "";
        ok.disabled = problem !== null;
        return problem === null;
      };
      const done = (answer: { value: string; from?: string } | null) => {
        panel.hidden = true;
        panel.replaceChildren();
        resolve(answer);
      };

      check();
      input.addEventListener("input", check);
      input.addEventListener("keydown", (e) => {
        if (e.key === "Enter" && check()) {
          e.preventDefault();
          done({ value: input.value.trim(), from: fromSelect.value || undefined });
        }
        // Escape closes the question, not the whole dialog behind it.
        if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          done(null);
        }
      });
      ok.addEventListener("click", () => {
        if (check()) done({ value: input.value.trim(), from: fromSelect.value || undefined });
      });
      panel.querySelector(".browser-ask-cancel")!.addEventListener("click", () => done(null));
      input.focus();
      input.select();
    });
  }

  /** A message, and optionally a confirmation. Resolves true if confirmed. */
  function tell(text: string, confirm?: string): Promise<boolean> {
    const panel = el<HTMLElement>(".browser-ask");
    panel.hidden = false;
    panel.innerHTML = `
      <div class="${ASK_BOX}">
        <p class="browser-ask-text m-0 mb-3.5 whitespace-pre-wrap"></p>
        <div class="flex justify-end gap-2 mt-2.5">
          <button type="button" class="browser-ask-cancel ${BUTTON}"></button>
          <button type="button" class="browser-ask-ok ${BUTTON_DANGER}" hidden></button>
        </div>
      </div>`;
    panel.querySelector(".browser-ask-text")!.textContent = text;
    const cancel = panel.querySelector<HTMLButtonElement>(".browser-ask-cancel")!;
    const ok = panel.querySelector<HTMLButtonElement>(".browser-ask-ok")!;
    cancel.textContent = confirm ? "Cancel" : "OK";
    if (confirm) {
      ok.hidden = false;
      ok.textContent = confirm;
    }

    return new Promise((resolve) => {
      const done = (answer: boolean) => {
        panel.hidden = true;
        panel.replaceChildren();
        resolve(answer);
      };
      cancel.addEventListener("click", () => done(false));
      ok.addEventListener("click", () => done(true));
      (confirm ? ok : cancel).focus();
    });
  }

  // ---------------------------------------------------------------- wiring

  searchBox.addEventListener("input", draw);
  searchBox.addEventListener("keydown", (e) => {
    if (e.key !== "Enter" || !projects) return;
    // Enter opens the only thing left, which is what a search box is for.
    const shown = flatten(search(projects.tree, searchBox.value));
    if (shown.length) void choose(shown[0].path);
  });
  el(".browser-new").addEventListener("click", () => void newPart());
  el(".browser-new-folder").addEventListener("click", () => void newFolder());
  // Clicking the backdrop closes it; clicking inside must not.
  dialog.addEventListener("click", (e) => {
    if (e.target === dialog) dialog.close();
  });

  return {
    async show() {
      await refresh();
      if (!dialog.open) dialog.showModal();
      searchBox.value = "";
      searchBox.focus();
      draw();
    },
    known: () => projects,
    reload,
  };
}

/** A host refusal, whole. Its wording names the fix; do not summarise it. */
function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
