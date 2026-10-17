<script lang="ts">
  import type { Snippet } from "svelte";

  type IconButtonSize = "sm" | "md" | "lg" | "small" | "default";
  type IconButtonVariant = "ghost" | "default" | "subtle" | "danger";

  interface Props {
    title?: string;
    ariaLabel?: string;
    size?: IconButtonSize;
    variant?: IconButtonVariant;
    active?: boolean;
    disabled?: boolean;
    class?: string;
    onclick?: (e: MouseEvent) => void;
    children?: Snippet;
  }

  let {
    title = "",
    ariaLabel = "",
    size = "md",
    variant = "ghost",
    active = false,
    disabled = false,
    class: className = "",
    onclick,
    children,
  }: Props = $props();

  let normalizedSize = $derived(
    size === "small" || size === "sm" ? "sm" : size === "lg" ? "lg" : "md"
  );
</script>

<button
  type="button"
  class="relay-icon-btn {normalizedSize} {variant} {active ? 'active' : ''} {className}"
  {title}
  {disabled}
  {onclick}
  aria-label={ariaLabel || title}
>
  {@render children?.()}
</button>

<style>
  .relay-icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--r-md);
    background-color: transparent;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    transition: background-color 80ms ease, color 80ms ease, border-color 80ms ease;
    user-select: none;
    box-sizing: border-box;
    flex-shrink: 0;
    position: relative;
    padding: 0;
  }

  /* Sizes */
  .relay-icon-btn.sm {
    width: 24px;
    height: 24px;
  }

  .relay-icon-btn.md {
    width: 28px;
    height: 28px;
  }

  .relay-icon-btn.lg {
    width: 32px;
    height: 32px;
  }

  /* Variants */
  .relay-icon-btn.ghost:hover:not(:disabled) {
    background-color: rgba(0, 0, 0, 0.05);
    color: var(--text);
  }

  .relay-icon-btn.default {
    background-color: var(--bg);
    border: 1px solid var(--border);
  }
  .relay-icon-btn.default:hover:not(:disabled) {
    background-color: var(--bg-muted);
    color: var(--text);
  }

  .relay-icon-btn.subtle {
    background-color: var(--bg-subtle);
  }
  .relay-icon-btn.subtle:hover:not(:disabled) {
    background-color: var(--bg-muted);
    color: var(--text);
  }

  .relay-icon-btn.danger:hover:not(:disabled) {
    background-color: rgba(207, 34, 46, 0.12);
    color: var(--danger);
  }

  /* Active state */
  .relay-icon-btn.active {
    background-color: #dedfe0;
    color: #494a4c;
  }

  .relay-icon-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
</style>
