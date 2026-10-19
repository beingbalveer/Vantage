<script lang="ts">
  import type { CommitSummary, TagInfo } from "../../types";
  import { formatRelativeTime } from "../../format";
  import Icon from "../common/Icon.svelte";
  import TagChip from "./TagChip.svelte";

  interface Props {
    commit: CommitSummary;
    isSelected: boolean;
    isLocal?: boolean;
    repoId?: string;
    tags?: TagInfo[];
    onselect?: () => void;
    onclick?: () => void;
  }

  let {
    commit,
    isSelected,
    isLocal = false,
    repoId = "",
    tags = [],
    onselect,
    onclick,
  }: Props = $props();

  let isMerge = $derived(commit.parents && commit.parents.length > 1);
  let visibleTags = $derived(tags.slice(0, 2));
  let hiddenTagCount = $derived(Math.max(0, tags.length - visibleTags.length));

  function handleTrigger(e?: Event) {
    if (e) e.stopPropagation();
    if (onselect) onselect();
    else if (onclick) onclick();
  }
</script>

<div
  class="commit-item {isSelected ? 'active' : ''} {commit.isNew ? 'is-new' : 'is-reviewed'} {isMerge ? 'merge-commit' : ''} {isLocal ? 'is-local' : ''}"
  onclick={handleTrigger}
  onkeydown={(e) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      handleTrigger(e);
    }
  }}
  role="button"
  aria-pressed={isSelected}
  tabindex="0"
  data-sha={commit.sha}
>
  <span class="commit-node"></span>
  <span class="commit-copy">
    <span class="commit-title" title={commit.subject}>{commit.subject}</span>
    <span class="commit-meta">
      <span class="author">{commit.authorName}</span>
      <span aria-hidden="true">·</span>
      <span class="time">{formatRelativeTime(commit.date)}</span>
    </span>
  </span>
  <div class="commit-trailing">
    {#if visibleTags.length > 0}
      <span class="tag-chips">
        {#each visibleTags as tag (tag.name)}
          <TagChip {tag} {repoId} compact />
        {/each}
        {#if hiddenTagCount > 0}
          <span class="more-tags" title="{hiddenTagCount} more tag{hiddenTagCount === 1 ? '' : 's'}">
            +{hiddenTagCount}
          </span>
        {/if}
      </span>
    {/if}
    {#if isMerge}
      <span class="merge-badge">merge</span>
    {/if}
    {#if isLocal}
      <span class="unpushed-icon" title="Unpushed commit (local)">
        <Icon name="arrow-up" size={13} strokeWidth={2.4} />
      </span>
    {/if}
  </div>
</div>

<style>
  .commit-item {
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

  .commit-item:hover {
    background: #f5f7f8;
  }
  .commit-item.active,
  .commit-item.active:hover {
    background: var(--accent-bg);
  }

  /* Continuous timeline hairline. */
  .commit-item::before {
    content: "";
    position: absolute;
    width: 1px;
    left: 22px;
    top: 0;
    bottom: 0;
    background: #d3dae2;
  }
  .commit-item.active::after {
    content: "";
    position: absolute;
    left: 0;
    top: 7px;
    bottom: 7px;
    width: 3px;
    border-radius: 0 3px 3px 0;
    background: var(--accent);
  }

  .commit-node {
    position: absolute;
    left: 17px;
    top: calc(50% - 5px);
    width: 11px;
    height: 11px;
    margin: 0;
    border: 1.5px solid #8996a3;
    border-radius: 50%;
    background: #fff;
    z-index: 1;
  }
  .is-new .commit-node {
    background: var(--accent);
    border-color: var(--accent);
  }
  .merge-commit .commit-node {
    border-radius: 2px;
    transform: rotate(45deg);
  }

  .commit-copy {
    min-width: 0;
    flex: 1;
    overflow: hidden;
  }
  .commit-title {
    display: block;
    font-size: 14px;
    font-weight: 400;
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
    color: #737982;
  }
  .commit-meta .author {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 45%;
  }
  .commit-meta .time {
    white-space: nowrap;
  }
  .commit-trailing {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-left: auto;
    flex-shrink: 0;
  }
  .tag-chips {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .more-tags {
    font-size: 10px;
    color: var(--text-faint, #8c959f);
    white-space: nowrap;
  }
  .merge-badge {
    flex-shrink: 0;
    padding: 3px 6px;
    border-radius: 5px;
    background: #edf0f3;
    color: #929aa3;
    font-size: 10px;
  }
  .unpushed-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    border-radius: 4px;
    color: #636b74;
    background: transparent;
    flex-shrink: 0;
  }
  :global([data-theme="dark"]) .unpushed-icon {
    color: #8b949e;
  }
  .commit-item.active .unpushed-icon {
    color: var(--accent, #2563eb);
  }

  /* Prototype compact override (approved-prototype.html · @media max-width:1400px). */
  @media (max-width: 1400px) {
    .commit-title {
      font-size: 12.5px;
    }
  }
</style>
