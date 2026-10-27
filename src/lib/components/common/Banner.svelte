<script lang="ts">
  // Inline, persistent error banner (DESIGN §7.8 / §6.4). Lives next to the
  // failure so it survives a dismissed toast, with a Retry action and an
  // on-demand details block instead of a permanent wall of stderr.
  import Icon from "./Icon.svelte";
  import type { NormalizedError } from "../../domain/asyncState";

  interface Props {
    kind?: "warning" | "error" | "info";
    message: string;
    error?: NormalizedError | null;
    retryLabel?: string;
    onretry?: () => void;
    dismissible?: boolean;
    ondismiss?: () => void;
  }

  let {
    kind = "warning",
    message,
    error = null,
    retryLabel = "",
    onretry,
    dismissible = false,
    ondismiss,
  }: Props = $props();

  let showDetails = $state(false);
  let copied = $state(false);

  let detailText = $derived(
    [error?.code && error.code !== "Unknown" ? `code: ${error.code}` : "", error?.stderr || ""]
      .filter(Boolean)
      .join("\n\n")
  );

  async function copyDetails() {
    const text = [message, detailText].filter(Boolean).join("\n\n");
    try {
      await navigator.clipboard.writeText(text);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      // Clipboard access can be denied; the text stays selectable in the block.
    }
  }
</script>

<div
  class="banner {kind}"
  role={kind === "error" ? "alert" : "status"}
>
  <span class="banner-icon">
    <Icon name={kind === "info" ? "message" : "flag"} size={14} strokeWidth={1.8} />
  </span>

  <div class="banner-body">
    <span class="banner-message">{message}</span>

    {#if showDetails}
      <div class="banner-details font-mono text-xs">
        {#if detailText}
          <pre>{detailText}</pre>
        {:else}
          <span class="text-muted">No additional details.</span>
        {/if}
        <div class="banner-details-actions">
          <button
            type="button"
            class="link-btn icon-only"
            onclick={copyDetails}
            title={copied ? "Copied" : "Copy details"}
            aria-label={copied ? "Copied" : "Copy details"}
          >
            {#if copied}
              <Icon name="check" size={12} strokeWidth={2.2} />
            {:else}
              <Icon name="copy" size={12} strokeWidth={2.2} />
            {/if}
          </button>
        </div>
      </div>
    {/if}
  </div>

  <div class="banner-actions">
    {#if onretry && retryLabel}
      <button type="button" class="banner-btn" onclick={onretry}>{retryLabel}</button>
    {/if}
    {#if detailText}
      <button
        type="button"
        class="link-btn"
        aria-expanded={showDetails}
        onclick={() => (showDetails = !showDetails)}
      >
        {showDetails ? "Hide details" : "Details"}
      </button>
    {/if}
    {#if dismissible}
      <button
        type="button"
        class="link-btn icon-only"
        aria-label="Dismiss"
        onclick={ondismiss}
      >
        <Icon name="close" size={12} strokeWidth={2.2} />
      </button>
    {/if}
  </div>
</div>

<style>
  .banner {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border-subtle);
    background-color: var(--warning-bg);
    color: var(--text);
  }
  .banner.error {
    background-color: var(--danger-bg);
  }
  .banner.info {
    background-color: var(--bg-subtle);
  }

  .banner-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 18px;
    flex-shrink: 0;
    color: var(--warning);
  }
  .banner.error .banner-icon {
    color: var(--danger);
  }
  .banner.info .banner-icon {
    color: var(--text-muted);
  }

  .banner-body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .banner-message {
    font-size: 12px;
    line-height: 17px;
    word-break: break-word;
  }

  .banner-details {
    background-color: var(--bg);
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-sm);
    padding: 6px 8px;
  }
  .banner-details pre {
    margin: 0;
    max-height: 180px;
    overflow: auto;
    white-space: pre-wrap;
    word-break: break-word;
    user-select: text;
    color: var(--text-muted);
  }
  .banner-details-actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 4px;
  }

  .banner-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .banner-btn {
    height: 22px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background-color: var(--bg);
    color: var(--text);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    white-space: nowrap;
  }
  .banner-btn:hover {
    background-color: var(--bg-muted);
  }

  .link-btn {
    background: none;
    border: none;
    padding: 0 2px;
    color: var(--accent);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    white-space: nowrap;
  }
  .link-btn:hover {
    text-decoration: underline;
  }
  .link-btn.icon-only {
    color: var(--text-muted);
    display: flex;
    align-items: center;
  }
</style>
