<script lang="ts">
  import type { Snippet } from "svelte";

  type BadgeVariant = "neutral" | "info" | "success" | "warning" | "danger" | "accent";
  type BadgeSize = "sm" | "md";

  interface Props {
    variant?: BadgeVariant;
    size?: BadgeSize;
    class?: string;
    children?: Snippet;
  }

  let {
    variant = "neutral",
    size = "sm",
    class: className = "",
    children,
  }: Props = $props();
</script>

<span class="relay-badge {variant} {size} {className}">
  {@render children?.()}
</span>

<style>
  .relay-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-family: var(--font-ui);
    font-weight: 600;
    line-height: 1;
    border-radius: 9999px;
    white-space: nowrap;
    user-select: none;
    letter-spacing: 0.02em;
  }

  .relay-badge.sm {
    height: 16px;
    padding: 0 6px;
    font-size: 10px;
  }

  .relay-badge.md {
    height: 20px;
    padding: 0 8px;
    font-size: 11px;
  }

  /* Variants */
  .relay-badge.neutral {
    background-color: var(--bg-muted);
    color: var(--text-muted);
  }

  .relay-badge.accent {
    background-color: var(--accent-bg);
    color: var(--accent);
  }

  .relay-badge.info {
    background-color: rgba(9, 105, 218, 0.12);
    color: var(--accent);
  }

  .relay-badge.success {
    background-color: rgba(26, 127, 55, 0.14);
    color: var(--success);
  }

  .relay-badge.warning {
    background-color: var(--warning-bg);
    color: var(--warning);
  }

  .relay-badge.danger {
    background-color: rgba(207, 34, 46, 0.14);
    color: var(--danger);
  }
</style>
