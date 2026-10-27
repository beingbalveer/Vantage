<script lang="ts">
  import type { RepoStatus, OperationKind } from "../../platform/types";
  import * as api from "../../platform/api";
  import { addToast } from "../../app/ui";
  import { Modal, Button } from "../common";
  import Icon from "../common/Icon.svelte";

  interface Props {
    repoId: string;
    status: RepoStatus | null;
    /** Called after continue/abort succeeds so the pane can refresh. */
    onresolved?: () => void;
  }

  let { repoId, status, onresolved }: Props = $props();

  const OPERATION_LABEL: Record<OperationKind, string> = {
    merge: "merge",
    rebase: "rebase",
    cherryPick: "cherry-pick",
    revert: "revert",
  };

  let operation = $derived<OperationKind | null>(status?.operation ?? null);
  let conflictedPaths = $derived(status?.conflictedPaths ?? []);
  let conflictedCount = $derived(status?.conflictedCount ?? conflictedPaths.length);
  let opLabel = $derived(operation ? OPERATION_LABEL[operation] : "");
  let visible = $derived(Boolean(operation) || conflictedCount > 0);

  let title = $derived(
    operation ? `${capitalize(opLabel)} in progress` : "Unresolved conflicts"
  );
  let detail = $derived.by(() => {
    if (conflictedCount > 0) {
      return `${conflictedCount} file${conflictedCount === 1 ? "" : "s"} ${
        conflictedCount === 1 ? "has" : "have"
      } conflicts. Resolve and stage them, then continue.`;
    }
    if (operation) {
      return `All conflicts resolved. Continue to finish the ${opLabel}.`;
    }
    return "Resolve the conflicts in the working tree before committing.";
  });

  function capitalize(s: string): string {
    return s ? s.charAt(0).toUpperCase() + s.slice(1) : s;
  }

  let dialog = $state<null | "continue" | "abort">(null);
  let busy = $state(false);
  let errorMessage = $state<string | null>(null);

  const PREVIEW_LIMIT = 4;
  let previewPaths = $derived(conflictedPaths.slice(0, PREVIEW_LIMIT));
  let hiddenCount = $derived(Math.max(0, conflictedPaths.length - PREVIEW_LIMIT));

  function openDialog(kind: "continue" | "abort") {
    errorMessage = null;
    dialog = kind;
  }

  function closeDialog() {
    if (busy) return;
    dialog = null;
    errorMessage = null;
  }

  async function openInEditor() {
    try {
      await api.openExternalEditor(repoId, conflictedPaths[0] ?? "", 1);
    } catch (e: any) {
      addToast({ message: e.message || "Could not open editor", kind: "error" });
    }
  }

  async function confirmContinue() {
    if (busy) return;
    busy = true;
    errorMessage = null;
    try {
      await api.operationContinue(repoId);
      addToast({ message: `${capitalize(opLabel)} continued`, kind: "success" });
      dialog = null;
      onresolved?.();
    } catch (e: any) {
      errorMessage = e.message || `Failed to continue the ${opLabel}`;
    } finally {
      busy = false;
    }
  }

  async function confirmAbort() {
    if (busy) return;
    busy = true;
    errorMessage = null;
    try {
      await api.operationAbort(repoId);
      addToast({ message: `${capitalize(opLabel)} aborted`, kind: "success" });
      dialog = null;
      onresolved?.();
    } catch (e: any) {
      errorMessage = e.message || `Failed to abort the ${opLabel}`;
    } finally {
      busy = false;
    }
  }
</script>

{#if visible}
  <div class="conflict-banner" role="alert" aria-label="{title}: {detail}">
    <span class="banner-icon" aria-hidden="true">⚠</span>
    <div class="banner-text">
      <div class="banner-title text-strong">{title}</div>
      <div class="banner-detail text-sm">{detail}</div>
      {#if previewPaths.length > 0}
        <ul class="banner-files" aria-label="Files with conflicts">
          {#each previewPaths as path (path)}
            <li class="font-mono">{path}</li>
          {/each}
          {#if hiddenCount > 0}
            <li class="text-muted">and {hiddenCount} more</li>
          {/if}
        </ul>
      {/if}
    </div>
    <div class="banner-actions">
      <Button variant="default" size="sm" onclick={openInEditor} title="Open a conflicted file in your editor">
        <Icon name="external-link" size={13} />
        Open in editor
      </Button>
      {#if operation}
        <Button
          variant="primary"
          size="sm"
          onclick={() => openDialog("continue")}
          title="Finish the {opLabel}"
        >
          <Icon name="check" size={13} />
          Continue {opLabel}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          class="abort-btn"
          onclick={() => openDialog("abort")}
          title="Abandon the {opLabel}"
        >
          Abort
        </Button>
      {/if}
    </div>
  </div>
{/if}

{#if operation && dialog === "continue"}
  <Modal
    open={true}
    title="Continue {opLabel}?"
    width="440px"
    closeOnEscape
    onclose={closeDialog}
  >
    <p class="text-base text-muted">
      This finishes the in-progress {opLabel}.
      {#if operation === "merge"}
        A merge commit will be created with the prepared message.
      {/if}
    </p>
    {#if conflictedCount > 0}
      <div class="dialog-warning text-sm" role="alert">
        {conflictedCount} file{conflictedCount === 1 ? " still has" : "s still have"} conflicts.
        Resolve and stage them first, or the {opLabel} cannot continue.
      </div>
    {/if}
    {#if errorMessage}
      <pre class="dialog-error text-xs font-mono">{errorMessage}</pre>
    {/if}

    {#snippet footer()}
      <Button variant="default" onclick={closeDialog} disabled={busy}>Cancel</Button>
      <Button variant="primary" onclick={confirmContinue} loading={busy}>
        {busy ? "Continuing…" : `Continue ${opLabel}`}
      </Button>
    {/snippet}
  </Modal>
{/if}

{#if operation && dialog === "abort"}
  <Modal
    open={true}
    title="Abort {opLabel}?"
    width="440px"
    closeOnEscape
    onclose={closeDialog}
  >
    <p class="text-base text-muted">
      This abandons the in-progress {opLabel} and returns the repository to the state
      before it started.
    </p>
    <div class="dialog-warning danger text-sm" role="alert">
      {#if conflictedCount > 0}
        Your work resolving {conflictedCount} conflicted file{conflictedCount === 1 ? "" : "s"}{#if operation === "merge"} — including any edits made to resolve them{/if} — will be discarded.
      {:else if operation === "cherryPick" || operation === "revert"}
        The commit being applied will be discarded.
      {:else}
        Any changes made for this {opLabel} will be discarded.
      {/if}
    </div>
    {#if errorMessage}
      <pre class="dialog-error text-xs font-mono">{errorMessage}</pre>
    {/if}

    {#snippet footer()}
      <Button variant="default" onclick={closeDialog} disabled={busy}>Cancel</Button>
      <Button variant="danger" onclick={confirmAbort} loading={busy}>
        {busy ? "Aborting…" : `Abort ${opLabel}`}
      </Button>
    {/snippet}
  </Modal>
{/if}

<style>
  .conflict-banner {
    flex-shrink: 0;
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 14px;
    background-color: var(--warning-bg);
    border-bottom: 1px solid var(--border);
    color: var(--text);
  }

  .banner-icon {
    font-size: 15px;
    line-height: 18px;
    color: var(--warning);
    flex-shrink: 0;
  }

  .banner-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .banner-title {
    color: var(--text);
  }

  .banner-detail {
    color: var(--text-muted);
  }

  .banner-files {
    list-style: none;
    margin: 4px 0 0;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
  }
  .banner-files li {
    font-size: 11.5px;
    color: var(--text-muted);
    max-width: 320px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .banner-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    flex-wrap: wrap;
    gap: 6px;
    flex-shrink: 0;
  }
  .banner-actions :global(.abort-btn) {
    color: var(--danger);
  }

  .dialog-warning {
    padding: 8px 12px;
    background-color: var(--warning-bg);
    border: 1px solid var(--warning);
    border-radius: var(--r-md);
    color: var(--text);
  }
  .dialog-warning.danger {
    background-color: var(--danger-subtle, rgba(218, 54, 51, 0.1));
    border-color: var(--danger);
    color: var(--danger);
  }

  .dialog-error {
    margin: 0;
    max-height: 200px;
    overflow: auto;
    white-space: pre-wrap;
    word-break: break-word;
    padding: 8px 12px;
    background-color: var(--danger-subtle, rgba(218, 54, 51, 0.1));
    border: 1px solid var(--danger);
    border-radius: var(--r-md);
    color: var(--danger);
  }
</style>
