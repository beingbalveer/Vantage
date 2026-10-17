<script lang="ts">
  import type { Snippet } from "svelte";

  type ButtonVariant = "primary" | "default" | "ghost" | "danger" | "subtle";
  type ButtonSize = "sm" | "md" | "lg" | "small" | "default";

  interface Props {
    variant?: ButtonVariant;
    size?: ButtonSize;
    disabled?: boolean;
    loading?: boolean;
    fullWidth?: boolean;
    type?: "button" | "submit" | "reset";
    title?: string;
    class?: string;
    onclick?: (e: MouseEvent) => void;
    children?: Snippet;
  }

  let {
    variant = "default",
    size = "md",
    disabled = false,
    loading = false,
    fullWidth = false,
    type = "button",
    title = "",
    class: className = "",
    onclick,
    children,
  }: Props = $props();

  let normalizedSize = $derived(
    size === "small" || size === "sm" ? "sm" : size === "lg" ? "lg" : "md"
  );
</script>

<button
  {type}
  class="relay-btn {variant} {normalizedSize} {fullWidth ? 'full-width' : ''} {loading ? 'loading' : ''} {className}"
  disabled={disabled || loading}
  {title}
  {onclick}
>
  {@render children?.()}
</button>

<style>
  .relay-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    font-family: var(--font-ui);
    font-weight: 500;
    border-radius: var(--r-md);
    cursor: pointer;
    border: 1px solid transparent;
    transition: background-color 80ms ease, border-color 80ms ease, filter 80ms ease, opacity 80ms ease;
    white-space: nowrap;
    user-select: none;
    box-sizing: border-box;
  }

  /* Sizes */
  .relay-btn.sm {
    height: 24px;
    padding: 0 8px;
    font-size: 11px;
  }

  .relay-btn.md {
    height: 28px;
    padding: 0 12px;
    font-size: 12px;
  }

  .relay-btn.lg {
    height: 34px;
    padding: 0 16px;
    font-size: 13px;
  }

  .relay-btn.full-width {
    width: 100%;
  }

  /* Variants */
  .relay-btn.primary {
    background-color: var(--accent);
    color: var(--accent-fg);
  }
  .relay-btn.primary:hover:not(:disabled) {
    filter: brightness(0.93);
  }

  .relay-btn.default {
    background-color: var(--bg);
    border-color: var(--border);
    color: var(--text);
  }
  .relay-btn.default:hover:not(:disabled) {
    background-color: var(--bg-muted);
  }

  .relay-btn.subtle {
    background-color: var(--bg-subtle);
    border-color: var(--border-subtle);
    color: var(--text);
  }
  .relay-btn.subtle:hover:not(:disabled) {
    background-color: var(--bg-muted);
  }

  .relay-btn.ghost {
    background-color: transparent;
    color: var(--text-muted);
  }
  .relay-btn.ghost:hover:not(:disabled) {
    background-color: var(--bg-muted);
    color: var(--text);
  }

  .relay-btn.danger {
    background-color: var(--danger);
    color: #ffffff;
  }
  .relay-btn.danger:hover:not(:disabled) {
    filter: brightness(0.92);
  }

  .relay-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .relay-btn.loading {
    cursor: wait;
    opacity: 0.75;
  }
</style>
