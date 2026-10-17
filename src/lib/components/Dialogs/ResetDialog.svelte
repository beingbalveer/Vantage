<script lang="ts">
  import type { CommitSummary, BackupInfo } from "../../types";
  import * as api from "../../api";
  import { activeDialog, closeDialog, addToast } from "../../stores/ui";
  import { statuses, repos, refreshSingleStatus, loadRepos } from "../../stores/repos";
  import { Modal, Button, Checkbox } from "../common";

  interface Props {
    open?: boolean;
    repoId?: string;
    targetSha?: string;
    fromSha?: string;
    targetLabel?: string;
    commits?: CommitSummary[];
    onclose?: () => void;
    onsuccess?: (backup: BackupInfo) => void;
  }

  let {
    open,
    repoId,
    targetSha,
    fromSha,
    targetLabel = "reviewed marker",
    commits,
    onclose,
    onsuccess,
  }: Props = $props();

  let isStoreActive = $derived($activeDialog?.type === "reset");
  let isOpen = $derived(open !== undefined ? open : isStoreActive);

  let currentRepoId = $derived(
    repoId || (isStoreActive ? $activeDialog?.repoId : "") || ""
  );

  let currentTargetSha = $derived(
    targetSha ||
      (isStoreActive
        ? $activeDialog?.data?.targetSha || $activeDialog?.data?.sha
        : "") ||
      ""
  );

  let currentFromSha = $derived(
    fromSha || (isStoreActive ? $activeDialog?.data?.fromSha : "") || ""
  );

  let repoEntry = $derived(
    $repos.find((r) => r.id === currentRepoId) || null
  );

  let droppedCommits = $state<CommitSummary[]>([]);
  let alsoDiscardDirty = $state(false);
  let isResetting = $state(false);
  let isRevertingRange = $state(false);
  let isCheckingPushed = $state(false);
  let pushedCount = $state(0);
  let errorMessage = $state<string | null>(null);

  // Check dirty state
  let isDirty = $derived.by(() => {
    if (!currentRepoId) return false;
    const status = $statuses[currentRepoId];
    return Boolean(status && status.dirtyCount > 0);
  });

  // Load dropped commits and check remote status on open
  $effect(() => {
    if (!isOpen || !currentRepoId) {
      droppedCommits = [];
      pushedCount = 0;
      alsoDiscardDirty = false;
      errorMessage = null;
      return;
    }

    if (commits && commits.length > 0) {
      droppedCommits = commits;
      checkRemoteCommits(commits);
    } else if (isStoreActive && $activeDialog?.data?.commits) {
      droppedCommits = $activeDialog.data.commits;
      checkRemoteCommits($activeDialog.data.commits);
    } else if (currentTargetSha) {
      fetchDroppedCommits();
    }
  });

  async function fetchDroppedCommits() {
    try {
      const page = await api.logPage(currentRepoId, 0, 50);
      const list: CommitSummary[] = [];
      for (const c of page) {
        if (c.sha === currentTargetSha) break;
        list.push(c);
      }
      droppedCommits = list;
      await checkRemoteCommits(list);
    } catch (e: any) {
      console.error("Failed to load commits for reset:", e);
    }
  }

  async function checkRemoteCommits(list: CommitSummary[]) {
    if (!currentRepoId || list.length === 0) return;
    isCheckingPushed = true;
    let count = 0;
    try {
      for (const c of list) {
        try {
          const isPushed = await api.checkPushed(currentRepoId, c.sha);
          if (isPushed) count++;
        } catch {
          // checkPushed may not be supported or error on detached HEAD
          break;
        }
      }
      pushedCount = count;
    } catch (e) {
      console.warn("Could not check remote status of commits:", e);
    } finally {
      isCheckingPushed = false;
    }
  }

  function handleClose() {
    errorMessage = null;
    isResetting = false;
    isRevertingRange = false;
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

  async function handleReset() {
    if (!currentRepoId || !currentTargetSha || isResetting) return;
    if (isDirty && !alsoDiscardDirty) return;

    isResetting = true;
    errorMessage = null;

    try {
      const backup = await api.resetTo(
        currentRepoId,
        currentTargetSha,
        alsoDiscardDirty
      );

      const count = droppedCommits.length;
      addToast({
        message: `Reset done. Dropped ${count} commit${count === 1 ? "" : "s"}.`,
        kind: "info",
        actionLabel: "Undo",
        action: async () => {
          try {
            await api.backupRestore(currentRepoId, backup.ref);
            addToast({ message: "Reset undone (restored from backup)", kind: "success" });
            await refreshSingleStatus(currentRepoId);
            await loadRepos();
          } catch (e: any) {
            addToast({ message: e.message || "Failed to restore backup", kind: "error" });
          }
        },
      });

      await refreshSingleStatus(currentRepoId);
      await loadRepos();

      onsuccess?.(backup);
      handleClose();
    } catch (e: any) {
      const msg = errorMessage || "Failed to reset repository.";
      addToast({
        message: msg,
        kind: "error",
      });
    } finally {
      isResetting = false;
    }
  }

  async function handleRevertRange() {
    if (!currentRepoId || !currentTargetSha || isRevertingRange) return;

    isRevertingRange = true;
    errorMessage = null;

    try {
      const from = currentFromSha || "HEAD";
      const to = currentTargetSha;
      const res = await api.revertRange(currentRepoId, from, to);

      addToast({
        message: `Created ${res.length} revert commit${res.length === 1 ? "" : "s"} for range`,
        kind: "success",
      });

      await refreshSingleStatus(currentRepoId);
      await loadRepos();

      handleClose();
    } catch (e: any) {
      const msg = errorMessage || "Failed to revert range.";
      addToast({
        message: msg,
        kind: "error",
      });
    } finally {
      isRevertingRange = false;
    }
  }
</script>

{#if isOpen}
  <Modal
    open={isOpen}
    title={`Reset ${repoEntry?.name || "repository"} to ${targetLabel}?`}
    width="520px"
    closeOnEscape={!isResetting && !isRevertingRange}
    closeOnOutsideClick={!isResetting && !isRevertingRange}
    onclose={handleClose}
  >
    <div class="reset-content">
      <p class="text-base text-muted">
        This will remove {droppedCommits.length} commit{droppedCommits.length === 1 ? "" : "s"} from {repoEntry?.branch || "HEAD"}:
      </p>

      <!-- Commits list (up to 20) -->
      <div class="commits-list-container">
        <ul class="commits-list">
          {#each droppedCommits.slice(0, 20) as c (c.sha)}
            <li class="commit-item text-xs font-mono">
              <span class="commit-bullet">●</span>
              <span class="commit-subj text-strong">{c.subject}</span>
              <span class="commit-sha text-muted">{c.short || c.sha.slice(0, 7)}</span>
            </li>
          {/each}
        </ul>
        {#if droppedCommits.length > 20}
          <div class="more-commits text-xs text-muted">
            … and {droppedCommits.length - 20} more
          </div>
        {/if}
      </div>

      <!-- Pushed to remote red warning banner -->
      {#if pushedCount > 0}
        <div class="pushed-warning" role="alert">
          <div class="warning-left">
            <span class="warning-icon">⚠</span>
            <div class="warning-text text-sm">
              <span class="warning-strong text-strong">
                {pushedCount} of these commits {pushedCount === 1 ? "is" : "are"} already pushed.
              </span>
              Resetting will require a force push. Prefer reverting.
            </div>
          </div>
          <Button
            variant="default"
            size="sm"
            onclick={handleRevertRange}
            disabled={isRevertingRange || isResetting}
          >
            {isRevertingRange ? "Reverting range…" : "Revert the range instead"}
          </Button>
        </div>
      {/if}

      <!-- Dirty Working Tree Checkbox -->
      {#if isDirty}
        <div class="dirty-section text-sm">
          <Checkbox
            bind:checked={alsoDiscardDirty}
            label="Also discard uncommitted changes (backed up)"
          />
          {#if !alsoDiscardDirty}
            <p class="dirty-hint text-xs text-muted">
              Working tree is dirty. Check the box above to discard uncommitted changes with backup, or commit them first.
            </p>
          {/if}
        </div>
      {/if}

      <p class="backup-note text-sm text-muted">
        A backup is saved so you can undo this.
      </p>

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
        disabled={isResetting || isRevertingRange}
      >
        Cancel
      </Button>
      <Button
        variant="danger"
        onclick={handleReset}
        loading={isResetting}
        disabled={isResetting || isRevertingRange || (isDirty && !alsoDiscardDirty) || !currentTargetSha}
      >
        {isResetting
          ? "Resetting…"
          : `Reset and drop ${droppedCommits.length} commit${droppedCommits.length === 1 ? "" : "s"}`}
      </Button>
    {/snippet}
  </Modal>
{/if}

<style>
  .reset-content {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .commits-list-container {
    background-color: var(--bg-subtle);
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-md);
    padding: 8px 12px;
    max-height: 180px;
    overflow-y: auto;
  }

  .commits-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .commit-item {
    display: flex;
    align-items: center;
    gap: 8px;
    line-height: 18px;
  }

  .commit-bullet {
    color: var(--text-faint);
    font-size: 8px;
  }

  .commit-subj {
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }

  .commit-sha {
    flex-shrink: 0;
  }

  .more-commits {
    padding-top: 6px;
    text-align: center;
  }

  .pushed-warning {
    padding: 10px 12px;
    background-color: var(--danger-subtle, rgba(218, 54, 51, 0.1));
    border: 1px solid var(--danger);
    border-radius: var(--r-md);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .warning-left {
    display: flex;
    align-items: flex-start;
    gap: 8px;
  }

  .warning-icon {
    color: var(--danger);
    font-size: 16px;
    line-height: 1.2;
  }

  .warning-text {
    color: var(--text);
    line-height: 18px;
  }

  .warning-strong {
    color: var(--danger);
  }

  .dirty-section {
    background-color: var(--bg-subtle);
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-md);
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .dirty-hint {
    margin: 0;
    line-height: 16px;
  }

  .backup-note {
    margin: 0;
  }

  .error-banner {
    padding: 8px 12px;
    background-color: var(--danger-subtle, rgba(218, 54, 51, 0.1));
    border: 1px solid var(--danger);
    border-radius: var(--r-md);
    color: var(--danger);
  }
</style>
