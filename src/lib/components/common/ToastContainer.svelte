<script lang="ts">
  import { toasts, removeToast, pauseToastTimer, resumeToastTimer } from "../../stores/ui";
  import Icon from "./Icon.svelte";

  let toastList = $derived($toasts);
</script>

<div class="toast-container" role="status" aria-live="polite">
  {#each toastList as toast (toast.id)}
    <div
      class="toast-item {toast.kind || 'info'}"
      role="alert"
      tabindex="-1"
      onmouseenter={() => pauseToastTimer(toast.id)}
      onmouseleave={() => resumeToastTimer(toast.id)}
      onfocusin={() => pauseToastTimer(toast.id)}
      onfocusout={() => resumeToastTimer(toast.id)}
    >
      {#if toast.kind === "error"}
        <span class="error-dot"></span>
      {/if}
      <span class="toast-msg">{toast.message}</span>
      {#if toast.actionLabel && toast.action}
        <button type="button" class="toast-action" onclick={toast.action}>
          {toast.actionLabel}
        </button>
      {/if}
      {#if toast.linkLabel && toast.linkAction}
        <button type="button" class="toast-link" onclick={toast.linkAction}>
          {toast.linkLabel}
        </button>
      {/if}
      <button
        type="button"
        class="toast-close"
        onclick={() => removeToast(toast.id)}
        aria-label="Dismiss"
      >
        <Icon name="close" size={12} strokeWidth={2.5} />
      </button>
    </div>
  {/each}
</div>

<style>
  .toast-container {
    position: fixed;
    bottom: 40px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    flex-direction: column-reverse;
    gap: 8px;
    z-index: 300;
    pointer-events: none;
  }

  .toast-item {
    pointer-events: auto;
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 280px;
    max-width: 420px;
    padding: 8px 14px;
    background-color: var(--text);
    color: var(--bg);
    border-radius: 8px;
    box-shadow: var(--shadow-pop);
    font-size: 13px;
    line-height: 20px;
    animation: toast-in 160ms ease-out;
  }

  .error-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background-color: var(--danger);
    flex-shrink: 0;
  }

  .toast-msg {
    flex: 1;
    word-break: break-word;
  }

  .toast-action {
    background: none;
    border: none;
    color: var(--accent);
    font-weight: 600;
    font-size: 13px;
    cursor: pointer;
    padding: 0 4px;
  }
  .toast-action:hover {
    text-decoration: underline;
  }

  .toast-link {
    background: none;
    border: none;
    border-left: 1px solid currentColor;
    color: var(--bg);
    opacity: 0.85;
    font-weight: 600;
    font-size: 13px;
    cursor: pointer;
    padding: 0 4px 0 10px;
    margin-left: 2px;
  }
  .toast-link:hover {
    opacity: 1;
    text-decoration: underline;
  }

  .toast-close {
    background: none;
    border: none;
    color: var(--bg);
    opacity: 0.6;
    cursor: pointer;
    font-size: 12px;
  }
  .toast-close:hover {
    opacity: 1;
  }

  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
</style>
