<script lang="ts">
  // The persistent "Needs review" header for the history view (ALPH-146).
  //
  // This file used to be an unmounted copy of the old pane chrome (branch
  // dropdown, sync button, tabs). App.svelte renders TopHeader for that now, so
  // the dead component was replaced with the one header the review loop was
  // missing: repo, marker..HEAD range, counts, +/−, and Mark reviewed / Undo.
  import { onDestroy } from "svelte";
  import type { CommitSummary, FileChange, RepoEntry, RepoStatus } from "../../types";
  import {
    addToast,
    activeDialog,
    addRepoDialogOpen,
    paletteOpen,
    settingsOpen,
    shortcutsHelpOpen,
  } from "../../stores/ui";
  import { matchShortcut } from "../../keys";
  import { applyReviewMarker, reviewEndShas } from "../../stores/review";
  import Icon from "../common/Icon.svelte";
  import {
    commitCountLabel,
    countArrivedSinceOpen,
    effectiveMarkerState,
    fileCountLabel,
    rangeDisplay,
    resolveMarkTarget,
    shaCommitsAgo,
    shortSha,
    totalsFromFiles,
  } from "../../review";

  interface Props {
    repo: RepoEntry | null;
    status: RepoStatus | null;
    isFocused: boolean;
    /** Newest-first commit page, used for the snapshot index and fallbacks. */
    commits: CommitSummary[];
    /** Files for the currently displayed scope (range or single commit). */
    scopeFiles: FileChange[];
    selectedCommit: CommitSummary | null;
    isRangeSelected: boolean;
    /** Notify the pane so it can refresh its captured snapshot after a mark. */
    onmarked?: () => void;
  }

  let {
    repo,
    status,
    isFocused,
    commits,
    scopeFiles,
    selectedCommit,
    isRangeSelected,
    onmarked,
  }: Props = $props();

  let saving = $state(false);
  let reviewLastN = $state(5);
  let lastUndo = $state<{ before: string | null; at: string } | null>(null);
  let undoTimer: ReturnType<typeof setTimeout> | null = null;

  let markerState = $derived(effectiveMarkerState(repo, status));
  let newCommitCount = $derived(status?.newCommitCount ?? 0);
  let isCaughtUp = $derived(markerState === "ok" && newCommitCount === 0);
  let hasReview = $derived(markerState === "ok" && newCommitCount > 0);

  // Snapshot end comes from the shared store so the palette and the header can
  // never disagree about what the user was shown (ALPH-146 follow-up).
  let reviewEndSha = $derived(repo ? ($reviewEndShas[repo.id] ?? null) : null);
  let markTarget = $derived(resolveMarkTarget(reviewEndSha, commits));
  let arrived = $derived(countArrivedSinceOpen(commits, reviewEndSha));

  let totals = $derived.by(() => {
    if (scopeFiles && scopeFiles.length > 0) return totalsFromFiles(scopeFiles);
    if (selectedCommit) {
      return {
        files: selectedCommit.filesChanged,
        additions: selectedCommit.additions,
        deletions: selectedCommit.deletions,
      };
    }
    return { files: 0, additions: 0, deletions: 0 };
  });

  let rangeLabel = $derived(
    markerState === "none"
      ? "no marker"
      : rangeDisplay(repo?.marker?.sha ?? null, null)
  );

  let newFlagCount = $derived(
    commits.filter((c) => c.isNew).reduce((n, c) => n + (c.flags?.length ?? 0), 0)
  );
  let flagTitle = $derived(
    commits
      .filter((c) => c.isNew)
      .flatMap((c) => c.flags ?? [])
      .map((f) => `${f.kind}: ${f.detail}`)
      .join("\n")
  );

  let canMark = $derived(!!repo && !!markTarget && (hasReview || markerState !== "ok"));

  let markTitle = $derived(
    markTarget
      ? `Mark reviewed up to ${shortSha(markTarget)}${
          arrived > 0
            ? ` — ${arrived} newer commit${arrived === 1 ? "" : "s"} stay unreviewed`
            : ""
        }`
      : "Mark reviewed"
  );

  function scheduleUndoClear() {
    if (undoTimer) clearTimeout(undoTimer);
    undoTimer = setTimeout(() => {
      lastUndo = null;
      undoTimer = null;
    }, 10000);
  }

  async function applyMarker(target: string, message: string) {
    if (!repo || saving) return;
    saving = true;
    try {
      const { before } = await applyReviewMarker(repo.id, target);
      lastUndo = { before, at: target };
      scheduleUndoClear();
      addToast({
        message,
        kind: "success",
        actionLabel: before ? "Undo" : undefined,
        action: before ? () => void handleUndo() : undefined,
      });
      onmarked?.();
    } catch (e: any) {
      addToast({ message: e.message || "Failed to update the reviewed marker", kind: "error" });
    } finally {
      saving = false;
    }
  }

  function handleMarkReviewed() {
    if (markTarget) void applyMarker(markTarget, "Marked as reviewed");
  }

  function handleResetMarker() {
    if (markTarget) void applyMarker(markTarget, "Marker reset to HEAD");
  }

  function handleStartFromN() {
    const target = shaCommitsAgo(commits, reviewLastN);
    if (!target) {
      addToast({ message: "Load more history before reviewing from here", kind: "info" });
      return;
    }
    void applyMarker(target, `Reviewing from ${reviewLastN} commits ago`);
  }

  async function handleUndo() {
    const before = lastUndo?.before;
    if (!repo || !before) return;
    saving = true;
    try {
      await applyReviewMarker(repo.id, before);
      lastUndo = null;
      if (undoTimer) {
        clearTimeout(undoTimer);
        undoTimer = null;
      }
      addToast({ message: "Marker restored", kind: "success" });
      onmarked?.();
    } catch (e: any) {
      addToast({ message: e.message || "Failed to restore the marker", kind: "error" });
    } finally {
      saving = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (!isFocused || !repo) return;
    if ($paletteOpen || $settingsOpen || $shortcutsHelpOpen || $addRepoDialogOpen || $activeDialog) {
      return;
    }
    if (e.defaultPrevented) return;
    const target = e.target as HTMLElement | null;
    if (
      target &&
      (target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.isContentEditable)
    ) {
      return;
    }
    // Never steal keys from the embedded terminal.
    if (target?.closest?.(".xterm, .terminal-drawer")) return;
    if (!matchShortcut(e, "r")) return;
    if (!canMark) return;
    e.preventDefault();
    handleMarkReviewed();
  }

  onDestroy(() => {
    if (undoTimer) clearTimeout(undoTimer);
  });
</script>

<svelte:window onkeydown={handleKeydown} />

{#if repo}
  <div
    class="review-header"
    class:is-warning={markerState === "missing" || markerState === "diverged"}
    class:is-caught-up={isCaughtUp}
    role="region"
    aria-label="Needs review"
  >
    <div class="rh-row">
      <div class="rh-info">
        <span class="rh-repo" title={repo.path}>{repo.name}</span>
        <span class="rh-range font-mono" title="Review range">{rangeLabel}</span>

        {#if hasReview}
          <span class="rh-sep" aria-hidden="true">·</span>
          <span class="rh-count">{commitCountLabel(newCommitCount)}</span>
          <span class="rh-sep" aria-hidden="true">·</span>
          <span class="rh-files">{fileCountLabel(totals.files)}</span>
          <span class="rh-stats">
            <span class="rh-add">+{totals.additions}</span>
            <span class="rh-del">−{totals.deletions}</span>
          </span>
          {#if selectedCommit && !isRangeSelected}
            <span class="rh-sep" aria-hidden="true">·</span>
            <span class="rh-viewing" title={selectedCommit.subject}>
              viewing {selectedCommit.short}
            </span>
          {/if}
          {#if arrived > 0}
            <span
              class="rh-arrived"
              title="These commits landed after you opened this review and stay unreviewed"
            >
              {arrived} arrived — still unreviewed
            </span>
          {/if}
          {#if newFlagCount > 0}
            <span class="rh-flags" title={flagTitle}>
              <Icon name="flag" size={12} />
              {newFlagCount}
            </span>
          {/if}
        {:else if markerState === "none"}
          <span class="rh-sep" aria-hidden="true">·</span>
          <span class="rh-note">No reviewed marker yet</span>
        {:else if markerState === "missing"}
          <span class="rh-sep" aria-hidden="true">·</span>
          <span class="rh-note">The reviewed marker no longer exists in this repo</span>
        {:else if markerState === "diverged"}
          <span class="rh-sep" aria-hidden="true">·</span>
          <span class="rh-note">History was rewritten since you last reviewed</span>
        {:else}
          <span class="rh-sep" aria-hidden="true">·</span>
          <span class="rh-note rh-caught-up">
            <Icon name="check" size={13} />
            All caught up — reviewed up to {shortSha(repo.marker?.sha)}
          </span>
        {/if}
      </div>

      <div class="rh-actions">
        {#if markerState !== "ok"}
          <label class="rh-last">
            <span class="rh-last-label">Review last</span>
            <input
              type="number"
              min="1"
              max="200"
              bind:value={reviewLastN}
              aria-label="Number of recent commits to treat as unreviewed"
            />
            <span class="rh-last-label">commits</span>
          </label>
          <button
            type="button"
            class="rh-btn"
            onclick={handleStartFromN}
            disabled={saving || commits.length === 0}
          >
            Start reviewing
          </button>
        {/if}

        {#if markerState === "ok" && newCommitCount > 0}
          <button
            type="button"
            class="rh-primary"
            onclick={handleMarkReviewed}
            disabled={saving}
            title={markTitle}
          >
            <Icon name="check" size={14} />
            {saving ? "Saving…" : "Mark reviewed"}
            <kbd class="rh-kbd">R</kbd>
          </button>
        {:else if markerState === "missing"}
          <button
            type="button"
            class="rh-primary"
            onclick={handleResetMarker}
            disabled={saving || !markTarget}
            title={markTitle}
          >
            {saving ? "Saving…" : "Reset marker"}
          </button>
        {:else if markerState === "diverged"}
          <button
            type="button"
            class="rh-primary"
            onclick={handleResetMarker}
            disabled={saving || !markTarget}
            title={markTitle}
          >
            {saving ? "Saving…" : "Re-mark from HEAD"}
          </button>
        {:else if markerState === "none"}
          <button
            type="button"
            class="rh-primary"
            onclick={handleMarkReviewed}
            disabled={saving || !markTarget}
            title="Mark everything up to HEAD as reviewed"
          >
            {saving ? "Saving…" : "Mark reviewed"}
          </button>
        {/if}
      </div>
    </div>

    {#if lastUndo}
      <div class="rh-undo" role="status">
        <span>Marker set to {shortSha(lastUndo.at)}.</span>
        {#if lastUndo.before}
          <button type="button" class="rh-undo-btn" onclick={handleUndo} disabled={saving}>
            Undo
          </button>
        {:else}
          <span class="rh-undo-note">No previous marker to restore.</span>
        {/if}
      </div>
    {/if}
  </div>
{/if}

<style>
  .review-header {
    flex-shrink: 0;
    background-color: var(--accent-bg);
    border-bottom: 1px solid var(--border);
    border-left: 3px solid var(--accent);
    color: var(--text);
    user-select: none;
  }

  .review-header.is-warning {
    background-color: var(--warning-bg);
    border-left-color: var(--warning);
  }

  .review-header.is-caught-up {
    background-color: var(--bg-subtle);
    border-left-color: transparent;
  }

  .rh-row {
    display: flex;
    align-items: center;
    gap: var(--s-3);
    padding: var(--s-2) var(--s-3);
    min-height: 36px;
    flex-wrap: wrap;
  }

  .rh-info {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    min-width: 0;
    flex: 1;
    flex-wrap: wrap;
    font-size: 12px;
    line-height: 16px;
  }

  .rh-repo {
    font-size: 13px;
    font-weight: 600;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 220px;
  }

  .rh-range {
    font-size: 11px;
    padding: 1px 6px;
    border-radius: var(--r-sm);
    background-color: var(--bg-muted);
    color: var(--text-muted);
    white-space: nowrap;
  }

  .rh-sep {
    color: var(--text-faint);
  }

  .rh-count {
    font-weight: 600;
    color: var(--text);
  }

  .rh-files,
  .rh-viewing {
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .rh-stats {
    display: inline-flex;
    gap: var(--s-1);
    font-variant-numeric: tabular-nums;
  }

  .rh-add {
    color: var(--success);
  }

  .rh-del {
    color: var(--danger);
  }

  .rh-arrived {
    font-size: 11px;
    color: var(--warning);
    white-space: nowrap;
  }

  .rh-flags {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 1px 6px;
    border-radius: var(--r-sm);
    background-color: var(--bg-muted);
    color: var(--warning);
    font-size: 11px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  .rh-note {
    color: var(--text-muted);
  }

  .rh-caught-up {
    display: inline-flex;
    align-items: center;
    gap: var(--s-1);
    color: var(--text-faint);
  }

  .rh-actions {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    flex-shrink: 0;
  }

  .rh-last {
    display: inline-flex;
    align-items: center;
    gap: var(--s-1);
    color: var(--text-muted);
    font-size: 11px;
    cursor: text;
  }

  .rh-last input {
    width: 46px;
    height: 24px;
    padding: 0 4px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background-color: var(--bg);
    color: var(--text);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }

  .rh-last input:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .rh-btn {
    height: 26px;
    padding: 0 var(--s-2);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    background-color: var(--bg);
    color: var(--text);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
  }

  .rh-btn:hover:not(:disabled) {
    background-color: var(--bg-muted);
  }

  .rh-primary {
    display: inline-flex;
    align-items: center;
    gap: var(--s-1);
    height: 28px;
    padding: 0 var(--s-3);
    border: 1px solid transparent;
    border-radius: var(--r-md);
    background-color: var(--accent);
    color: var(--accent-fg);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }

  .rh-primary:hover:not(:disabled) {
    filter: brightness(1.06);
  }

  .rh-primary:disabled,
  .rh-btn:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .rh-primary:focus-visible,
  .rh-btn:focus-visible,
  .rh-undo-btn:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .rh-kbd {
    font-family: var(--font-mono);
    font-size: 10px;
    padding: 0 4px;
    border-radius: var(--r-sm);
    background-color: var(--bg);
    color: var(--text-muted);
    line-height: 15px;
  }

  .rh-undo {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    padding: var(--s-1) var(--s-3);
    border-top: 1px solid var(--border-subtle);
    font-size: 11px;
    color: var(--text-muted);
  }

  .rh-undo-btn {
    height: 22px;
    padding: 0 var(--s-2);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background-color: var(--bg);
    color: var(--accent);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
  }

  .rh-undo-btn:hover:not(:disabled) {
    background-color: var(--bg-muted);
  }

  .rh-undo-note {
    color: var(--text-faint);
  }

  @media (prefers-reduced-motion: reduce) {
    .rh-primary:hover:not(:disabled) {
      filter: none;
    }
  }
</style>
