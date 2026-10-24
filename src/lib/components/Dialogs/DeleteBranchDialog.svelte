<script lang="ts">
  import * as api from "../../api";
  import {
    activeDialog,
    closeDialog,
    addToast,
    bumpBranches,
  } from "../../stores/ui";
  import { repos, statuses, loadRepos, refreshSingleStatus } from "../../stores/repos";
  import { Modal, Button } from "../common";

  interface Props {
    open?: boolean;
    repoId?: string;
    name?: string;
    onclose?: () => void;
    onsuccess?: (name: string, forced: boolean) => void;
  }

  let { open, repoId, name, onclose, onsuccess }: Props = $props();

  let isStoreActive = $derived($activeDialog?.type === "branch_delete");
  let isOpen = $derived(open !== undefined ? open : isStoreActive);

  let currentRepoId = $derived(
    repoId || (isStoreActive ? $activeDialog?.repoId : "") || ""
  );

  let repo = $derived($repos.find((r) => r.id === currentRepoId) || null);

  let branchName = $derived(
    name || (isStoreActive ? $activeDialog?.data?.name : "") || ""
  );

  // Which branch the working copy actually has checked out (a repo may be open
  // as several branch views at once).
  let checkedOutBranch = $derived.by(() => {
    if (!repo) return null;
    const siblings = $repos.filter((r) => r.path === repo.path);
    for (const view of siblings) {
      const status = $statuses[view.id];
      if (status?.isCheckedOut) return status.branch ?? view.branch ?? null;
    }
    return null;
  });

  let isCurrent = $derived(!!branchName && checkedOutBranch === branchName);

  let merged = $state<boolean | null>(null);
  let step = $state<"confirm" | "force">("confirm");
  let isDeleting = $state(false);
  let errorMessage = $state<string | null>(null);

  $effect(() => {
    if (!isOpen) return;
    const id = currentRepoId;
    const target = branchName;
    step = "confirm";
    isDeleting = false;
    errorMessage = null;
    merged = null;
    if (id && target) {
      let cancelled = false;
      api
        .branchIsMerged(id, target)
        .then((v) => {
          if (!cancelled) merged = v;
        })
        .catch(() => {
          if (!cancelled) merged = null;
        });
      return () => {
        cancelled = true;
      };
    }
  });

  function handleClose() {
    if (isDeleting) return;
    onclose?.();
    if (isStoreActive) closeDialog();
  }

  async function performDelete(force: boolean) {
    if (!currentRepoId || !branchName || isDeleting) return;
    isDeleting = true;
    errorMessage = null;

    try {
      await api.branchDelete(currentRepoId, branchName, force);
      addToast({
        message: force
          ? `Force deleted branch ${branchName}`
          : `Deleted branch ${branchName}`,
        kind: force ? "info" : "success",
      });

      await refreshSingleStatus(currentRepoId);
      await loadRepos();
      bumpBranches();

      onsuccess?.(branchName, force);
      handleClose();
    } catch (e: any) {
      errorMessage = e?.message || "Failed to delete branch.";
      // A safe delete of an unmerged branch is git telling us to force.
      if (!force && /not fully merged|not merged/i.test(errorMessage || "")) {
        step = "force";
      }
    } finally {
      isDeleting = false;
    }
  }
</script>

{#if isOpen}
  <Modal
    open={isOpen}
    title={step === "force" ? "Force delete branch" : "Delete branch"}
    width="460px"
    closeOnEscape={!isDeleting}
    closeOnOutsideClick={!isDeleting}
    onclose={handleClose}
  >
    <div class="delete-content">
      {#if step === "force"}
        <p class="text-base text-strong">
          Force delete <span class="font-mono">{branchName}</span>?
        </p>
        <p class="text-sm text-muted">
          This discards any commits on <span class="font-mono">{branchName}</span> that
          are not merged elsewhere in <span class="text-strong">{repo?.name}</span>.
          Those commits may become unreachable and cannot be recovered from Vantage.
        </p>
      {:else}
        <p class="text-base text-strong">
          Delete branch <span class="font-mono">{branchName}</span>?
        </p>
        <p class="text-sm text-muted">
          From repository <span class="text-strong">{repo?.name || "…"}</span>
          {#if repo?.path}<span class="repo-path font-mono">{repo.path}</span>{/if}
        </p>

        {#if merged === false}
          <div class="warning-banner" role="alert">
            <span class="warning-icon">⚠</span>
            <div class="warning-content text-sm">
              <span class="text-strong">This branch has unmerged commits.</span>
              <p class="warning-desc">
                A safe delete will be refused by git. To remove the branch anyway you must
                use force delete, which discards those commits.
              </p>
            </div>
          </div>
        {:else if merged === true}
          <p class="text-sm text-muted">
            This branch is fully merged — deleting it is safe.
          </p>
        {/if}

        {#if isCurrent}
          <div class="error-banner text-sm" role="alert">
            This is the currently checked-out branch. Switch to another branch before
            deleting it.
          </div>
        {/if}
      {/if}

      {#if errorMessage}
        <div class="error-banner text-sm" role="alert">{errorMessage}</div>
      {/if}
    </div>

    {#snippet footer()}
      <Button variant="default" onclick={handleClose} disabled={isDeleting}>
        Cancel
      </Button>

      {#if step === "force"}
        <Button
          variant="danger"
          onclick={() => performDelete(true)}
          loading={isDeleting}
          disabled={isDeleting || isCurrent}
        >
          Force delete branch
        </Button>
      {:else if merged === false}
        <Button
          variant="danger"
          onclick={() => (step = "force")}
          disabled={isDeleting || isCurrent}
        >
          Force delete…
        </Button>
      {:else}
        <Button
          variant="danger"
          onclick={() => performDelete(false)}
          loading={isDeleting}
          disabled={isDeleting || isCurrent}
        >
          Delete branch
        </Button>
      {/if}
    {/snippet}
  </Modal>
{/if}

<style>
  .delete-content {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .repo-path {
    display: block;
    margin-top: 4px;
    color: var(--text-faint);
    word-break: break-all;
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
