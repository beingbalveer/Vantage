<script lang="ts">
  import type { TagInfo } from "../../types";
  import { openTagDetailsDialog } from "../../stores/ui";
  import Icon from "../common/Icon.svelte";

  interface Props {
    tag: TagInfo;
    repoId: string;
    compact?: boolean;
  }

  let { tag, repoId, compact = false }: Props = $props();

  function handleClick(e: MouseEvent) {
    // Keep the click from also selecting the commit row underneath.
    e.stopPropagation();
    openTagDetailsDialog({
      repoId,
      name: tag.name,
      annotated: tag.annotated,
      message: tag.message,
      date: tag.date,
      target: tag.target,
    });
  }

  let title = $derived(
    `${tag.name} · ${tag.annotated ? "annotated" : "lightweight"} tag${
      tag.message ? ` — ${tag.message}` : ""
    }`
  );
</script>

<button
  type="button"
  class="tag-chip {compact ? 'compact' : ''}"
  {title}
  onclick={handleClick}
  aria-label="View tag {tag.name}"
>
  <Icon name="tag" size={compact ? 10 : 11} strokeWidth={2} />
  <span class="tag-name">{tag.name}</span>
</button>

<style>
  .tag-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    max-width: 160px;
    height: 18px;
    padding: 0 6px;
    border-radius: 9px;
    border: 1px solid var(--border);
    background-color: var(--bg-subtle);
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 500;
    line-height: 1;
    cursor: pointer;
    white-space: nowrap;
    overflow: hidden;
    flex-shrink: 0;
  }
  .tag-chip:hover {
    background-color: var(--accent-bg);
    color: var(--accent);
    border-color: var(--accent);
  }
  .tag-chip.compact {
    max-width: 110px;
    height: 16px;
    padding: 0 5px;
    font-size: 10px;
  }
  .tag-name {
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
