import { describe, it, expect, vi, beforeEach } from "vitest";
import { get } from "svelte/store";
import {
  repos,
  statuses,
  sortedRepos,
  reorderRepos,
  repoSelections,
  setRepoSelection,
  getRepoSelection,
} from "../src/lib/stores/repos";
import type { RepoEntry, RepoStatus } from "../src/lib/types";
import * as api from "../src/lib/api";

vi.mock("../src/lib/api", () => ({
  repoReorder: vi.fn().mockResolvedValue(undefined),
  repoList: vi.fn().mockResolvedValue([]),
  repoRefreshAll: vi.fn().mockResolvedValue([]),
  repoStatus: vi.fn().mockResolvedValue({}),
  onRepoStatus: vi.fn().mockResolvedValue(() => {}),
  onHeadChanged: vi.fn().mockResolvedValue(() => {}),
  markerSetToHead: vi.fn().mockResolvedValue(undefined),
}));

describe("repos store & ordering", () => {
  beforeEach(() => {
    repos.set([]);
    statuses.set({});
    repoSelections.set({});
    vi.clearAllMocks();
  });

  it("sortedRepos respects manual order within unpinned repos", () => {
    const list: RepoEntry[] = [
      { id: "1", name: "Repo A", path: "/a", pinned: false, order: 2, marker: null, notes: [] },
      { id: "2", name: "Repo B", path: "/b", pinned: false, order: 0, marker: null, notes: [] },
      { id: "3", name: "Repo C", path: "/c", pinned: false, order: 1, marker: null, notes: [] },
    ];
    repos.set(list);

    const sorted = get(sortedRepos);
    expect(sorted.map((r) => r.id)).toEqual(["2", "3", "1"]);
  });

  it("sortedRepos keeps pinned repos at the top ordered by order", () => {
    const list: RepoEntry[] = [
      { id: "1", name: "Repo A", path: "/a", pinned: false, order: 0, marker: null, notes: [] },
      { id: "2", name: "Repo B", path: "/b", pinned: true, order: 1, marker: null, notes: [] },
      { id: "3", name: "Repo C", path: "/c", pinned: true, order: 0, marker: null, notes: [] },
    ];
    repos.set(list);

    const sorted = get(sortedRepos);
    expect(sorted.map((r) => r.id)).toEqual(["3", "2", "1"]);
  });

  it("reorderRepos updates local store and invokes api.repoReorder", async () => {
    const list: RepoEntry[] = [
      { id: "1", name: "Repo A", path: "/a", pinned: false, order: 0, marker: null, notes: [] },
      { id: "2", name: "Repo B", path: "/b", pinned: false, order: 1, marker: null, notes: [] },
    ];
    repos.set(list);

    // Swap items
    const reordered = [list[1], list[0]];
    await reorderRepos(reordered);

    const updated = get(repos);
    expect(updated[0].id).toBe("2");
    expect(updated[0].order).toBe(0);
    expect(updated[1].id).toBe("1");
    expect(updated[1].order).toBe(1);

    expect(api.repoReorder).toHaveBeenCalledWith([
      { id: "2", order: 0, pinned: false },
      { id: "1", order: 1, pinned: false },
    ]);
  });
});

describe("repo selection store & switching logic", () => {
  beforeEach(() => {
    repoSelections.set({});
  });

  it("records and retrieves repo selection per repo", () => {
    setRepoSelection("repo-1", { type: "commit", sha: "abc1234" });
    setRepoSelection("repo-2", { type: "uncommitted" });

    expect(getRepoSelection("repo-1")).toEqual({ type: "commit", sha: "abc1234" });
    expect(getRepoSelection("repo-2")).toEqual({ type: "uncommitted" });
    expect(getRepoSelection("repo-3")).toBeUndefined();
  });

  it("preserves selection when switching back to a repo", () => {
    // Repo A was on commit 123
    setRepoSelection("repo-a", { type: "commit", sha: "commit-123" });
    // Switch to Repo B and select uncommitted
    setRepoSelection("repo-b", { type: "uncommitted" });

    // Switching back to Repo A retrieves commit 123
    const selA = getRepoSelection("repo-a");
    expect(selA).toEqual({ type: "commit", sha: "commit-123" });

    // Switching back to Repo B retrieves uncommitted
    const selB = getRepoSelection("repo-b");
    expect(selB).toEqual({ type: "uncommitted" });
  });
});
