<script lang="ts">
  import * as api from "../../platform/api";
  import {
    tagCreateRequest,
    closeTagCreateDialog,
    addToast,
  } from "../../app/ui";
  import { loadTags } from "../../domain/stores/tags";
  import { Modal, Button, Checkbox } from "../common";

  let request = $derived($tagCreateRequest);

  let name = $state("");
  let annotated = $state(false);
  let message = $state("");
  let pushAfter = $state(false);
  let creating = $state(false);
  let errorMessage = $state<string | null>(null);

  // Reset the form each time a new request opens.
  let lastKey = $state<string | null>(null);
  $effect(() => {
    const req = $tagCreateRequest;
    if (req) {
      const key = `${req.repoId}:${req.target}`;
      if (key !== lastKey) {
        lastKey = key;
        name = "";
        annotated = false;
        message = "";
        pushAfter = false;
        errorMessage = null;
        creating = false;
      }
    } else {
      lastKey = null;
    }
  });

  let isOpen = $derived(!!request);
  let trimmedName = $derived(name.trim());
  let canCreate = $derived(
    trimmedName.length > 0 && (!annotated || message.trim().length > 0)
  );

  function handleClose() {
    if (creating) return;
    errorMessage = null;
    closeTagCreateDialog();
  }

  async function handleCreate() {
    if (!request || !canCreate || creating) return;
    creating = true;
    errorMessage = null;
    try {
      await api.tagCreate(
        request.repoId,
        trimmedName,
        request.target,
        annotated,
        annotated ? message : undefined
      );
      await loadTags(request.repoId);
      addToast({ message: `Created tag ${trimmedName}`, kind: "success" });

      if (pushAfter) {
        try {
          await api.tagPush(request.repoId, trimmedName);
          addToast({
            message: `Pushed tag ${trimmedName} to origin`,
            kind: "success",
          });
        } catch (e: any) {
          addToast({
            message: e.message || `Created tag but failed to push ${trimmedName}`,
            kind: "error",
          });
        }
      }

      closeTagCreateDialog();
    } catch (e: any) {
      errorMessage = e.message || "Failed to create tag";
    } finally {
      creating = false;
    }
  }
</script>

{#if isOpen && request}
  <Modal
    open={isOpen}
    title="Create tag"
    width="460px"
    closeOnEscape={!creating}
    closeOnOutsideClick={!creating}
    onclose={handleClose}
  >
    <div class="tag-form">
      <div class="target-line text-sm text-muted">
        Tagging <span class="target-label font-mono text-strong">{request.targetLabel}</span>
      </div>

      <label class="field">
        <span class="field-label text-strong text-sm">Tag name</span>
        <input
          class="text-input font-mono"
          type="text"
          placeholder="v1.0.0"
          bind:value={name}
          spellcheck="false"
          autocomplete="off"
          onkeydown={(e) => {
            if (e.key === "Enter" && canCreate) {
              e.preventDefault();
              handleCreate();
            }
          }}
        />
      </label>

      <Checkbox bind:checked={annotated} label="Annotated tag (adds a message)" />

      {#if annotated}
        <label class="field">
          <span class="field-label text-strong text-sm">Message</span>
          <textarea
            class="text-input"
            rows="3"
            placeholder="Release 1.0.0"
            bind:value={message}
          ></textarea>
          {#if message.trim().length === 0}
            <span class="field-hint text-xs">A message is required for annotated tags.</span>
          {/if}
        </label>
      {/if}

      <Checkbox bind:checked={pushAfter} label="Push to origin after creating" />

      {#if errorMessage}
        <div class="error-banner text-sm" role="alert">{errorMessage}</div>
      {/if}
    </div>

    {#snippet footer()}
      <Button variant="default" onclick={handleClose} disabled={creating}>Cancel</Button>
      <Button
        variant="primary"
        onclick={handleCreate}
        loading={creating}
        disabled={!canCreate || creating}
        title={!canCreate ? "Enter a tag name (and a message for annotated tags)" : "Create tag"}
      >
        {creating ? "Creating…" : "Create tag"}
      </Button>
    {/snippet}
  </Modal>
{/if}

<style>
  .tag-form {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .target-line {
    line-height: 1.4;
  }

  .target-label {
    background-color: var(--bg-muted);
    padding: 1px 5px;
    border-radius: var(--r-sm);
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
    color: var(--warning, #9a6700);
  }

  .text-input {
    width: 100%;
    box-sizing: border-box;
    min-height: 28px;
    padding: 5px 8px;
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    background-color: var(--bg);
    color: var(--text);
    font-size: 12.5px;
    outline: none;
    resize: vertical;
  }
  .text-input:focus {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px var(--accent-bg);
  }

  .error-banner {
    padding: 8px 12px;
    background-color: var(--danger-subtle, rgba(218, 54, 51, 0.1));
    border: 1px solid var(--danger);
    border-radius: var(--r-md);
    color: var(--danger);
  }
</style>
