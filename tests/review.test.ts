import { describe, it, expect } from "vitest";
import {
  shortSha,
  totalsFromFiles,
  rangeDisplay,
  effectiveMarkerState,
  commitCountLabel,
  fileCountLabel,
  shaCommitsAgo,
  countArrivedSinceOpen,
} from "../src/lib/review";
import type { CommitSummary, FileChange, RepoEntry } from "../src/lib/types";

function commit(sha: string, over: Partial<CommitSummary> = {}): CommitSummary {
  return {
    sha,
    short: sha.slice(0, 7),
    authorName: "Codex",
    authorEmail: "codex@example.com",
    date: "2026-10-19T10:00:00Z",
    subject: `commit ${sha}`,
    body: "",
    parents: [],
    filesChanged: 1,
    additions: 1,
    deletions: 0,
    flags: [],
    isNew: true,
    ...over,
  };
}

function file(additions: number, deletions: number): FileChange {
  return {
    path: "src/x.ts",
    oldPath: null,
    status: "modified",
    additions,
    deletions,
    binary: false,
    oldMode: null,
    newMode: null,
  };
}

describe("review helpers", () => {
  it("shortens SHAs and tolerates missing values", () => {
    expect(shortSha("0123456789abcdef")).toBe("0123456");
    expect(shortSha("abc")).toBe("abc");
    expect(shortSha(null)).toBe("");
    expect(shortSha(undefined)).toBe("");
  });

  it("sums net file totals from a file list", () => {
    expect(totalsFromFiles([file(10, 2), file(3, 4)])).toEqual({
      files: 2,
      additions: 13,
      deletions: 6,
    });
    expect(totalsFromFiles(null)).toEqual({ files: 0, additions: 0, deletions: 0 });
    expect(totalsFromFiles([])).toEqual({ files: 0, additions: 0, deletions: 0 });
  });

  it("labels the review range, falling back to HEAD", () => {
    expect(rangeDisplay("0123456789", "abcdef0123")).toBe("0123456..abcdef0");
    expect(rangeDisplay("0123456789", null)).toBe("0123456..HEAD");
    expect(rangeDisplay(null, "abcdef0123")).toBe("start..abcdef0");
  });

  it("derives marker state defensively when status is absent", () => {
    expect(effectiveMarkerState(null, null)).toBe("none");
    expect(effectiveMarkerState({ marker: null } as RepoEntry, null)).toBe("none");
    expect(
      effectiveMarkerState({ marker: { sha: "abc", setAt: "", branch: null } } as RepoEntry, null)
    ).toBe("ok");
    // Status always wins when present.
    expect(
      effectiveMarkerState({ marker: { sha: "abc", setAt: "", branch: null } } as RepoEntry, {
        markerState: "diverged",
      } as any)
    ).toBe("diverged");
  });

  it("pluralizes commit and file counts", () => {
    expect(commitCountLabel(1)).toBe("1 new commit");
    expect(commitCountLabel(7)).toBe("7 new commits");
    expect(fileCountLabel(1)).toBe("1 file");
    expect(fileCountLabel(3)).toBe("3 files");
  });

  it("picks the marker SHA N commits back from HEAD", () => {
    const commits = [commit("a"), commit("b"), commit("c"), commit("d")];
    expect(shaCommitsAgo(commits, 0)).toBe("a");
    expect(shaCommitsAgo(commits, 2)).toBe("c");
    // Clamps to the oldest loaded commit instead of returning nothing.
    expect(shaCommitsAgo(commits, 99)).toBe("d");
    expect(shaCommitsAgo([], 3)).toBeNull();
  });

  it("counts commits that arrived after the review snapshot opened", () => {
    const commits = [commit("new1"), commit("new2"), commit("snapshot"), commit("old")];
    expect(countArrivedSinceOpen(commits, "snapshot")).toBe(2);
    expect(countArrivedSinceOpen(commits, "new1")).toBe(0);
    // Snapshot not in the loaded page: cannot tell, so report none.
    expect(countArrivedSinceOpen(commits, "missing")).toBe(0);
    expect(countArrivedSinceOpen(commits, null)).toBe(0);
  });
});
