// Pure helpers for the "needs review" range header (ALPH-146).
//
// The review loop is: glance at the sidebar, open a repo, skim the combined
// marker..HEAD diff, then mark reviewed. These helpers keep the header's
// labelling and the marker target logic testable without a webview.

import type {
  CommitSummary,
  FileChange,
  MarkerState,
  RepoEntry,
  RepoStatus,
} from "./types";

/** Short (7-char) SHA for compact display; tolerant of null/empty. */
export function shortSha(sha: string | null | undefined, len = 7): string {
  if (!sha) return "";
  return sha.length > len ? sha.slice(0, len) : sha;
}

export interface FileTotals {
  files: number;
  additions: number;
  deletions: number;
}

/** Net file count and +/− across a file list (a scope's file list is already net). */
export function totalsFromFiles(files: FileChange[] | null | undefined): FileTotals {
  const list = files ?? [];
  let additions = 0;
  let deletions = 0;
  for (const file of list) {
    additions += file.additions || 0;
    deletions += file.deletions || 0;
  }
  return { files: list.length, additions, deletions };
}

/**
 * Display label for the review range. `to` falls back to the literal "HEAD"
 * when no snapshot end has been captured yet.
 */
export function rangeDisplay(
  from: string | null | undefined,
  to: string | null | undefined
): string {
  const start = from ? shortSha(from) : "start";
  const end = to ? shortSha(to) : "HEAD";
  return `${start}..${end}`;
}

/**
 * Marker state with a defensive fallback: when status hasn't loaded yet, a
 * stored marker means "ok", otherwise the repo has never been reviewed.
 */
export function effectiveMarkerState(
  repo: RepoEntry | null | undefined,
  status: RepoStatus | null | undefined
): MarkerState {
  if (status?.markerState) return status.markerState;
  if (repo?.marker?.sha) return "ok";
  return "none";
}

export function commitCountLabel(count: number): string {
  return `${count} new commit${count === 1 ? "" : "s"}`;
}

export function fileCountLabel(count: number): string {
  return `${count} file${count === 1 ? "" : "s"}`;
}

/**
 * The SHA that should become the new marker to leave the last `n` commits
 * unreviewed. `commits` is newest-first, so `commits[0]` is HEAD. Clamps to
 * the oldest loaded commit when `n` exceeds the page.
 */
export function shaCommitsAgo(
  commits: CommitSummary[],
  n: number
): string | null {
  if (commits.length === 0) return null;
  const safe = Number.isFinite(n) ? Math.max(0, Math.floor(n)) : 0;
  const index = Math.min(safe, commits.length - 1);
  return commits[index]?.sha ?? null;
}

/**
 * The END SHA of a freshly loaded review page. `commits` is newest-first, so
 * its head is the snapshot end. This is the single capture rule used by the
 * review store; both the header and the palette consume the stored result.
 */
export function reviewSnapshotEnd(commits: CommitSummary[]): string | null {
  return commits[0]?.sha ?? null;
}

/**
 * Marker target for "Mark reviewed": prefer the snapshot the user was shown,
 * and only fall back to the newest loaded commit when no snapshot was captured
 * yet. Never consults a live HEAD.
 */
export function resolveMarkTarget(
  snapshotEnd: string | null | undefined,
  commits: CommitSummary[]
): string | null {
  return snapshotEnd ?? reviewSnapshotEnd(commits);
}

/**
 * How many commits landed on top of the captured review snapshot while the
 * user was reviewing. `commits` is newest-first; the snapshot end's index is
 * exactly the number of newer commits. Returns 0 when the end isn't in this
 * page (can't tell) or is already the newest.
 */
export function countArrivedSinceOpen(
  commits: CommitSummary[],
  endSha: string | null | undefined
): number {
  if (!endSha) return 0;
  const index = commits.findIndex((c) => c.sha === endSha);
  return index > 0 ? index : 0;
}
