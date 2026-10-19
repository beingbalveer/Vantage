<script lang="ts">
  import type { CommitSummary } from "../../types";
  import { formatRelativeTime } from "../../format";
  import Icon from "../common/Icon.svelte";

  interface Props {
    commit: CommitSummary;
    isSelected: boolean;
    isLocal?: boolean;
    isMenuOpen?: boolean;
    onselect?: () => void;
    onclick?: () => void;
    onmenu?: (pos: { x: number; y: number }) => void;
  }

  let {
    commit,
    isSelected,
    isLocal = false,
    isMenuOpen = false,
    onselect,
    onclick,
    onmenu,
  }: Props = $props();

  let isMerge = $derived(commit.parents && commit.parents.length > 1);

  function handleTrigger(e?: Event) {
    if (e) e.stopPropagation();
    if (onselect) onselect();
    else if (onclick) onclick();
  }

  function openMenuAt(x: number, y: number) {
    onmenu?.({ x, y });
  }

  function handleRowKeydown(e: KeyboardEvent) {
    // Keyboard-accessible alternative to right-click (Shift+F10 / Menu key).
    if (onmenu && (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10"))) {
      e.preventDefault();
      e.stopPropagation();
      const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
      openMenuAt(rect.left + 16, rect.top + 16);
      return;
    }
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      handleTrigger(e);
    }
  }

  function handleContextMenu(e: MouseEvent) {
    if (!onmenu) return;
    e.preventDefault();
    e.stopPropagation();
    openMenuAt(e.clientX, e.clientY);
  }

  function handleMenuButton(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    openMenuAt(rect.left, rect.bottom + 4);
  }
</script>

<div
  class="commit-item {isSelected ? 'active' : ''} {commit.isNew ? 'is-new' : 'is-reviewed'} {isMerge ? 'merge-commit' : ''} {isLocal ? 'is-local' : ''} {isMenuOpen ? 'menu-open' : ''}"
  onclick={handleTrigger}
  onkeydown={handleRowKeydown}
  oncontextmenu={handleContextMenu}
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
    {#if isMerge}
      <span class="merge-badge">merge</span>
    {/if}
    {#if isLocal}
      <span class="unpushed-icon" title="Unpushed commit (local)">
        <Icon name="arrow-up" size={13} strokeWidth={2.4} />
      </span>
    {/if}
    {#if onmenu}
      <button
        type="button"
        class="row-menu-btn"
        aria-label="Commit actions"
        aria-haspopup="menu"
        aria-expanded={isMenuOpen}
        title="Commit actions"
        onclick={handleMenuButton}
        onkeydown={(e) => e.stopPropagation()}
      >
        <Icon name="more-horizontal" size={15} />
      </button>
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

  /* Ghost "⋯" action button, revealed on hover/focus (DESIGN.md §5.3). */
  .row-menu-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    padding: 0;
    border: none;
    border-radius: var(--r-sm, 4px);
    background: transparent;
    color: #636b74;
    cursor: pointer;
    flex-shrink: 0;
    opacity: 0;
    transition: opacity 100ms ease, background-color 100ms ease;
  }
  :global([data-theme="dark"]) .row-menu-btn {
    color: #8b949e;
  }
  .commit-item:hover .row-menu-btn,
  .commit-item.active .row-menu-btn,
  .commit-item.menu-open .row-menu-btn,
  .row-menu-btn:focus-visible {
    opacity: 1;
  }
  .row-menu-btn:hover {
    background: rgba(0, 0, 0, 0.08);
    color: #1a1c1f;
  }
  :global([data-theme="dark"]) .row-menu-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: var(--text);
  }

  /* Prototype compact override (approved-prototype.html · @media max-width:1400px). */
  @media (max-width: 1400px) {
    .commit-title {
      font-size: 12.5px;
    }
  }
</style>
