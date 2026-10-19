import { get, writable } from "svelte/store";
import type { CommitSummary } from "../types";
import * as api from "../api";
import { reviewSnapshotEnd } from "../review";
import { loadRepos, repos } from "./repos";

/**
 * Per-repo END SHA of the history snapshot the user was actually shown when the
 * review opened. This is the single source of truth for "Mark reviewed": the
 * range header and the command palette both read it, so the two entry points
 * can never drift. Commit captures land here via `captureReviewEnd`.
 *
 * Deliberately independent of live `HEAD`: getting the value from a fresh
 * `git rev-parse HEAD` is exactly the bug this indirection prevents.
 */
export const reviewEndShas = writable<Record<string, string>>({});

export function setReviewEndSha(
  repoId: string,
  sha: string | null | undefined
): void {
  reviewEndShas.update((map) => {
    const next = { ...map };
    if (sha) next[repoId] = sha;
    else delete next[repoId];
    return next;
  });
}

export function clearReviewEndSha(repoId: string): void {
  setReviewEndSha(repoId, null);
}

/** The captured snapshot end for a repo, or null when it has never been opened. */
export function getReviewEndSha(repoId: string | null | undefined): string | null {
  if (!repoId) return null;
  return get(reviewEndShas)[repoId] ?? null;
}

/**
 * Record the END SHA of a freshly loaded review page. `commits` is newest-first,
 * so its head is the snapshot end. Called by `Pane` when a review opens.
 */
export function captureReviewEnd(
  repoId: string,
  commits: CommitSummary[]
): string | null {
  const end = reviewSnapshotEnd(commits);
  setReviewEndSha(repoId, end);
  return end;
}

export interface MarkerChange {
  /** Marker SHA before the change (null when the repo had no marker). */
  before: string | null;
  /** Marker SHA that was written. */
  target: string;
  /** Snapshot end after the change, advanced to the current HEAD. */
  nextEnd: string | null;
}

/**
 * The one place that writes a reviewed marker. Both the header and the palette
 * call it, so a mark is always `markerSet(repoId, target)` where `target` came
 * from the captured snapshot. After a successful write the snapshot is advanced
 * to the current HEAD: the review that just finished is done, and whatever is
 * left unreviewed (commits that arrived, or a "review last N" boundary) becomes
 * the next review.
 *
 * Never falls back to a live HEAD to *choose* the target — that choice belongs
 * to the caller via `captureReviewEnd` / `getReviewEndSha`.
 */
export async function applyReviewMarker(
  repoId: string,
  target: string
): Promise<MarkerChange> {
  const before = get(repos).find((r) => r.id === repoId)?.marker?.sha ?? null;
  await api.markerSet(repoId, target);

  let nextEnd: string | null = null;
  try {
    const head = await api.logPage(repoId, 0, 1);
    nextEnd = reviewSnapshotEnd(head);
    setReviewEndSha(repoId, nextEnd);
  } catch {
    // Snapshot stays as-is; marking still succeeded.
  }

  await loadRepos();
  return { before, target, nextEnd };
}
