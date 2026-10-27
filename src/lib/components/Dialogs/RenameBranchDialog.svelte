<script lang="ts">
  import * as api from "../../platform/api";
  import {
    activeDialog,
    closeDialog,
    addToast,
    bumpBranches,
  } from "../../app/ui";
  import { repos, statuses, loadRepos, refreshSingleStatus } from "../../domain/stores/repos";
  import { validateBranchName } from "../../domain/branchName";
  import { Modal, Button, Input } from "../common";

  interface Props {
    open?: boolean;
    repoId?: string;
    from?: string;
    onclose?: () => void;
    onsuccess?: (from: string, to: string) => void;
  }

  let { open, repoId, from, onclose, onsuccess }: Props = $props();

  let isStoreActive = $derived($activeDialog?.type === "branch_rename");
  let isOpen = $derived(open !== undefined ? open : isStoreActive);

  let currentRepoId = $derived(
    repoId || (isStoreActive ? $activeDialog?.repoId : "") || ""
  );

  let repo = $derived($repos.find((r) => r.id === currentRepoId) || null);

  let sourceBranch = $derived(
    from ||
      (isStoreActive ? $activeDialog?.data?.from : "") ||
      repo?.branch ||
      $statuses[currentRepoId]?.branch ||
      ""
  );

  let newName = $state("");
  let touched = $state(false);
  let isSubmitting = $state(false);
  let errorMessage = $state<string | null>(null);

  $effect(() => {
    if (isOpen) {
      newName = sourceBranch;
      touched = false;
      isSubmitting = false;
      errorMessage = null;
    }
  });

  let nameError = $derived(newName.trim() ? validateBranchName(newName) : null);
  let unchanged = $derived(newName.trim() === sourceBranch);
  let canSubmit = $derived(
    newName.trim().length > 0 && !nameError && !unchanged && !isSubmitting
  );

  function handleClose() {
    if (isSubmitting) return;
    onclose?.();
    if (isStoreActive) closeDialog();
  }

  async function handleSubmit() {
    if (!canSubmit || !currentRepoId || !sourceBranch) return;
    touched = true;
    if (nameError) return;

    isSubmitting = true;
    errorMessage = null;
    const target = newName.trim();

    try {
      await api.branchRename(currentRepoId, sourceBranch, target);

      // Repoint any branch views that were showing the old name so they do not
      // become dangling after the rename.
      const siblings = repo
        ? $repos.filter((r) => r.path === repo.path && r.branch === sourceBranch)
        : [];
      for (const view of siblings) {
        try {
          await api.repoUpdate(view.id, { branch: target });
        } catch {
          // A failed repoint should not mask a successful rename.
        }
      }

      addToast({
        message: `Renamed ${sourceBranch} to ${target}`,
        kind: "success",
      });

      await loadRepos();
      await refreshSingleStatus(currentRepoId);
      bumpBranches();

      onsuccess?.(sourceBranch, target);
      handleClose();
    } catch (e: any) {
      errorMessage = e?.message || "Failed to rename branch.";
    } finally {
      isSubmitting = false;
    }
  }
</script>

{#if isOpen}
  <Modal
    open={isOpen}
    title="Rename branch"
    width="460px"
    closeOnEscape={!isSubmitting}
    closeOnOutsideClick={!isSubmitting}
    onclose={handleClose}
  >
    <div class="branch-form">
      <p class="text-sm text-muted">
        Rename <span class="font-mono text-strong">{sourceBranch || "…"}</span> in
        <span class="text-strong">{repo?.name || "this repository"}</span>.
      </p>

      <label class="field">
        <span class="field-label text-sm text-strong">New name</span>
        <Input
          bind:value={newName}
          autofocus
          error={touched && !!nameError}
          oninput={() => (touched = true)}
          onkeydown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              handleSubmit();
            }
          }}
        />
        {#if touched && nameError}
          <span class="field-error text-xs" role="alert">{nameError}</span>
        {/if}
      </label>

      {#if errorMessage}
        <div class="error-banner text-sm" role="alert">{errorMessage}</div>
      {/if}
    </div>

    {#snippet footer()}
      <Button variant="default" onclick={handleClose} disabled={isSubmitting}>
        Cancel
      </Button>
      <Button
        variant="primary"
        onclick={handleSubmit}
        loading={isSubmitting}
        disabled={!canSubmit}
      >
        Rename branch
      </Button>
    {/snippet}
  </Modal>
{/if}

<style>
  .branch-form {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .field-label {
    color: var(--text);
  }

  .field-error {
    color: var(--danger);
  }

  .error-banner {
    padding: 8px 12px;
    background-color: var(--danger-subtle, rgba(218, 54, 51, 0.1));
    border: 1px solid var(--danger);
    border-radius: var(--r-md);
    color: var(--danger);
  }
</style>
