<script lang="ts">
  import type { CommitSummary } from "../../types";
  import * as api from "../../api";
  import { activeDialog, closeDialog, addToast } from "../../stores/ui";
  import { statuses, refreshSingleStatus, loadRepos } from "../../stores/repos";
  import { formatRelativeTime, formatFullTimestamp } from "../../format";
  import { Modal, Button } from "../common";

  interface Props {
    open?: boolean;
    repoId?: string;
    sha?: string;
    commit?: CommitSummary | null;
    onclose?: () => void;
    onsuccess?: (commit: CommitSummary) => void;
  }

  let {
    open,
    repoId,
    sha,
    commit,
    onclose,
    onsuccess,
  }: Props = $props();

  let isStoreActive = $derived($activeDialog?.type === "revert");
  let isOpen = $derived(open !== undefined ? open : isStoreActive);

  let currentRepoId = $derived(
    repoId || (isStoreActive ? $activeDialog?.repoId : "") || ""
  );

  let currentSha = $derived(
    sha ||
      (isStoreActive
        ? $activeDialog?.data?.sha || $activeDialog?.data?.commit?.sha
        : "") ||
      ""
  );

  let currentCommit = $state<CommitSummary | null>(null);

  $effect(() => {
    if (commit !== undefined) {
      currentCommit = commit;
    } else if (isStoreActive && $activeDialog?.data?.commit) {
      currentCommit = $activeDialog.data.commit;
    } else {
      currentCommit = null;
    }
  });

  let isReverting = $state(false);
  let errorMessage = $state<string | null>(null);

  // Check dirty state
  let isDirty = $derived.by(() => {
    if (!currentRepoId) return false;
    const status = $statuses[currentRepoId];
    return Boolean(status && status.dirtyCount > 0);
  });

  function handleClose() {
    errorMessage = null;
    isReverting = false;
    onclose?.();
    if (isStoreActive) {
      closeDialog();
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      handleClose();
    }
  }

  async function handleRevert() {
    if (!currentRepoId || !currentSha || isDirty || isReverting) return;

    isReverting = true;
    errorMessage = null;

    try {
      const res = await api.revert(currentRepoId, currentSha);
      const shortSha = res.short || currentSha.slice(0, 7);
      addToast({
        message: `Created revert commit ${shortSha}`,
        kind: "success",
      });

      await refreshSingleStatus(currentRepoId);
      await loadRepos();

      onsuccess?.(res);
      handleClose();
    } catch (e: any) {
      errorMessage = e.message || "Failed to revert commit. A merge conflict may have occurred.";
      const msg = errorMessage || "Failed to revert commit";
      addToast({
        message: msg,
        kind: "error",
      });
    } finally {
      isReverting = false;
    }
  }
</script>

{#if isOpen}
  <Modal
    open={isOpen}
    title="Revert commit"
    width="480px"
    closeOnEscape={!isReverting}
    closeOnOutsideClick={!isReverting}
    onclose={handleClose}
  >
    <div class="revert-content">
      <p class="text-base text-muted">
        This will create a new commit that inverts the changes made in the selected commit.
      </p>

      <!-- Commit Preview Box -->
      <div class="commit-preview">
        {#if currentCommit}
          <div class="commit-subject text-strong text-base">{currentCommit.subject}</div>
          <div class="commit-meta text-xs">
            <span class="commit-sha font-mono text-muted">{currentCommit.short || currentCommit.sha.slice(0, 7)}</span>
            <span class="commit-author text-muted">by {currentCommit.authorName}</span>
            <span class="commit-date text-muted font-mono" title={formatFullTimestamp(currentCommit.date)}>
              {formatRelativeTime(currentCommit.date)}
            </span>
          </div>
          {#if currentCommit.filesChanged > 0}
            <div class="commit-stats text-xs font-mono">
              <span class="stat-files">{currentCommit.filesChanged} file{currentCommit.filesChanged === 1 ? "" : "s"}</span>
              <span class="stat-add">+{currentCommit.additions}</span>
              <span class="stat-del">−{currentCommit.deletions}</span>
            </div>
          {/if}
        {:else}
          <div class="commit-subject text-strong text-base">Commit</div>
          <div class="commit-meta text-xs">
            <span class="commit-sha font-mono text-muted">{currentSha.slice(0, 7)}</span>
          </div>
        {/if}
      </div>

      <!-- Dirty Working Tree Warning Banner -->
      {#if isDirty}
        <div class="warning-banner" role="alert">
          <span class="warning-icon">⚠</span>
          <div class="warning-content text-sm">
            <span class="warning-title text-strong">Working tree has uncommitted changes.</span>
            <p class="warning-desc">Commit or discard your changes first before reverting.</p>
          </div>
        </div>
      {/if}

      {#if errorMessage}
        <div class="error-banner text-sm" role="alert">
          {errorMessage}
        </div>
      {/if}
    </div>

    {#snippet footer()}
      <Button
        variant="default"
        onclick={handleClose}
        disabled={isReverting}
      >
        Cancel
      </Button>
      <Button
        variant="primary"
        onclick={handleRevert}
        loading={isReverting}
        disabled={isDirty || isReverting || !currentSha}
        title={isDirty ? "Commit or discard your changes first" : "Create revert commit"}
      >
        {isReverting ? "Reverting…" : "Create revert commit"}
      </Button>
    {/snippet}
  </Modal>
{/if}

<style>
  .revert-content {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .commit-preview {
    background-color: var(--bg-subtle);
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-md);
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .commit-subject {
    color: var(--text);
    word-break: break-word;
  }

  .commit-meta {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .commit-sha {
    background-color: var(--bg);
    padding: 1px 4px;
    border-radius: var(--r-sm);
    border: 1px solid var(--border-subtle);
  }

  .commit-stats {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-top: 4px;
    border-top: 1px solid var(--border-subtle);
  }
  .stat-files {
    color: var(--text-muted);
  }
  .stat-add {
    color: var(--success);
  }
  .stat-del {
    color: var(--danger);
  }

  .warning-banner {
    display: flex;
    gap: 10px;
    padding: 10px 12px;
    background-color: var(--warning-bg, rgba(210, 153, 34, 0.12));
    border: 1px solid var(--warning, #d29922);
    border-radius: var(--r-md);
    align-items: flex-start;
  }

  .warning-icon {
    font-size: 16px;
    line-height: 1.2;
    color: var(--warning, #d29922);
  }

  .warning-content {
    display: flex;
    flex-direction: column;
    gap: 2px;
    color: var(--text);
  }

  .warning-title {
    color: var(--warning-fg, var(--text));
  }

  .warning-desc {
    margin: 0;
    color: var(--text-muted);
  }

  .error-banner {
    padding: 8px 12px;
    background-color: var(--danger-subtle, rgba(218, 54, 51, 0.1));
    border: 1px solid var(--danger);
    border-radius: var(--r-md);
    color: var(--danger);
  }
</style>
