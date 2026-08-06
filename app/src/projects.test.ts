import { describe, expect, test } from "bun:test";
import {
  flatten,
  folderPaths,
  freePath,
  join,
  leafOf,
  nameProblem,
  parentOf,
  partAt,
  search,
  when,
  type ProjectEntry,
} from "./projects";

const part = (path: string, extra: Partial<ReturnType<typeof bare>> = {}) => ({
  ...bare(path),
  ...extra,
});

function bare(path: string) {
  const name = leafOf(path);
  return {
    kind: "part" as const,
    name,
    path,
    title: name.replace(/-/g, " "),
    bundle: true,
    thumbnail: false,
    tags: [] as string[],
    modified: null as number | null,
  };
}

const tree: ProjectEntry[] = [
  {
    kind: "folder",
    name: "Mounts",
    path: "Mounts",
    children: [
      { kind: "folder", name: "Legacy", path: "Mounts/Legacy", children: [part("Mounts/Legacy/old-plate")] },
      part("Mounts/motor-mount", { tags: ["nema17"] }),
    ],
  },
  part("bracket"),
];

describe("the tree", () => {
  test("flattens depth first, folders in order", () => {
    expect(flatten(tree).map((p) => p.path)).toEqual([
      "Mounts/Legacy/old-plate",
      "Mounts/motor-mount",
      "bracket",
    ]);
  });

  test("finds a part by its path", () => {
    expect(partAt(tree, "Mounts/motor-mount")?.title).toBe("motor mount");
    expect(partAt(tree, "Mounts")).toBeUndefined();
  });

  test("lists every folder", () => {
    expect(folderPaths(tree)).toEqual(["Mounts", "Mounts/Legacy"]);
  });
});

describe("search", () => {
  test("keeps a matching part and the folders above it", () => {
    const found = search(tree, "motor");
    expect(found).toHaveLength(1);
    expect(flatten(found).map((p) => p.path)).toEqual(["Mounts/motor-mount"]);
  });

  test("matches a tag", () => {
    expect(flatten(search(tree, "nema17")).map((p) => p.path)).toEqual([
      "Mounts/motor-mount",
    ]);
  });

  // Typing a folder's name should show you the folder, not empty it.
  test("a folder that matches by name keeps its contents", () => {
    expect(flatten(search(tree, "legacy")).map((p) => p.path)).toEqual([
      "Mounts/Legacy/old-plate",
    ]);
  });

  test("an empty query is the whole tree", () => {
    expect(search(tree, "  ")).toBe(tree);
  });

  test("nothing matching is nothing, not everything", () => {
    expect(search(tree, "zzz")).toEqual([]);
  });
});

describe("names", () => {
  test("accepts what the host accepts", () => {
    for (const name of ["bracket", "pipe-tee", "Motor Mount 2", "v_block"]) {
      expect(nameProblem(name)).toBeNull();
    }
  });

  // These mirror `safe()` in projects.rs; if that changes, this goes red.
  test("refuses what the host refuses", () => {
    for (const name of ["", "   ", "a/b", ".hidden", "bracket.js", "x.parcad", "a\\b", "a:b"]) {
      expect(nameProblem(name)).toBeString();
    }
  });

  test("joins and splits a path", () => {
    expect(join("Mounts", "bracket")).toBe("Mounts/bracket");
    expect(join("", "bracket")).toBe("bracket");
    expect(parentOf("Mounts/Legacy/old-plate")).toBe("Mounts/Legacy");
    expect(parentOf("bracket")).toBe("");
    expect(leafOf("Mounts/bracket")).toBe("bracket");
  });

  test("suffixes until the path is free", () => {
    const taken = ["bracket", "bracket-2"];
    expect(freePath(taken, "", "bracket")).toBe("bracket-3");
    expect(freePath(taken, "Mounts", "bracket")).toBe("Mounts/bracket");
  });
});

describe("when", () => {
  const now = 1_700_000_000_000;
  const ago = (seconds: number) => when(now / 1000 - seconds, now);

  test("reads as a person would say it", () => {
    expect(ago(5)).toBe("just now");
    expect(ago(600)).toBe("10 min ago");
    expect(ago(7200)).toBe("2 h ago");
    expect(ago(86_400)).toBe("yesterday");
    expect(ago(86_400 * 3)).toBe("3 days ago");
  });

  test("says nothing when the host could not read a time", () => {
    expect(when(null, now)).toBe("");
  });
});
