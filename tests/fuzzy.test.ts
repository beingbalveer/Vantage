import { describe, it, expect } from "vitest";
import { fuzzyScore, rankPaths } from "../src/lib/util/fuzzy";

describe("fuzzyScore", () => {
  it("matches empty query", () => {
    const res = fuzzyScore("", "api-server");
    expect(res).not.toBeNull();
    expect(res?.score).toBe(1);
  });

  it("fails when query longer than text", () => {
    const res = fuzzyScore("longqueryhere", "short");
    expect(res).toBeNull();
  });

  it("matches subsequence with word-start bonuses", () => {
    const res = fuzzyScore("api", "api-server");
    expect(res).not.toBeNull();
    expect(res?.indices).toEqual([0, 1, 2]);
  });

  it("matches camelCase word starts", () => {
    const res = fuzzyScore("as", "apiServer");
    expect(res).not.toBeNull();
    expect(res?.indices).toEqual([0, 3]);
  });

  it("returns null on non-matching string", () => {
    const res = fuzzyScore("xyz", "api-server");
    expect(res).toBeNull();
  });
});

describe("rankPaths", () => {
  const paths = [
    "src/lib/components/Pane/FileTabs.svelte",
    "src/lib/domain/stores/fileTabs.ts",
    "README.md",
    "docs/file-tabs.md",
  ];

  it("returns every path for an empty query", () => {
    expect(rankPaths("  ", paths)).toEqual(paths);
  });

  it("keeps only fuzzy matches and ranks the best first", () => {
    const ranked = rankPaths("fileTabs.svelte", paths);
    expect(ranked).toEqual(["src/lib/components/Pane/FileTabs.svelte"]);

    const stores = rankPaths("stores", paths);
    expect(stores).toEqual(["src/lib/domain/stores/fileTabs.ts"]);
  });

  it("returns an empty list when nothing matches", () => {
    expect(rankPaths("zzzz", paths)).toEqual([]);
  });
});
