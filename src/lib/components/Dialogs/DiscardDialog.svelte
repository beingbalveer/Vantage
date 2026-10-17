<script lang="ts">
  import type { BackupInfo } from "../../types";
  import * as api from "../../api";
  import { activeDialog, closeDialog, addToast } from "../../stores/ui";
  import { refreshSingleStatus, loadRepos } from "../../stores/repos";
  import { Modal, Button } from "../common";

  interface Props {
    open?: boolean;
    repoId?: string;
    paths?: string[];
    onclose?: () => void;
    onsuccess?: (backup: BackupInfo) => void;
  }

  let {
    open,
    repoId,
    paths,
    onclose,
    onsuccess,
  }: Props = $props();

  let isStoreActive = $derived($activeDialog?.type === "discard");
  let isOpen = $derived(open !== undefined ? open : isStoreActive);

  let currentRepoId = $derived(
    repoId || (isStoreActive ? $activeDialog?.repoId : "") || ""
  );

  let currentPaths = $derived(
    paths || (isStoreActive ? $activeDialog?.data?.paths : []) || []
  );

  let isDiscarding = $state(false);
  let errorMessage = $state<string | null>(null);

  function handleClose() {
    errorMessage = null;
    isDiscarding = false;
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

  async function handleDiscard() {
    if (!currentRepoId || currentPaths.length === 0 || isDiscarding) return;

    isDiscarding = true;
    errorMessage = null;

    try {
      const backup = await api.discard(currentRepoId, currentPaths);
      const count = currentPaths.length;

      addToast({
        message: `Discarded changes to ${count} file${count === 1 ? "" : "s"}.`,
        kind: "info",
        actionLabel: "Undo",
        action: async () => {
          try {
            await api.backupRestore(currentRepoId, backup.ref);
            addToast({ message: "Discard undone (restored from backup)", kind: "success" });
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
      const msg = e.message || "Failed to discard changes.";
      errorMessage = msg;
      addToast({
        message: msg,
        kind: "error",
      });
    } finally {
      isDiscarding = false;
    }
  }
</script>

<Modal
  open={isOpen}
  title="Discard changes"
  onclose={handleClose}
>
  <p class="text-base text-strong">
    Discard changes to {currentPaths.length} file{currentPaths.length === 1 ? "" : "s"}?
  </p>

  <p class="text-sm text-muted">
    A backup is saved so you can undo this for 30 days.
  </p>

  <!-- Files list preview -->
  {#if currentPaths.length > 0}
    <div class="files-list-box font-mono text-xs">
      {#each currentPaths.slice(0, 8) as path}
        <div class="file-item text-muted">
          <span class="file-bullet">•</span>
          <span class="file-path">{path}</span>
        </div>
      {/each}
      {#if currentPaths.length > 8}
        <div class="more-files text-muted">
          … and {currentPaths.length - 8} more
        </div>
      {/if}
    </div>
  {/if}

  {#if errorMessage}
    <div class="error-banner text-sm" role="alert">
      {errorMessage}
    </div>
  {/if}

  {#snippet footer()}
    <Button
      variant="default"
      onclick={handleClose}
      disabled={isDiscarding}
    >
      Cancel
    </Button>
    <Button
      variant="danger"
      onclick={handleDiscard}
      disabled={isDiscarding || currentPaths.length === 0}
      loading={isDiscarding}
    >
      {isDiscarding
        ? "Discarding…"
        : `Discard ${currentPaths.length} file${currentPaths.length === 1 ? "" : "s"}`}
    </Button>
  {/snippet}
</Modal>

<style>
  .files-list-box {
    background-color: var(--bg-subtle);
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-md);
    padding: 8px 12px;
    max-height: 160px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .file-item {
    display: flex;
    align-items: center;
    gap: 6px;
    line-height: 18px;
  }

  .file-bullet {
    color: var(--text-faint);
  }

  .file-path {
    color: var(--text);
    word-break: break-all;
  }

  .more-files {
    padding-top: 4px;
    text-align: center;
  }

  .error-banner {
    padding: 8px 12px;
    background-color: var(--danger-subtle, rgba(218, 54, 51, 0.1));
    border: 1px solid var(--danger);
    border-radius: var(--r-md);
    color: var(--danger);
  }
</style>
