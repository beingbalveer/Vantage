<script lang="ts">
  import * as api from "../../api";
  import {
    activeDialog,
    closeDialog,
    addToast,
    bumpBranches,
  } from "../../stores/ui";
  import { repos, statuses, loadRepos, refreshSingleStatus } from "../../stores/repos";
  import { validateBranchName, describeStartPoint } from "../../branchName";
  import { Modal, Button, Input, Checkbox } from "../common";

  interface Props {
    open?: boolean;
    repoId?: string;
    startPoint?: string;
    checkoutDefault?: boolean;
    onclose?: () => void;
    onsuccess?: (name: string, checkedOut: boolean) => void;
  }

  let {
    open,
    repoId,
    startPoint,
    checkoutDefault,
    onclose,
    onsuccess,
  }: Props = $props();

  let isStoreActive = $derived($activeDialog?.type === "branch_create");
  let isOpen = $derived(open !== undefined ? open : isStoreActive);

  let currentRepoId = $derived(
    repoId || (isStoreActive ? $activeDialog?.repoId : "") || ""
  );

  let repo = $derived($repos.find((r) => r.id === currentRepoId) || null);

  let initialStartPoint = $derived(
    (startPoint !== undefined
      ? startPoint
      : isStoreActive
        ? $activeDialog?.data?.startPoint
        : "") || "HEAD"
  );

  let name = $state("");
  let start = $state("HEAD");
  let checkout = $state(false);
  let touched = $state(false);
  let isSubmitting = $state(false);
  let errorMessage = $state<string | null>(null);

  // Reset the form every time the dialog opens, honouring any provided
  // start point (e.g. a commit sha passed by a commit-row menu).
  $effect(() => {
    if (isOpen) {
      const sp = initialStartPoint;
      name = "";
      start = sp;
      checkout = checkoutDefault ?? false;
      touched = false;
      isSubmitting = false;
      errorMessage = null;
    }
  });

  let nameError = $derived(name.trim() ? validateBranchName(name) : null);
  let canSubmit = $derived(
    name.trim().length > 0 && !nameError && !isSubmitting
  );

  let dirtyCount = $derived(
    currentRepoId ? $statuses[currentRepoId]?.dirtyCount ?? 0 : 0
  );
  let dirtyWarning = $derived(checkout && dirtyCount > 0);

  let startPointLabel = $derived(
    describeStartPoint(start, repo?.branch ?? $statuses[currentRepoId]?.branch ?? null)
  );

  function handleClose() {
    if (isSubmitting) return;
    onclose?.();
    if (isStoreActive) closeDialog();
  }

  async function handleSubmit() {
    if (!canSubmit || !currentRepoId) return;
    touched = true;
    if (nameError) return;

    isSubmitting = true;
    errorMessage = null;
    const branchName = name.trim();
    const startValue = start.trim() || "HEAD";
    const wantsCheckout = checkout;

    try {
      await api.branchCreate(currentRepoId, branchName, startValue, wantsCheckout);
      addToast({
        message: wantsCheckout
          ? `Created and checked out ${branchName}`
          : `Created branch ${branchName}`,
        kind: "success",
      });

      await refreshSingleStatus(currentRepoId);
      await loadRepos();
      bumpBranches();

      onsuccess?.(branchName, wantsCheckout);
      handleClose();
    } catch (e: any) {
      errorMessage = e?.message || "Failed to create branch.";
    } finally {
      isSubmitting = false;
    }
  }
</script>

{#if isOpen}
  <Modal
    open={isOpen}
    title="New branch"
    width="480px"
    closeOnEscape={!isSubmitting}
    closeOnOutsideClick={!isSubmitting}
    onclose={handleClose}
  >
    <div class="branch-form">
      <label class="field">
        <span class="field-label text-sm text-strong">Branch name</span>
        <Input
          bind:value={name}
          placeholder="feature/my-change"
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

      <label class="field">
        <span class="field-label text-sm text-strong">Start point</span>
        <Input bind:value={start} placeholder="HEAD" />
        <span class="field-hint text-xs text-muted">
          HEAD by default. Use a commit SHA or an existing branch to branch from it.
        </span>
      </label>

      <Checkbox
        bind:checked={checkout}
        label="Check out this branch after creating"
      />

      {#if dirtyWarning}
        <div class="warning-banner" role="alert">
          <span class="warning-icon">⚠</span>
          <div class="warning-content text-sm">
            <span class="text-strong">Working tree has uncommitted changes.</span>
            <p class="warning-desc">
              Checking out the new branch would need a clean tree. Uncheck “Check out” to
              just create the branch, or commit/discard your changes first.
            </p>
          </div>
        </div>
      {/if}

      {#if errorMessage}
        <div class="error-banner text-sm" role="alert">{errorMessage}</div>
      {/if}

      <p class="preview text-xs text-muted">
        Creates <span class="font-mono">{name.trim() || "…"}</span> from
        <span class="font-mono">{startPointLabel}</span> in
        <span class="text-strong">{repo?.name || "this repository"}</span>.
      </p>
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
        {checkout ? "Create and check out" : "Create branch"}
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

  .field-hint {
    line-height: 1.4;
  }

  .field-error {
    color: var(--danger);
  }

  .preview {
    line-height: 1.5;
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
