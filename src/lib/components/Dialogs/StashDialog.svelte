<script lang="ts">
  import type { StashEntry } from "../../platform/types";
  import * as api from "../../platform/api";
  import { addToast, stashDialogOpen } from "../../app/ui";
  import { repos, activeRepoId, refreshSingleStatus, loadRepos } from "../../domain/stores/repos";
  import { Modal, Button, Input, Checkbox } from "../common";
  import { formatRelativeTime, formatFullTimestamp } from "../../domain/format";
  import { formatStashIndex, summarizeStashSubject } from "../../domain/stash";

  interface Props {
    open?: boolean;
    repoId?: string;
    onclose?: () => void;
  }

  let { open, repoId, onclose }: Props = $props();

  let isOpen = $derived(open !== undefined ? open : $stashDialogOpen);
  let currentRepoId = $derived(repoId || $activeRepoId || "");
  let repo = $derived($repos.find((r) => r.id === currentRepoId) || null);

  let stashes = $state<StashEntry[]>([]);
  let isLoading = $state(false);
  let busyIndex = $state<number | null>(null);
  let errorMessage = $state<string | null>(null);
  let pendingDrop = $state<StashEntry | null>(null);

  let showCreate = $state(false);
  let newMessage = $state("");
  let includeUntracked = $state(true);
  let isCreating = $state(false);

  $effect(() => {
    if (!isOpen) return;
    const id = currentRepoId;
    if (!id) {
      stashes = [];
      return;
    }
    void loadStashes(id);
  });

  $effect(() => {
    if (!isOpen) {
      pendingDrop = null;
      errorMessage = null;
      showCreate = false;
      newMessage = "";
    }
  });

  async function loadStashes(id: string) {
    isLoading = true;
    try {
      stashes = await api.stashList(id);
    } catch (e: any) {
      stashes = [];
      errorMessage = e?.message || "Failed to load stashes";
    } finally {
      isLoading = false;
    }
  }

  async function refreshAfterChange(id: string) {
    await refreshSingleStatus(id);
    await loadRepos();
    await loadStashes(id);
  }

  async function handleApply(stash: StashEntry) {
    if (!currentRepoId || busyIndex !== null) return;
    busyIndex = stash.index;
    errorMessage = null;
    try {
      await api.stashApply(currentRepoId, stash.index);
      addToast({
        message: `Applied ${formatStashIndex(stash.index)} (stash kept)`,
        kind: "success",
      });
      await refreshAfterChange(currentRepoId);
    } catch (e: any) {
      errorMessage = e?.message || "Apply failed";
      addToast({ message: errorMessage || "Apply failed", kind: "error" });
    } finally {
      busyIndex = null;
    }
  }

  async function handlePop(stash: StashEntry) {
    if (!currentRepoId || busyIndex !== null) return;
    busyIndex = stash.index;
    errorMessage = null;
    try {
      await api.stashPop(currentRepoId, stash.index);
      addToast({
        message: `Popped ${formatStashIndex(stash.index)}`,
        kind: "success",
      });
      await refreshAfterChange(currentRepoId);
    } catch (e: any) {
      errorMessage = e?.message || "Pop failed";
      addToast({ message: errorMessage || "Pop failed", kind: "error" });
    } finally {
      busyIndex = null;
    }
  }

  async function handleDrop(stash: StashEntry) {
    if (!currentRepoId || busyIndex !== null) return;
    busyIndex = stash.index;
    errorMessage = null;
    try {
      await api.stashDrop(currentRepoId, stash.index);
      addToast({
        message: `Dropped ${formatStashIndex(stash.index)}`,
        kind: "info",
      });
      pendingDrop = null;
      await refreshAfterChange(currentRepoId);
    } catch (e: any) {
      errorMessage = e?.message || "Drop failed";
      addToast({ message: errorMessage || "Drop failed", kind: "error" });
    } finally {
      busyIndex = null;
    }
  }

  async function handleCreate() {
    if (!currentRepoId || isCreating) return;
    isCreating = true;
    errorMessage = null;
    try {
      const result = await api.stashPush(
        currentRepoId,
        newMessage.trim() || undefined,
        includeUntracked
      );
      if (!result.stashed) {
        addToast({ message: "Nothing to stash", kind: "info" });
      } else {
        const label = result.entry
          ? `${formatStashIndex(result.entry.index)} · ${summarizeStashSubject(result.entry.subject)}`
          : "Stashed your changes";
        addToast({ message: `Stashed as ${label}`, kind: "success" });
        showCreate = false;
        newMessage = "";
      }
      await refreshAfterChange(currentRepoId);
    } catch (e: any) {
      errorMessage = e?.message || "Stash failed";
      addToast({ message: errorMessage || "Stash failed", kind: "error" });
    } finally {
      isCreating = false;
    }
  }

  function handleClose() {
    onclose?.();
    if (open === undefined) {
      $stashDialogOpen = false;
    }
  }
</script>

{#if isOpen}
  <Modal
    open={isOpen}
    title={repo ? `Stashes — ${repo.name}` : "Stashes"}
    description="Parked work you can restore at any time."
    width="560px"
    closeOnEscape={busyIndex === null && !isCreating}
    closeOnOutsideClick={busyIndex === null && !isCreating}
    onclose={handleClose}
  >
    <div class="stash-content">
      {#if !currentRepoId}
        <div class="empty-state text-sm text-muted">Select a repository to see its stashes.</div>
      {:else if isLoading}
        <div class="empty-state text-sm text-muted">Loading stashes…</div>
      {:else if stashes.length === 0}
        <div class="empty-state">
          <p class="text-sm text-muted">No stashes yet.</p>
          <p class="text-xs text-muted">
            Choose "Stash changes" below, or switch branches with a dirty tree to park work here.
          </p>
        </div>
      {:else}
        <ul class="stash-list">
          {#each stashes as stash (stash.sha)}
            <li class="stash-row">
              <div class="stash-info">
                <div class="stash-line">
                  <span class="stash-index font-mono text-xs">{formatStashIndex(stash.index)}</span>
                  <span class="stash-subject text-sm" title={stash.subject}>
                    {summarizeStashSubject(stash.subject)}
                  </span>
                </div>
                <div class="stash-meta text-xs text-muted" title={formatFullTimestamp(stash.date)}>
                  {formatRelativeTime(stash.date)} · {stash.sha.slice(0, 7)}
                </div>
              </div>

              <div class="stash-actions">
                <Button
                  variant="default"
                  size="sm"
                  disabled={busyIndex !== null}
                  onclick={() => handleApply(stash)}
                >
                  Apply
                </Button>
                <Button
                  variant="default"
                  size="sm"
                  disabled={busyIndex !== null}
                  onclick={() => handlePop(stash)}
                >
                  Pop
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  class="drop-btn"
                  disabled={busyIndex !== null}
                  onclick={() => (pendingDrop = stash)}
                >
                  Drop
                </Button>
              </div>
            </li>
            {#if pendingDrop && pendingDrop.index === stash.index}
              <li class="drop-confirm" role="alert">
                <span class="text-sm">
                  Drop <span class="font-mono">{formatStashIndex(stash.index)}</span> —
                  "{summarizeStashSubject(stash.subject)}"? This cannot be undone.
                </span>
                <div class="confirm-actions">
                  <Button variant="default" size="sm" onclick={() => (pendingDrop = null)}>
                    Cancel
                  </Button>
                  <Button
                    variant="danger"
                    size="sm"
                    loading={busyIndex === stash.index}
                    disabled={busyIndex !== null}
                    onclick={() => handleDrop(stash)}
                  >
                    Drop stash
                  </Button>
                </div>
              </li>
            {/if}
          {/each}
        </ul>
      {/if}

      {#if showCreate && currentRepoId}
        <div class="create-box">
          <Input
            bind:value={newMessage}
            placeholder="Stash message (optional)"
            onkeydown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                handleCreate();
              }
            }}
          />
          <Checkbox bind:checked={includeUntracked} label="Include untracked files" />
          <div class="create-actions">
            <Button variant="ghost" size="sm" onclick={() => (showCreate = false)}>
              Cancel
            </Button>
            <Button variant="primary" size="sm" loading={isCreating} onclick={handleCreate}>
              Save stash
            </Button>
          </div>
        </div>
      {/if}

      {#if errorMessage}
        <div class="error-banner text-sm" role="alert">{errorMessage}</div>
      {/if}
    </div>

    {#snippet footer()}
      {#if currentRepoId && !showCreate}
        <Button variant="default" onclick={() => (showCreate = true)}>Stash changes…</Button>
      {/if}
      <Button variant="default" onclick={handleClose}>Close</Button>
    {/snippet}
  </Modal>
{/if}

<style>
  .stash-content {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 12px 4px;
  }
  .empty-state p {
    margin: 0;
  }

  .stash-list {
    list-style: none;
    margin: 0;
    padding: 4px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 320px;
    overflow-y: auto;
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-md);
  }

  .stash-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 6px 8px;
    border-radius: var(--r-sm);
  }
  .stash-row:hover {
    background-color: var(--bg-subtle);
  }

  .stash-info {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .stash-line {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }

  .stash-index {
    color: var(--accent);
    flex-shrink: 0;
  }

  .stash-subject {
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .stash-meta {
    white-space: nowrap;
  }

  .stash-actions {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }

  :global(.drop-btn) {
    color: var(--danger);
  }

  .drop-confirm {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
    background-color: var(--warning-bg);
    border: 1px solid var(--warning);
    border-radius: var(--r-md);
  }

  .confirm-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .create-box {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
    background-color: var(--bg-subtle);
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-md);
  }

  .create-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .error-banner {
    padding: 8px 12px;
    background-color: var(--danger-subtle, rgba(218, 54, 51, 0.1));
    border: 1px solid var(--danger);
    border-radius: var(--r-md);
    color: var(--danger);
    white-space: pre-wrap;
  }
</style>
