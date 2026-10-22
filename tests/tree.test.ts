import { describe, it, expect } from "vitest";
import {
  buildFileTree,
  filterFileTree,
  flattenVisibleTree,
  collectAllFolderIds,
  getAncestorFolderIds,
} from "../src/lib/tree";

describe("buildFileTree", () => {
  it("builds a nested tree with directories sorted before files", () => {
    const paths = [
      "README.md",
      "src/index.ts",
      "package.json",
      "src/utils/math.ts",
      "src/utils/string.ts",
      "docs/intro.md",
    ];

    const tree = buildFileTree(paths, { compactSingleChildDirs: false });

    // Top level: docs (dir), src (dir), package.json (file), README.md (file)
    expect(tree.map((n) => n.name)).toEqual(["docs", "src", "package.json", "README.md"]);

    const srcDir = tree.find((n) => n.name === "src")!;
    expect(srcDir.isDir).toBe(true);
    expect(srcDir.children.map((c) => c.name)).toEqual(["utils", "index.ts"]);

    const utilsDir = srcDir.children.find((c) => c.name === "utils")!;
    expect(utilsDir.isDir).toBe(true);
    expect(utilsDir.children.map((c) => c.name)).toEqual(["math.ts", "string.ts"]);
  });

  it("compacts single-child directory chains VS Code-style", () => {
    const paths = [
      "src/lib/components/Pane/FileTree.svelte",
      "package.json",
    ];

    const tree = buildFileTree(paths, { compactSingleChildDirs: true });

    // Since src only has lib, lib only has components, components only has Pane,
    // they should be compacted into "src/lib/components/Pane"
    expect(tree[0].name).toBe("src/lib/components/Pane");
    expect(tree[0].fullPath).toBe("src/lib/components/Pane");
    expect(tree[0].children[0].name).toBe("FileTree.svelte");
    expect(tree[1].name).toBe("package.json");
  });

  it("handles empty or root files gracefully", () => {
    expect(buildFileTree([])).toEqual([]);
    expect(buildFileTree(["", "  "])).toEqual([]);
  });
});

describe("filterFileTree", () => {
  it("filters files matching query and returns autoExpanded folders", () => {
    const paths = [
      "src/components/Header.svelte",
      "src/components/Footer.svelte",
      "src/utils/math.ts",
      "README.md",
    ];
    const tree = buildFileTree(paths, { compactSingleChildDirs: false });

    const result = filterFileTree(tree, "head");
    expect(result.matchCount).toBe(1);
    expect(result.filtered.length).toBe(1); // src

    const src = result.filtered[0];
    expect(src.name).toBe("src");
    expect(result.autoExpanded.has(src.id)).toBe(true);

    const components = src.children[0];
    expect(components.name).toBe("components");
    expect(result.autoExpanded.has(components.id)).toBe(true);
    expect(components.children.map((c) => c.name)).toEqual(["Header.svelte"]);
  });

  it("returns original tree on empty filter", () => {
    const paths = ["src/index.ts", "README.md"];
    const tree = buildFileTree(paths);
    const result = filterFileTree(tree, "");
    expect(result.filtered).toBe(tree);
    expect(result.matchCount).toBe(0);
  });
});

describe("flattenVisibleTree", () => {
  it("flattens tree according to expanded directories", () => {
    const paths = [
      "src/index.ts",
      "src/utils.ts",
      "README.md",
    ];
    const tree = buildFileTree(paths, { compactSingleChildDirs: false });
    const expanded = new Set<string>();

    // When nothing is expanded: only top-level items visible
    let visible = flattenVisibleTree(tree, expanded);
    expect(visible.map((v) => v.node.name)).toEqual(["src", "README.md"]);
    expect(visible[0].isOpen).toBe(false);

    // Expand src
    expanded.add("dir:src");
    visible = flattenVisibleTree(tree, expanded);
    expect(visible.map((v) => v.node.name)).toEqual(["src", "index.ts", "utils.ts", "README.md"]);
    expect(visible[0].isOpen).toBe(true);
    expect(visible[1].depth).toBe(1);
  });
});

describe("collectAllFolderIds and getAncestorFolderIds", () => {
  it("collects all folder ids", () => {
    const paths = ["a/b/c/file.txt", "x/y/other.txt"];
    const tree = buildFileTree(paths, { compactSingleChildDirs: false });
    const ids = collectAllFolderIds(tree);
    expect(ids.has("dir:a")).toBe(true);
    expect(ids.has("dir:a/b")).toBe(true);
    expect(ids.has("dir:a/b/c")).toBe(true);
    expect(ids.has("dir:x")).toBe(true);
  });

  it("gets ancestor folder ids for a path", () => {
    const ancestors = getAncestorFolderIds("src/lib/components/Pane/FileTree.svelte");
    expect(ancestors).toEqual(
      new Set([
        "dir:src",
        "dir:src/lib",
        "dir:src/lib/components",
        "dir:src/lib/components/Pane",
      ])
    );
  });
});
