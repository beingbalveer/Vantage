<script lang="ts">
  // Skeleton placeholder for a pending list or diff load (DESIGN §7.7).
  // Rows use fixed heights matching the real rows so revealing content does
  // not shift the layout. The pulse is static under prefers-reduced-motion.
  interface Props {
    rows?: number;
    height?: number;
    shape?: "commit" | "file" | "line";
    label?: string;
  }

  let { rows = 6, height = 60, shape = "line", label = "Loading" }: Props = $props();

  const WIDTHS = [72, 54, 63, 45, 68, 50, 58, 47];
</script>

<div class="skeleton" role="status" aria-busy="true" aria-label={label}>
  {#each Array(rows) as _, i (i)}
    <div class="sk-row" style="height: {height}px;">
      {#if shape === "commit"}
        <span class="sk-dot"></span>
        <span class="sk-stack">
          <span class="sk-bar" style="width: {WIDTHS[i % WIDTHS.length]}%;"></span>
          <span class="sk-bar sk-bar-sm" style="width: {24 + ((i * 7) % 26)}%;"></span>
        </span>
      {:else if shape === "file"}
        <span class="sk-bar" style="width: {40 + ((i * 11) % 45)}%;"></span>
        <span class="sk-bar sk-bar-sm sk-bar-trail" style="width: 22px;"></span>
      {:else}
        <span class="sk-bar" style="width: {30 + ((i * 17) % 55)}%;"></span>
      {/if}
    </div>
  {/each}
</div>

<style>
  .skeleton {
    display: flex;
    flex-direction: column;
    width: 100%;
  }

  .sk-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 0 16px;
    box-sizing: border-box;
    border-bottom: 1px solid var(--border-subtle);
  }

  .sk-dot {
    flex-shrink: 0;
    width: 11px;
    height: 11px;
    border-radius: var(--r-sm);
    background-color: var(--bg-muted);
    animation: sk-pulse 1.4s ease-in-out infinite;
  }

  .sk-stack {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }

  .sk-bar {
    display: block;
    height: 10px;
    border-radius: var(--r-sm);
    background-color: var(--bg-muted);
    animation: sk-pulse 1.4s ease-in-out infinite;
  }

  .sk-bar-sm {
    height: 8px;
    opacity: 0.7;
  }

  .sk-bar-trail {
    margin-left: auto;
    flex-shrink: 0;
  }

  @keyframes sk-pulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.5;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .sk-bar,
    .sk-dot {
      animation: none;
    }
  }
</style>
