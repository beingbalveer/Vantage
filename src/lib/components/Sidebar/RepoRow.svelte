<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import type { RepoEntry, RepoStatus, BranchInfo } from "../../platform/types";
  import {
    openRepoInFocusedPane,
    openRepoInNextPane,
    focusedPaneIndex,
    panes,
    paneCount,
  } from "../../domain/stores/panes";
  import { repos, loadRepos, activeRepoId, removeRepo } from "../../domain/stores/repos";
  import { addToast, sidebarCollapsed, openDialog } from "../../app/ui";
  import { registerDismiss } from "../../app/commands";
  import * as api from "../../platform/api";
  import { get } from "svelte/store";
  import Icon from "../common/Icon.svelte";
  import { Modal } from "../common";
  import RepoIcon from "./RepoIcon.svelte";
  import { formatCount, pluralize } from "../../domain/format";

  interface Props {
    repo: RepoEntry;
    status?: RepoStatus;
    isDragging?: boolean;
    dropIndicator?: "before" | "after" | null;
    /** Pointer press on the row body; the sidebar turns a drag into a reorder. */
    onreorderstart?: (e: PointerEvent) => void;
    /** Keyboard reorder: -1 moves up, +1 moves down. */
    onmove?: (delta: -1 | 1) => void;
  }

  let {
    repo,
    status,
    isDragging = false,
    dropIndicator = null,
    onreorderstart,
    onmove,
  }: Props = $props();

  let showMenu = $state(false);
  let menuPos = $state<{ top: number; left: number } | null>(null);
  let showBranchModal = $state(false);
  let availableBranches = $state<BranchInfo[]>([]);
  let branchFilter = $state("");
  let isLoadingBranches = $state(false);

  // ---- Collapsed hover card (UI-04) ----
  let hovered = $state(false);
  let hoverTop = $state(0);
  let hoverLeft = $state(0);
  let hoverTimer: ReturnType<typeof setTimeout> | undefined;

  function startHover(e: PointerEvent | FocusEvent) {
    if (!$sidebarCollapsed || showMenu) return;
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    // Keep the ~150px card within the viewport near the bottom of a long list.
    hoverTop = Math.max(8, Math.min(rect.top, window.innerHeight - 158));
    hoverLeft = rect.right + 10;
    clearTimeout(hoverTimer);
    hoverTimer = setTimeout(() => {
      hovered = true;
    }, 220);
  }

  function stopHover() {
    if (hoverTimer) clearTimeout(hoverTimer);
    hoverTimer = undefined;
    hovered = false;
  }

  onMount(() => {
    // Scroll (capture, since scroll doesn't bubble) dismisses the card.
    const onScroll = () => stopHover();
    window.addEventListener("scroll", onScroll, true);
    return () => {
      window.removeEventListener("scroll", onScroll, true);
    };
  });

  // Escape is owned by the registry's dismiss stack (no ad-hoc window listener).
  $effect(() => {
    const unregister = registerDismiss(() => {
      if (showMenu) {
        showMenu = false;
        menuPos = null;
        return true;
      }
      if (showBranchModal) {
        showBranchModal = false;
        return true;
      }
      if (hovered) {
        stopHover();
        return true;
      }
      return false;
    });
    return unregister;
  });

  // Collapse/expand transitions must dismiss the card.
  $effect(() => {
    void $sidebarCollapsed;
    stopHover();
  });

  onDestroy(() => {
    if (hoverTimer) clearTimeout(hoverTimer);
  });

  // Pane indicators
  let paneIndicators = $derived.by(() => {
    const list: { paneNumber: number; isFocused: boolean }[] = [];
    const count = $paneCount;
    const fIdx = $focusedPaneIndex;
    $panes.slice(0, count).forEach((p, idx) => {
      if (p.repoId === repo.id) {
        list.push({ paneNumber: idx + 1, isFocused: idx === fIdx });
      }
    });
    return list;
  });

  let isOpenInFocused = $derived(paneIndicators.some((p) => p.isFocused));

  let displayBranch = $derived(repo.branch || status?.branch || "");

  // ---- Attention signals (ALPH-145) ----
  let hasError = $derived(Boolean(status?.error));
  let dirtyCount = $derived(status?.dirtyCount ?? 0);
  let aheadCount = $derived(status?.ahead ?? 0);
  let behindCount = $derived(status?.behind ?? 0);

  let aheadLabel = $derived(formatCount(aheadCount));

  let dirtyLabel = $derived(
    `${dirtyCount} ${pluralize(dirtyCount, "file")} changed`
  );
  let aheadTitle = $derived(
    `${aheadCount} unpushed ${pluralize(aheadCount, "commit")}`
  );

  // Screen-reader summary for the row (DESIGN §11).
  let rowAriaLabel = $derived.by(() => {
    const parts = [`${repo.name}, branch ${displayBranch || "HEAD"}`];
    if (hasError) {
      parts.push(`error: ${status?.error}`);
    } else {
      if (dirtyCount > 0) parts.push("uncommitted changes");
      if (aheadCount > 0) {
        parts.push(`${aheadCount} unpushed ${pluralize(aheadCount, "commit")}`);
      }
    }
    return parts.join(", ");
  });

  // Native hover tooltip: identity + sync + attention (DESIGN §4.4).
  let rowTitle = $derived.by(() => {
    const sync = `${displayBranch || "HEAD"}${aheadCount ? ` ↑${aheadCount}` : ""}${
      behindCount ? ` ↓${behindCount}` : ""
    }`;
    const lines = [`${repo.name} · ${sync}`, repo.path, "Alt+↑/↓ to reorder"];
    if (hasError) {
      lines.push(status?.error || "Repository error");
    } else {
      const attention: string[] = [];
      if (dirtyCount > 0) attention.push(dirtyLabel);
      if (aheadCount > 0) attention.push(aheadTitle);
      if (attention.length > 0) lines.push(attention.join(" · "));
    }
    return lines.join("\n");
  });

  // Attention signal displayed as a badge on the repository icon (in all modes).
  let attentionSignal = $derived.by(() => {
    if (hasError) return { kind: "error" as const, text: "", label: status?.error || "Repository error" };
    if (dirtyCount > 0) return { kind: "dirty" as const, text: "", label: dirtyLabel };
    if (aheadCount > 0) return { kind: "ahead" as const, text: "", label: aheadTitle };
    return null;
  });

  function handleContextMenu(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    stopHover();
    menuPos = {
      top: Math.min(e.clientY, window.innerHeight - 240),
      left: Math.min(e.clientX, window.innerWidth - 190),
    };
    showMenu = true;
  }

  function handleMenuBtnClick(e: MouseEvent) {
    e.stopPropagation();
    stopHover();
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    menuPos = {
      top: Math.min(rect.bottom + 4, window.innerHeight - 240),
      left: Math.max(8, rect.right - 180),
    };
    showMenu = !showMenu;
  }

  function handleClick(e?: MouseEvent) {
    stopHover();
    if (e && (e.metaKey || e.ctrlKey || e.button === 1)) {
      openRepoInNextPane(repo.id);
    } else {
      activeRepoId.set(repo.id);
      openRepoInFocusedPane(repo.id);
    }
  }

  async function openBranchModal(e?: Event) {
    if (e) e.stopPropagation();
    showMenu = false;
    showBranchModal = true;
    branchFilter = "";
    isLoadingBranches = true;
    try {
      availableBranches = await api.branchList(repo.id);
    } catch {
      availableBranches = [];
    } finally {
      isLoadingBranches = false;
    }
  }

  async function handleAddBranch(targetBranch: string) {
    const b = targetBranch.trim();
    if (!b) return;
    showBranchModal = false;
    try {
      const newEntry = await api.repoAdd(repo.path, b);
      addToast({ message: `Added branch view for ${b}`, kind: "success" });
      await loadRepos();
      activeRepoId.set(newEntry.id);
      openRepoInFocusedPane(newEntry.id);
    } catch (err: any) {
      addToast({ message: err.message || "Failed to add branch view", kind: "error" });
    }
  }

  function handleRemove(e?: Event) {
    if (e) e.stopPropagation();
    showMenu = false;
    stopHover();
    openDialog({ type: "remove_repo", repoId: repo.id, data: { repo } });
  }

  async function handleTogglePin(e?: Event) {
    if (e) e.stopPropagation();
    showMenu = false;
    try {
      await api.repoUpdate(repo.id, { pinned: !repo.pinned });
      await loadRepos();
      addToast({
        message: repo.pinned ? `Unpinned ${repo.name}` : `Pinned ${repo.name}`,
        kind: "info",
      });
    } catch (err: any) {
      addToast({ message: err.message || "Failed to update pin", kind: "error" });
    }
  }

  async function handleCopyPath(e?: Event) {
    if (e) e.stopPropagation();
    showMenu = false;
    await navigator.clipboard.writeText(repo.path);
    addToast({ message: "Repository path copied to clipboard", kind: "success" });
  }

  async function handleReveal(e?: Event) {
    if (e) e.stopPropagation();
    showMenu = false;
    try {
      await api.revealInFileManager(repo.path);
    } catch (err: any) {
      addToast({ message: err.message || "Failed to reveal in file manager", kind: "error" });
    }
  }

  function handleChangeIcon(e?: Event) {
    if (e) e.stopPropagation();
    showMenu = false;
    menuPos = null;
    openDialog({ type: "change_icon", repoId: repo.id, data: { repo } });
  }

  function handleRowPointerDown(e: PointerEvent) {
    if (showMenu || showBranchModal) return;
    // Let the row's own controls (menu, badges, buttons) keep their clicks.
    const target = e.target;
    if (
      target instanceof Element &&
      target.closest("button, a, input, textarea, select, [contenteditable='true']")
    ) {
      return;
    }
    stopHover();
    onreorderstart?.(e);
  }
</script>

<div
  class="repo-row {isOpenInFocused ? 'active' : ''} {$sidebarCollapsed ? 'collapsed' : ''}"
  class:error={hasError}
  class:dragging={isDragging}
  class:drop-before={dropIndicator === "before"}
  class:drop-after={dropIndicator === "after"}
  onpointerdown={handleRowPointerDown}
  onclick={handleClick}
  onpointerenter={startHover}
  onpointerleave={stopHover}
  onfocus={startHover}
  onblur={stopHover}
  oncontextmenu={handleContextMenu}
  onkeydown={(e) => {
    // Alt+Arrow reorders without a pointer. Plain arrows stay available for
    // scrolling the dock, matching the drag-and-drop affordance.
    if (e.altKey && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
      e.preventDefault();
      onmove?.(e.key === "ArrowUp" ? -1 : 1);
      return;
    }
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      handleClick();
    }
  }}
  role="button"
  tabindex="0"
  data-repo-id={repo.id}
  aria-label={rowAriaLabel}
  aria-keyshortcuts="Alt+ArrowUp Alt+ArrowDown"
  aria-current={isOpenInFocused ? "page" : undefined}
  title={rowTitle}
>
  <div class="repo-icon-wrapper">
    <div class="repo-icon-box">
      <RepoIcon id={repo.id} repoName={repo.name} repoPath={repo.path} size={22} />
    </div>

    {#if attentionSignal}
      <span
        class="repo-badge {attentionSignal.kind}"
        role="img"
        aria-label={attentionSignal.label}
        title={attentionSignal.label}
      >
        {#if attentionSignal.kind === "error"}
          <Icon name="alert" size={10} />
        {:else if attentionSignal.text}
          {attentionSignal.text}
        {/if}
      </span>
    {/if}
  </div>

  {#if !$sidebarCollapsed}
    <div class="info">
      <div class="row-top">
        <span class="repo-name">{repo.name}</span>
      </div>
      <span class="branch-name">{displayBranch || "HEAD"}</span>
    </div>

    <button
      type="button"
      class="row-menu-btn"
      class:menu-open={showMenu}
      title="Repository options"
      onclick={handleMenuBtnClick}
      onkeydown={(e) => e.stopPropagation()}
      aria-label="Repository options"
      aria-haspopup="menu"
      aria-expanded={showMenu}
    >
      <Icon name="more-horizontal" size={15} />
    </button>
  {/if}

  {#if hovered && $sidebarCollapsed && !showMenu}
    <div
      class="repo-hover-card"
      style="top:{hoverTop}px;left:{hoverLeft}px"
      role="tooltip"
    >
      <div class="hover-title">
        <RepoIcon id={repo.id} size={22} />
        <span>{repo.name}</span>
        <small>Repository</small>
      </div>
      <div class="hover-line">
        <Icon name="folder" size={16} />
        {repo.name}
      </div>
      <div class="hover-line branch">
        <Icon name="git-branch" size={16} />
        {displayBranch || "HEAD"}
      </div>
      {#if hasError}
        <div class="hover-line error-line">
          <Icon name="alert" size={16} />
          {status?.error}
        </div>
      {:else}
        {#if dirtyCount > 0}
          <div class="hover-line">
            <Icon name="changes" size={16} />
            {dirtyLabel}
          </div>
        {/if}
        {#if aheadCount > 0}
          <div class="hover-line">
            <Icon name="arrow-up" size={16} />
            {aheadTitle}
          </div>
        {/if}
      {/if}
      <div class="hover-path">{repo.path}</div>
    </div>
  {/if}

  <!-- Context menu -->
  {#if showMenu}
    <div
      class="menu-backdrop"
      role="presentation"
      onclick={(e) => {
        e.stopPropagation();
        showMenu = false;
        menuPos = null;
      }}
      onkeydown={(e) => {
        if (e.key === "Escape") {
          showMenu = false;
          menuPos = null;
        }
      }}
    ></div>

    <div
      class="repo-menu-dropdown"
      role="menu"
      tabindex="-1"
      style={menuPos ? `position: fixed; top: ${menuPos.top}px; left: ${menuPos.left}px;` : ""}
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => {
        e.stopPropagation();
        if (e.key === "Escape") {
          showMenu = false;
          menuPos = null;
        }
      }}
    >
      <button class="menu-item" onclick={handleChangeIcon}>Change icon…</button>
      <div class="menu-divider"></div>
      <button class="menu-item" onclick={openBranchModal}>Add branch view…</button>
      <div class="menu-divider"></div>
      <button class="menu-item" onclick={handleTogglePin}>
        {repo.pinned ? "Unpin repository" : "Pin repository"}
      </button>
      <button class="menu-item" onclick={handleCopyPath}>Copy path</button>
      <button class="menu-item" onclick={handleReveal}>Reveal in file manager</button>
      <div class="menu-divider"></div>
      <button class="menu-item danger" onclick={handleRemove}>Remove from Vantage…</button>
    </div>
  {/if}

  <!-- Add branch view modal -->
  <Modal
    open={showBranchModal}
    title={`Add branch view — ${repo.name}`}
    description="Add another view of this repository for a different branch."
    width={380}
    onclose={() => (showBranchModal = false)}
  >
    <div class="dialog-search">
      <input
        type="text"
        placeholder="Search or enter branch name…"
        aria-label="Search or enter branch name"
        bind:value={branchFilter}
        class="dialog-branch-input"
        onkeydown={(e) => {
          if (e.key === "Enter" && branchFilter.trim()) {
            e.preventDefault();
            handleAddBranch(branchFilter.trim());
          }
        }}
      />
      {#if branchFilter.trim() && !availableBranches.some((b) => b.name === branchFilter.trim())}
        <button class="add-branch-btn primary" onclick={() => handleAddBranch(branchFilter.trim())}>
          Add "{branchFilter.trim()}"
        </button>
      {/if}
    </div>

    <div class="branches-container">
      {#if isLoadingBranches}
        <div class="branch-loading text-xs text-muted" role="status">Loading branches…</div>
      {:else}
        {@const filtered = availableBranches.filter(
          (b) => !branchFilter || b.name.toLowerCase().includes(branchFilter.toLowerCase())
        )}
        {#if filtered.length === 0}
          <div class="empty-branches text-xs text-muted">
            No matching branches found. Press enter to add "{branchFilter}".
          </div>
        {:else}
          {#each filtered as b (b.name)}
            <div class="branch-picker-item" class:is-head={b.isHead}>
              <span class="branch-picker-name font-mono text-xs">
                <Icon name="git-branch" size={12} class="mr-1 inline-block" />
                {b.name}
                {#if b.isHead}
                  <span class="head-pill text-xs">current</span>
                {/if}
              </span>
              <button class="pick-btn" onclick={() => handleAddBranch(b.name)}>Add view</button>
            </div>
          {/each}
        {/if}
      {/if}
    </div>
  </Modal>
</div>

<style>
  .repo-row {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 52px;
    padding: 6px 12px;
    cursor: pointer;
    border-radius: 12px;
    margin: 2px 6px;
    position: relative;
    transition: background-color 100ms ease;
    user-select: none;
    background-color: transparent;
  }

  .repo-row:hover {
    background-color: rgba(0, 0, 0, 0.04);
  }
  :global(:root[data-theme="dark"]) .repo-row:hover {
    background-color: rgba(255, 255, 255, 0.06);
  }

  .repo-row.active {
    background-color: rgba(0, 0, 0, 0.07);
  }
  :global(:root[data-theme="dark"]) .repo-row.active {
    background-color: rgba(255, 255, 255, 0.1);
  }

  .repo-row.dragging {
    opacity: 0.4;
  }

  .repo-row.drop-before::before {
    content: "";
    position: absolute;
    top: -2px;
    left: 4px;
    right: 4px;
    height: 3px;
    background: var(--accent);
    border-radius: 2px;
    z-index: 20;
    pointer-events: none;
  }

  .repo-row.drop-after::after {
    content: "";
    position: absolute;
    bottom: -2px;
    left: 4px;
    right: 4px;
    height: 3px;
    background: var(--accent);
    border-radius: 2px;
    z-index: 20;
    pointer-events: none;
  }

  /* Collapsed View */
  .repo-row.collapsed {
    width: 44px;
    height: 44px;
    min-height: 44px;
    max-width: 44px;
    padding: 0;
    margin: 4px auto;
    justify-content: center;
    border-radius: 12px;
    gap: 0;
    flex-shrink: 0;
  }

  .repo-row.collapsed .repo-icon-box {
    margin: 0;
  }

  .repo-icon-wrapper {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .repo-icon-box {
    width: 36px;
    height: 36px;
    border-radius: 10px;
    display: flex;
    align-items: center;
    justify-content: center;
    background-color: var(--bg);
    border: 1px solid var(--border-subtle);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.04);
    flex-shrink: 0;
    box-sizing: border-box;
    overflow: hidden;
  }

  :global([data-theme="dark"]) .repo-icon-box,
  :global(:root[data-theme="dark"]) .repo-icon-box {
    background-color: #21262d;
    border-color: #30363d;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.3);
  }

  /* Attention badge on repository icon (shown in all modes) */
  .repo-badge {
    position: absolute;
    top: -3px;
    right: -4px;
    min-width: 14px;
    height: 14px;
    padding: 0 4px;
    border-radius: 7px;
    box-sizing: border-box;
    background-color: var(--accent);
    color: var(--accent-fg);
    font-size: 9px;
    font-weight: 700;
    line-height: 1;
    font-variant-numeric: tabular-nums;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    pointer-events: none;
    border: 1.5px solid var(--chrome, #f3f4f6);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.1);
    z-index: 2;
  }
  :global([data-theme="dark"]) .repo-badge,
  :global(:root[data-theme="dark"]) .repo-badge {
    border-color: #1b1f24;
  }

  .repo-badge.dirty,
  .repo-badge.ahead {
    padding: 0;
    min-width: 10px;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    top: -1px;
    right: -2px;
  }
  .repo-badge.dirty {
    background-color: var(--warning);
  }
  .repo-badge.ahead {
    background-color: var(--text-muted);
  }
  .repo-badge.error {
    background-color: var(--warning-bg);
    color: var(--warning);
    padding: 0;
    width: 14px;
    height: 14px;
  }

  .info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 2px;
  }

  .row-top {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .repo-row.error .repo-name,
  .repo-row.error .branch-name {
    color: var(--text-faint);
  }

  .repo-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1 1 auto;
    min-width: 0;
    color: var(--text);
    font-size: 14px;
    font-weight: 500;
    line-height: 1.25;
    letter-spacing: -0.01em;
    font-family: var(--font-ui);
  }

  :global([data-theme="dark"]) .repo-name,
  :global(:root[data-theme="dark"]) .repo-name {
    color: var(--text);
  }

  .branch-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-muted);
    font-size: 12px;
    font-weight: 400;
    line-height: 1.25;
    letter-spacing: 0;
    font-family: var(--font-ui);
  }

  :global([data-theme="dark"]) .branch-name,
  :global(:root[data-theme="dark"]) .branch-name {
    color: var(--text-muted);
  }

  .row-menu-btn {
    width: 24px;
    height: 24px;
    border-radius: var(--r-sm, 4px);
    background: transparent;
    border: none;
    color: var(--text-muted);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    flex-shrink: 0;
    opacity: 0;
    transition: opacity 120ms ease, background-color 120ms ease;
  }
  .repo-row:hover .row-menu-btn,
  .row-menu-btn:focus,
  .row-menu-btn:focus-visible,
  .row-menu-btn.menu-open {
    opacity: 0.8;
  }
  .row-menu-btn:hover,
  .row-menu-btn.menu-open {
    opacity: 1 !important;
    background-color: rgba(0, 0, 0, 0.08);
    color: var(--text);
  }
  :global([data-theme="dark"]) .row-menu-btn {
    color: var(--text-muted);
  }
  :global([data-theme="dark"]) .row-menu-btn:hover,
  :global([data-theme="dark"]) .row-menu-btn.menu-open {
    background-color: rgba(255, 255, 255, 0.1);
    color: var(--text);
  }

  /* Collapsed hover card (UI-04) */
  .repo-hover-card {
    position: fixed;
    width: 310px;
    padding: 15px 17px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 14px;
    box-shadow: 0 9px 28px rgba(0, 0, 0, 0.075);
    z-index: 100;
    pointer-events: none;
    color: var(--text);
    font-size: 13px;
  }
  .hover-title {
    display: flex;
    align-items: center;
    gap: 9px;
    font-size: 15px;
    font-weight: 500;
    margin-bottom: 13px;
  }
  .hover-title > span {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .hover-title small {
    margin-left: auto;
    font-size: 10px;
    font-weight: 400;
    color: var(--text-faint);
    flex-shrink: 0;
  }
  .hover-line {
    display: flex;
    align-items: center;
    gap: 9px;
    margin-top: 9px;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hover-line :global(svg) {
    color: var(--text-muted);
    flex-shrink: 0;
  }
  .hover-line.branch :global(svg) {
    color: var(--accent);
  }
  .hover-line.error-line {
    color: var(--warning);
  }
  .hover-line.error-line :global(svg) {
    color: var(--warning);
  }
  .hover-path {
    font-size: 11px;
    color: var(--text-faint);
    margin: 11px 0 0 25px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 99;
  }

  .repo-menu-dropdown {
    position: absolute;
    top: 40px;
    right: 8px;
    width: 190px;
    background-color: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    box-shadow: var(--shadow-pop);
    padding: 4px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    z-index: 100;
  }

  .menu-item {
    height: 28px;
    padding: 0 10px;
    border-radius: var(--r-sm);
    background: transparent;
    border: none;
    color: var(--text);
    font-size: 12px;
    text-align: left;
    cursor: pointer;
    display: flex;
    align-items: center;
  }
  .menu-item:hover {
    background-color: var(--bg-subtle);
  }
  .menu-item.danger {
    color: var(--danger);
  }
  .menu-item.danger:hover {
    background-color: var(--danger-bg);
  }

  .menu-divider {
    height: 1px;
    background-color: var(--border-subtle);
    margin: 3px 0;
  }

  .dialog-search {
    display: flex;
    gap: 8px;
  }

  .dialog-branch-input {
    flex: 1;
    height: 31px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background-color: var(--bg-inset);
    color: var(--text);
    font-size: 12px;
    outline: none;
  }
  .dialog-branch-input:focus {
    border-color: var(--accent);
  }

  .add-branch-btn {
    height: 31px;
    padding: 0 10px;
    border-radius: var(--r-sm);
    border: none;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    background-color: var(--accent);
    color: var(--accent-fg);
    white-space: nowrap;
  }

  .branches-container {
    max-height: 240px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 4px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-md);
    padding: 4px;
  }

  .branch-picker-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 8px;
    border-radius: var(--r-sm);
  }
  .branch-picker-item:hover {
    background-color: var(--bg-subtle);
  }
  .branch-picker-item.is-head {
    background-color: var(--accent-bg);
  }

  .branch-picker-name {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .head-pill {
    padding: 0 4px;
    border-radius: 3px;
    background-color: var(--accent);
    color: var(--accent-fg);
    font-size: 9px;
    font-weight: 600;
  }

  .pick-btn {
    height: 24px;
    padding: 0 8px;
    font-size: 11px;
    font-weight: 500;
    border-radius: var(--r-sm);
    border: 1px solid var(--border);
    background-color: var(--bg);
    color: var(--text);
    cursor: pointer;
  }
  .pick-btn:hover {
    background-color: var(--bg-muted);
  }

  .branch-loading,
  .empty-branches {
    padding: 16px;
    text-align: center;
  }
</style>
