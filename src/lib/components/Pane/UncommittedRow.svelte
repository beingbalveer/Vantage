<script lang="ts">
  import Icon from "../common/Icon.svelte";

  interface Props {
    dirtyCount: number;
    isSelected: boolean;
    onselect?: () => void;
    onclick?: () => void;
  }

  let { dirtyCount, isSelected, onselect, onclick }: Props = $props();

  let metaLabel = $derived(
    dirtyCount > 0
      ? `${dirtyCount} ${dirtyCount === 1 ? "file" : "files"} changed`
      : "Working directory clean"
  );

  function handleTrigger(e?: Event) {
    if (e) e.stopPropagation();
    if (onselect) onselect();
    else if (onclick) onclick();
  }
</script>

<div
  class="change-item {isSelected ? 'active' : ''}"
  data-uncommitted
  onclick={handleTrigger}
  onkeydown={(e) => {
    if (e.key === " ") {
      e.preventDefault();
      handleTrigger(e);
    }
  }}
  role="button"
  aria-pressed={isSelected}
  tabindex="0"
>
  <span class="change-icon"><Icon name="changes" size={15} /></span>
  <span class="commit-copy">
    <span class="commit-title" title="Uncommitted changes">Uncommitted changes</span>
    <span class="commit-meta">{metaLabel}</span>
  </span>
</div>

<style>
  .change-item {
    position: relative;
    display: flex;
    width: 100%;
    align-items: center;
    justify-content: flex-start;
    text-align: left;
    min-height: 60px;
    padding: 9px 17px 9px 41px;
    gap: 0;
    border-radius: 0;
    border: 0;
    border-bottom: 1px solid #dfe3e7;
    background: transparent;
    cursor: pointer;
    user-select: none;
  }

  .change-item:hover {
    background: #f5f7f8;
  }
  .change-item.active,
  .change-item.active:hover {
    background: var(--accent-bg);
  }
  .change-item.active::after {
    content: "";
    position: absolute;
    left: 0;
    top: 7px;
    bottom: 7px;
    width: 3px;
    border-radius: 0 3px 3px 0;
    background: var(--accent);
  }

  .change-icon {
    position: absolute;
    left: 15px;
    top: calc(50% - 7px);
    width: 15px;
    height: 15px;
    color: #687b68;
  }
  .change-icon :global(svg) {
    display: block;
  }

  .commit-copy {
    min-width: 0;
    flex: 1;
    overflow: hidden;
  }
  .commit-title {
    display: block;
    font-size: 14px;
    font-weight: 500;
    line-height: 1.5;
    color: #30343a;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .commit-meta {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 6px;
    font-size: 12px;
    line-height: 17px;
    color: var(--text-muted);
  }
</style>
