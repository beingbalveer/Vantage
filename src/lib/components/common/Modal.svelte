<script lang="ts">
  import type { Snippet } from "svelte";
  import { onMount, tick } from "svelte";
  import Icon from "./Icon.svelte";

  interface Props {
    open?: boolean;
    title?: string;
    description?: string;
    width?: string | number;
    closeOnEsc?: boolean;
    closeOnEscape?: boolean;
    closeOnBackdrop?: boolean;
    closeOnOutsideClick?: boolean;
    showCloseButton?: boolean;
    bodyPadding?: boolean;
    onclose?: () => void;
    children?: Snippet;
    footer?: Snippet;
  }

  let {
    open = $bindable(false),
    title = "",
    description = "",
    width = 440,
    closeOnEsc,
    closeOnEscape,
    closeOnBackdrop,
    closeOnOutsideClick,
    showCloseButton = true,
    bodyPadding = true,
    onclose,
    children,
    footer,
  }: Props = $props();

  let cardEl = $state<HTMLElement | null>(null);
  let previousActiveElement: HTMLElement | null = null;

  let canCloseOnEsc = $derived(
    closeOnEscape !== undefined
      ? closeOnEscape
      : closeOnEsc !== undefined
      ? closeOnEsc
      : true
  );
  let canCloseOnBackdrop = $derived(
    closeOnOutsideClick !== undefined
      ? closeOnOutsideClick
      : closeOnBackdrop !== undefined
      ? closeOnBackdrop
      : true
  );

  function handleClose() {
    open = false;
    onclose?.();
    if (previousActiveElement && typeof previousActiveElement.focus === "function") {
      previousActiveElement.focus();
    }
  }

  function handleBackdropClick(e: MouseEvent) {
    if (canCloseOnBackdrop && e.target === e.currentTarget) {
      handleClose();
    }
  }

  function getFocusableElements(): HTMLElement[] {
    if (!cardEl) return [];
    const selector =
      'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';
    return Array.from(cardEl.querySelectorAll<HTMLElement>(selector)).filter(
      (el) => el.offsetParent !== null
    );
  }

  function handleCardKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      if (canCloseOnEsc) {
        e.preventDefault();
        e.stopPropagation();
        handleClose();
      }
      return;
    }

    if (e.key === "Tab") {
      const focusable = getFocusableElements();
      if (focusable.length === 0) {
        e.preventDefault();
        return;
      }
      const first = focusable[0];
      const last = focusable[focusable.length - 1];

      if (e.shiftKey) {
        if (document.activeElement === first || document.activeElement === cardEl) {
          e.preventDefault();
          last.focus();
        }
      } else {
        if (document.activeElement === last) {
          e.preventDefault();
          first.focus();
        }
      }
    }
  }

  $effect(() => {
    if (open) {
      previousActiveElement = document.activeElement as HTMLElement;
      tick().then(() => {
        const focusable = getFocusableElements();
        if (focusable.length > 0) {
          // Focus the first interactive element (or input if available)
          const firstInput = focusable.find(
            (el) => el.tagName === "INPUT" || el.tagName === "TEXTAREA"
          );
          (firstInput || focusable[0]).focus();
        } else if (cardEl) {
          cardEl.focus();
        }
      });
    }
  });

  let cardStyle = $derived(
    typeof width === "number" ? `width: ${width}px;` : `width: ${width};`
  );
</script>

{#if open}
  <div
    class="relay-modal-backdrop"
    role="presentation"
    onclick={handleBackdropClick}
  >
    <div
      bind:this={cardEl}
      class="relay-modal-card"
      style={cardStyle}
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={handleCardKeydown}
    >
      {#if title || showCloseButton}
        <div class="relay-modal-header">
          <div class="relay-modal-header-text">
            {#if title}
              <h2 class="relay-modal-title">{title}</h2>
            {/if}
            {#if description}
              <p class="relay-modal-desc">{description}</p>
            {/if}
          </div>

          {#if showCloseButton}
            <button
              type="button"
              class="relay-modal-close"
              onclick={handleClose}
              title="Close (Esc)"
              aria-label="Close"
            >
              <Icon name="close" size={14} />
            </button>
          {/if}
        </div>
      {/if}

      <div class="relay-modal-body {bodyPadding ? 'padded' : 'flush'}">
        {@render children?.()}
      </div>

      {#if footer}
        <div class="relay-modal-footer">
          {@render footer()}
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .relay-modal-backdrop {
    position: fixed;
    inset: 0;
    background-color: rgba(0, 0, 0, 0.45);
    backdrop-filter: blur(2px);
    z-index: 200;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 20px;
    animation: fadeIn 100ms ease;
  }

  .relay-modal-card {
    max-width: 90vw;
    max-height: 85vh;
    background-color: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--r-lg);
    box-shadow: var(--shadow-pop);
    display: flex;
    flex-direction: column;
    outline: none;
    overflow: hidden;
    animation: scaleIn 120ms cubic-bezier(0.16, 1, 0.3, 1);
  }

  .relay-modal-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    padding: 18px 20px 14px;
    gap: 12px;
    border-bottom: 1px solid var(--border-subtle);
  }

  .relay-modal-header-text {
    flex: 1;
    min-width: 0;
  }

  .relay-modal-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--text);
    margin: 0;
    line-height: 20px;
  }

  .relay-modal-desc {
    font-size: 12px;
    color: var(--text-muted);
    margin: 4px 0 0;
    line-height: 16px;
  }

  .relay-modal-close {
    width: 24px;
    height: 24px;
    padding: 0;
    border: none;
    background: transparent;
    color: var(--text-muted);
    border-radius: var(--r-sm);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    flex-shrink: 0;
    transition: background-color 80ms ease, color 80ms ease;
  }
  .relay-modal-close:hover {
    background-color: var(--bg-muted);
    color: var(--text);
  }

  .relay-modal-body {
    overflow-y: auto;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .relay-modal-body.padded {
    padding: 20px;
    gap: 16px;
  }
  .relay-modal-body.flush {
    padding: 0;
    gap: 0;
  }

  .relay-modal-footer {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    padding: 14px 20px;
    background-color: var(--bg-subtle);
    border-top: 1px solid var(--border-subtle);
  }

  @keyframes fadeIn {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  @keyframes scaleIn {
    from { transform: scale(0.97); opacity: 0; }
    to { transform: scale(1); opacity: 1; }
  }
</style>
