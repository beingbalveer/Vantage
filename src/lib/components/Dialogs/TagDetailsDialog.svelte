<script lang="ts">
  import * as api from "../../platform/api";
  import {
    tagDetailsRequest,
    closeTagDetailsDialog,
    addToast,
  } from "../../app/ui";
  import { formatRelativeTime, formatFullTimestamp } from "../../domain/format";
  import { Modal, Button } from "../common";

  let request = $derived($tagDetailsRequest);
  let pushing = $state(false);
  let errorMessage = $state<string | null>(null);

  let isOpen = $derived(!!request);
  let shortTarget = $derived(request ? request.target.slice(0, 7) : "");

  function handleClose() {
    if (pushing) return;
    errorMessage = null;
    closeTagDetailsDialog();
  }

  async function handlePush() {
    if (!request || pushing) return;
    pushing = true;
    errorMessage = null;
    try {
      await api.tagPush(request.repoId, request.name);
      addToast({
        message: `Pushed tag ${request.name} to origin`,
        kind: "success",
      });
      closeTagDetailsDialog();
    } catch (e: any) {
      errorMessage = e.message || `Failed to push tag ${request.name}`;
    } finally {
      pushing = false;
    }
  }
</script>

{#if isOpen && request}
  <Modal
    open={isOpen}
    title="Tag {request.name}"
    width="460px"
    closeOnEscape={!pushing}
    closeOnOutsideClick={!pushing}
    onclose={handleClose}
  >
    <div class="tag-details">
      <div class="detail-grid">
        <span class="detail-key text-sm text-muted">Type</span>
        <span class="detail-val text-sm">{request.annotated ? "Annotated" : "Lightweight"}</span>

        <span class="detail-key text-sm text-muted">Commit</span>
        <span class="detail-val font-mono text-sm" title={request.target}>{shortTarget}</span>

        {#if request.date}
          <span class="detail-key text-sm text-muted">Date</span>
          <span class="detail-val text-sm" title={formatFullTimestamp(request.date)}>
            {formatRelativeTime(request.date)}
          </span>
        {/if}
      </div>

      {#if request.annotated && request.message}
        <div class="message-block">
          <span class="detail-key text-sm text-muted">Message</span>
          <pre class="tag-message text-sm">{request.message}</pre>
        </div>
      {/if}

      {#if errorMessage}
        <div class="error-banner text-sm" role="alert">{errorMessage}</div>
      {/if}
    </div>

    {#snippet footer()}
      <Button variant="default" onclick={handleClose} disabled={pushing}>Close</Button>
      <Button
        variant="primary"
        onclick={handlePush}
        loading={pushing}
        disabled={pushing}
        title="Push this tag to origin"
      >
        {pushing ? "Pushing…" : "Push to origin"}
      </Button>
    {/snippet}
  </Modal>
{/if}

<style>
  .tag-details {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .detail-grid {
    display: grid;
    grid-template-columns: 80px 1fr;
    row-gap: 8px;
    column-gap: 10px;
    align-items: baseline;
  }

  .detail-val {
    color: var(--text);
    word-break: break-all;
  }

  .message-block {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .tag-message {
    margin: 0;
    padding: 8px 10px;
    background-color: var(--bg-subtle);
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-md);
    color: var(--text);
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 160px;
    overflow-y: auto;
    font-family: var(--font-ui);
    line-height: 1.45;
  }

  .error-banner {
    padding: 8px 12px;
    background-color: var(--danger-subtle, rgba(218, 54, 51, 0.1));
    border: 1px solid var(--danger);
    border-radius: var(--r-md);
    color: var(--danger);
  }
</style>
