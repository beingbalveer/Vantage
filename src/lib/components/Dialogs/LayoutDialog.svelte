<script lang="ts">
  import { activeDialog, closeDialog } from "../../stores/ui";
  import {
    loadLayoutByName,
    readSavedLayouts,
    saveLayoutAs,
  } from "../../commands";
  import { Modal, Button, Input } from "../common";

  let isStoreActive = $derived($activeDialog?.type === "layout");
  let mode = $derived<"save" | "load">(
    isStoreActive && $activeDialog?.data?.mode === "load" ? "load" : "save"
  );
  let isOpen = $derived(isStoreActive);

  let name = $state("");
  let isSubmitting = $state(false);

  // Re-read on open so a layout saved in another window shows up immediately.
  let savedLayouts = $derived(isStoreActive ? readSavedLayouts() : {});
  let savedNames = $derived(Object.keys(savedLayouts).sort((a, b) => a.localeCompare(b)));

  $effect(() => {
    if (isOpen) {
      name = "";
      isSubmitting = false;
    }
  });

  function handleClose() {
    if (isSubmitting) return;
    if (isStoreActive) closeDialog();
  }

  async function handleSave() {
    if (!name.trim() || isSubmitting) return;
    isSubmitting = true;
    try {
      if (await saveLayoutAs(name)) {
        closeDialog();
      }
    } finally {
      isSubmitting = false;
    }
  }

  async function handleLoad(target: string | null) {
    if (isSubmitting) return;
    isSubmitting = true;
    try {
      await loadLayoutByName(target);
      closeDialog();
    } finally {
      isSubmitting = false;
    }
  }
</script>

{#if isOpen}
  <Modal
    open={isOpen}
    title={mode === "save" ? "Save layout" : "Load layout"}
    width="460px"
    closeOnEscape={!isSubmitting}
    closeOnOutsideClick={!isSubmitting}
    onclose={handleClose}
  >
    {#if mode === "save"}
      <div class="layout-form">
        <label class="field">
          <span class="field-label text-sm text-strong">Layout name</span>
          <Input
            bind:value={name}
            placeholder="frontend + backend"
            autofocus
            onkeydown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                handleSave();
              }
            }}
          />
          <span class="field-hint text-xs text-muted">
            Saves the current pane count, repositories and tabs. The terminal and sidebar
            sizes are already restored with the workspace.
          </span>
        </label>
      </div>

      {#snippet footer()}
        <Button variant="default" onclick={handleClose} disabled={isSubmitting}>
          Cancel
        </Button>
        <Button
          variant="primary"
          onclick={handleSave}
          loading={isSubmitting}
          disabled={!name.trim()}
        >
          Save layout
        </Button>
      {/snippet}
    {:else}
      <div class="layout-list" role="listbox" aria-label="Saved layouts">
        <button
          type="button"
          class="layout-option"
          role="option"
          aria-selected="false"
          disabled={isSubmitting}
          onclick={() => handleLoad(null)}
        >
          <span class="layout-name">Last used workspace</span>
          <span class="layout-meta text-xs text-muted">Restore the panes from your last session</span>
        </button>

        {#each savedNames as savedName (savedName)}
          <button
            type="button"
            class="layout-option"
            role="option"
            aria-selected="false"
            disabled={isSubmitting}
            onclick={() => handleLoad(savedName)}
          >
            <span class="layout-name">{savedName}</span>
            <span class="layout-meta text-xs text-muted">
              {savedLayouts[savedName].paneCount}
              {savedLayouts[savedName].paneCount === 1 ? "pane" : "panes"}
              {#if savedLayouts[savedName].savedAt}
                · saved {new Date(savedLayouts[savedName].savedAt!).toLocaleDateString()}
              {/if}
            </span>
          </button>
        {/each}

        {#if savedNames.length === 0}
          <p class="empty-copy text-sm text-muted">
            No saved layouts yet. Use “Save layout as…” to name the current arrangement.
          </p>
        {/if}
      </div>

      {#snippet footer()}
        <Button variant="default" onclick={handleClose} disabled={isSubmitting}>
          Cancel
        </Button>
      {/snippet}
    {/if}
  </Modal>
{/if}

<style>
  .layout-form {
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
    line-height: 1.45;
  }

  .layout-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-height: 320px;
    overflow-y: auto;
  }

  .layout-option {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    width: 100%;
    padding: 9px 12px;
    text-align: left;
    background-color: var(--bg);
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-md);
    color: var(--text);
    cursor: pointer;
  }

  .layout-option:hover:not(:disabled) {
    background-color: var(--bg-muted);
    border-color: var(--border);
  }

  .layout-option:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
  }

  .layout-option:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .layout-name {
    font-size: 13px;
    font-weight: 600;
  }

  .layout-meta {
    line-height: 1.35;
  }

  .empty-copy {
    margin: 4px 0 0;
    line-height: 1.5;
  }
</style>
