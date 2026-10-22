<script lang="ts">
  import { repos, loadRepos } from "../../stores/repos";
  import { openRepoInFocusedPane } from "../../stores/panes";
  import { addToast } from "../../stores/ui";
  import * as api from "../../api";
  import type { BranchInfo, RepoEntry, RepoStatus } from "../../types";
  import Icon from "../common/Icon.svelte";

  interface Props {
    repo: RepoEntry | null;
    status: RepoStatus | null;
  }

  let { repo, status }: Props = $props();

  let currentBranch = $derived(repo?.branch || status?.branch || "HEAD");
  let isInactiveView = $derived(status?.isCheckedOut === false);

  let branches = $state<BranchInfo[]>([]);
  let showMenu = $state(false);
  let showModal = $state(false);
  let branchFilter = $state("");
  let isCheckingOut = $state(false);
  let activeTab = $state<"branches" | "pull-requests">("branches");

  let triggerEl = $state<HTMLButtonElement | null>(null);
  let menuEl = $state<HTMLDivElement | null>(null);
  let searchInputEl = $state<HTMLInputElement | null>(null);

  let filteredBranches = $derived(
    branches.filter(
      (b) => !branchFilter || b.name.toLowerCase().includes(branchFilter.toLowerCase())
    )
  );

  let defaultBranch = $derived(
    branches.find((b) => b.name === "main" || b.name === "master")
  );

  let otherBranches = $derived(
    filteredBranches.filter((b) => b.name !== currentBranch && (!defaultBranch || b.name !== defaultBranch.name))
  );

  let siblingViews = $derived(
    repo ? $repos.filter((r) => r.path === repo.path) : []
  );

  let branchGeneration = 0;
  async function refreshBranches(id: string) {
    const generation = ++branchGeneration;
    try {
      const result = await api.branchList(id);
      if (generation === branchGeneration && repo?.id === id) branches = result;
    } catch {
      if (generation === branchGeneration && repo?.id === id) branches = [];
    }
  }

  $effect(() => {
    const id = repo?.id;
    branches = [];
    showMenu = false;
    if (id) void refreshBranches(id);
    return () => {
      branchGeneration++;
    };
  });

  function closeMenu() {
    showMenu = false;
    branchFilter = "";
  }

  function toggleMenu() {
    if (showMenu) {
      closeMenu();
    } else {
      showMenu = true;
      branchFilter = "";
      queueMicrotask(() => {
        searchInputEl?.focus();
      });
    }
  }

  function handleTriggerKeydown(e: KeyboardEvent) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      toggleMenu();
    }
  }

  function handleMenuKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      closeMenu();
      triggerEl?.focus();
      return;
    }
    if (e.key === "Tab") {
      closeMenu();
      return;
    }
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (!menuEl) return;
      const items = Array.from(menuEl.querySelectorAll<HTMLElement>(".gh-branch-row"));
      if (items.length === 0) return;
      const i = items.indexOf(document.activeElement as HTMLElement);
      if (i === -1) {
        items[0]?.focus();
      } else {
        items[(i + (e.key === "ArrowDown" ? 1 : -1) + items.length) % items.length]?.focus();
      }
    }
  }

  async function selectBranch(branchName: string) {
    if (!repo) return;
    closeMenu();
    if (branchName === currentBranch) {
      triggerEl?.focus();
      return;
    }
    const existing = siblingViews.find((r) => r.branch === branchName);
    if (existing) {
      openRepoInFocusedPane(existing.id);
      return;
    }
    try {
      const entry = await api.repoAdd(repo.path, branchName);
      await loadRepos();
      openRepoInFocusedPane(entry.id);
      addToast({ message: `Opened branch ${branchName}`, kind: "success" });
    } catch (e: any) {
      addToast({ message: e.message || "Failed to open branch", kind: "error" });
      triggerEl?.focus();
    }
  }

  async function checkoutCurrentBranch() {
    if (!repo || isCheckingOut) return;
    const target = { id: repo.id, branch: currentBranch };
    isCheckingOut = true;
    closeMenu();
    try {
      await api.checkout(target.id, target.branch);
      await loadRepos();
      if (repo?.id === target.id) await refreshBranches(target.id);
      addToast({ message: `Checked out ${target.branch}`, kind: "success" });
    } catch (e: any) {
      addToast({ message: e.message || "Checkout failed", kind: "error" });
    } finally {
      isCheckingOut = false;
    }
  }

  async function handleAddBranchView(targetBranch: string) {
    if (!repo) return;
    const b = targetBranch.trim();
    if (!b) return;
    showMenu = false;
    showModal = false;
    try {
      const entry = await api.repoAdd(repo.path, b);
      addToast({ message: `Added view for branch ${b}`, kind: "success" });
      await loadRepos();
      openRepoInFocusedPane(entry.id);
    } catch (e: any) {
      addToast({ message: e.message || "Failed to add branch view", kind: "error" });
    }
  }
</script>

<div class="branch-dropdown-wrapper">
  <!-- GitHub Desktop Styled Current Branch Header / Trigger -->
  <button
    bind:this={triggerEl}
    type="button"
    class="branch-trigger-btn"
    aria-haspopup="menu"
    aria-expanded={showMenu}
    onclick={toggleMenu}
    onkeydown={handleTriggerKeydown}
    title={`Current branch: ${currentBranch}${isInactiveView ? " (not checked out)" : ""}`}
  >
    <div class="trigger-icon-wrap">
      <Icon name="git-branch" size={16} class="branch-icon" />
    </div>
    <div class="trigger-content">
      <span class="trigger-sublabel">Current Branch</span>
      <span class="trigger-name font-mono">{currentBranch}</span>
    </div>
    {#if isInactiveView}
      <span class="inactive-dot" title="Read-only view (not checked out on disk)"></span>
    {/if}
    <Icon name="chevron-down" size={14} class="chevron-icon" />
  </button>

  {#if showMenu}
    <div
      class="menu-backdrop"
      role="presentation"
      onclick={closeMenu}
    ></div>

    <div
      bind:this={menuEl}
      class="gh-dropdown-popover"
      role="menu"
      aria-label="Branches"
      tabindex="-1"
      onkeydown={handleMenuKeydown}
    >
      <!-- Top Segmented Tabs: Branches / Pull Requests -->
      <div class="gh-tab-bar">
        <button
          type="button"
          class="gh-tab"
          class:active={activeTab === "branches"}
          onclick={() => (activeTab = "branches")}
        >
          Branches
        </button>
        <button
          type="button"
          class="gh-tab"
          class:active={activeTab === "pull-requests"}
          onclick={() => (activeTab = "pull-requests")}
        >
          Pull Requests
        </button>
      </div>

      {#if activeTab === "branches"}
        <!-- Search filter input and 'New Branch' button -->
        <div class="gh-filter-bar">
          <div class="gh-input-wrapper">
            <Icon name="search" size={13} class="gh-search-icon" />
            <input
              bind:this={searchInputEl}
              type="text"
              placeholder="Filter"
              bind:value={branchFilter}
              class="gh-filter-input"
              onkeydown={(e) => {
                if (e.key === "Escape") {
                  closeMenu();
                  triggerEl?.focus();
                } else {
                  e.stopPropagation();
                }
              }}
            />
          </div>
          <button
            type="button"
            class="gh-new-branch-btn"
            onclick={() => {
              showMenu = false;
              branchFilter = "";
              showModal = true;
            }}
          >
            New Branch
          </button>
        </div>

        <div class="gh-branch-list-scroll">
          <!-- Default branch (if matches and filter isn't active or matches) -->
          {#if defaultBranch && (!branchFilter || defaultBranch.name.toLowerCase().includes(branchFilter.toLowerCase()))}
            <div class="gh-section-header">Default Branch</div>
            <button
              type="button"
              class="gh-branch-row"
              class:selected={currentBranch === defaultBranch.name}
              role="menuitemradio"
              aria-checked={currentBranch === defaultBranch.name}
              onclick={() => selectBranch(defaultBranch!.name)}
            >
              <div class="gh-row-left">
                <Icon name="git-branch" size={14} class="gh-row-icon" />
                <span class="gh-branch-title font-mono">{defaultBranch.name}</span>
                {#if defaultBranch.isHead}
                  <span class="gh-head-badge">HEAD</span>
                {/if}
              </div>
              <div class="gh-row-right">
                {#if defaultBranch.relativeDate}
                  <span class="gh-date">{defaultBranch.relativeDate}</span>
                {/if}
                {#if currentBranch === defaultBranch.name}
                  <span class="gh-check">✓</span>
                {/if}
              </div>
            </button>
          {/if}

          <!-- Current Branch Section if not default -->
          {#if (!defaultBranch || defaultBranch.name !== currentBranch) && (!branchFilter || currentBranch.toLowerCase().includes(branchFilter.toLowerCase()))}
            <div class="gh-section-header">Current Branch</div>
            <button
              type="button"
              class="gh-branch-row selected"
              role="menuitemradio"
              aria-checked={true}
              onclick={() => selectBranch(currentBranch)}
            >
              <div class="gh-row-left">
                <Icon name="git-branch" size={14} class="gh-row-icon" />
                <span class="gh-branch-title font-mono">{currentBranch}</span>
                <span class="gh-head-badge">CURRENT</span>
              </div>
              <div class="gh-row-right">
                <span class="gh-check">✓</span>
              </div>
            </button>
          {/if}

          <!-- Other branches -->
          {#if otherBranches.length > 0}
            <div class="gh-section-header">Other Branches</div>
            {#each otherBranches as b (b.name)}
              <button
                type="button"
                class="gh-branch-row"
                class:selected={currentBranch === b.name}
                role="menuitemradio"
                aria-checked={currentBranch === b.name}
                onclick={() => selectBranch(b.name)}
              >
                <div class="gh-row-left">
                  <Icon name="git-branch" size={14} class="gh-row-icon" />
                  <span class="gh-branch-title font-mono">{b.name}</span>
                  {#if b.isHead}
                    <span class="gh-head-badge">HEAD</span>
                  {/if}
                </div>
                <div class="gh-row-right">
                  {#if b.relativeDate}
                    <span class="gh-date">{b.relativeDate}</span>
                  {/if}
                  {#if currentBranch === b.name}
                    <span class="gh-check">✓</span>
                  {/if}
                </div>
              </button>
            {/each}
          {/if}

          {#if filteredBranches.length === 0}
            <div class="gh-empty-list">
              <span>No branches match "{branchFilter}"</span>
            </div>
          {/if}
        </div>

        {#if isInactiveView}
          <div class="gh-footer-bar">
            <button
              type="button"
              class="gh-checkout-btn"
              disabled={isCheckingOut}
              onclick={checkoutCurrentBranch}
            >
              <Icon name="check" size={13} />
              <span>{isCheckingOut ? "Checking out…" : "Check out this branch"}</span>
            </button>
          </div>
        {/if}
      {:else}
        <!-- Pull Requests Placeholder Tab -->
        <div class="gh-pr-empty-view">
          <Icon name="git-branch" size={28} class="pr-icon" />
          <span class="pr-empty-title">No Pull Requests</span>
          <span class="pr-empty-subtitle">Connect your GitHub account or repository remote to view pull requests.</span>
        </div>
      {/if}
    </div>
  {/if}
</div>

{#if showModal && repo}
  <div
    class="modal-backdrop"
    role="presentation"
    onclick={() => (showModal = false)}
  >
    <div
      class="branch-dialog"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <div class="dialog-title">
        <span class="text-strong">Add branch view — {repo.name}</span>
        <button
          type="button"
          class="dialog-close-btn"
          onclick={() => (showModal = false)}
          aria-label="Close"
        >
          <Icon name="close" size={14} />
        </button>
      </div>
      <p class="dialog-desc text-xs text-muted">
        Add another view of this repository for a different branch. You can compare and browse
        multiple branches simultaneously.
      </p>

      <div class="dialog-search">
        <input
          type="text"
          placeholder="Search or enter branch name…"
          bind:value={branchFilter}
          class="dialog-branch-input"
          onkeydown={(e) => {
            if (e.key === "Enter" && branchFilter.trim()) {
              e.preventDefault();
              handleAddBranchView(branchFilter.trim());
            }
          }}
        />
        {#if branchFilter.trim() && !branches.some((b) => b.name === branchFilter.trim())}
          <button
            type="button"
            class="add-branch-modal-btn primary"
            onclick={() => handleAddBranchView(branchFilter.trim())}
          >
            Add "{branchFilter.trim()}"
          </button>
        {/if}
      </div>

      <div class="branches-container">
        {#if filteredBranches.length === 0}
          <div class="empty-branches text-xs text-muted">
            No matching branches found. Press enter to add "{branchFilter}".
          </div>
        {:else}
          {#each filteredBranches as b (b.name)}
            <div class="branch-picker-item" class:is-head={b.isHead}>
              <span class="branch-picker-name font-mono text-xs">
                <Icon name="git-branch" size={12} class="mr-1 inline-block" />
                {b.name}
                {#if b.isHead}
                  <span class="head-pill text-xs">HEAD</span>
                {/if}
              </span>
              <button
                type="button"
                class="pick-btn"
                onclick={() => handleAddBranchView(b.name)}
              >
                Add view
              </button>
            </div>
          {/each}
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .branch-dropdown-wrapper {
    position: relative;
    width: 100%;
    display: flex;
    align-items: center;
  }

  /* GitHub Desktop Style Trigger Button */
  .branch-trigger-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 38px;
    padding: 2px 8px;
    border-radius: var(--r-sm, 6px);
    background-color: var(--bg);
    border: 1px solid var(--border);
    color: var(--text);
    cursor: pointer;
    transition: background-color 0.15s, border-color 0.15s;
    user-select: none;
    text-align: left;
  }

  .branch-trigger-btn:hover,
  .branch-trigger-btn[aria-expanded="true"] {
    background-color: var(--bg-muted);
    border-color: var(--border-subtle, var(--border));
  }

  .branch-trigger-btn:focus-visible {
    outline: none;
    box-shadow: 0 0 0 2px rgba(24, 115, 213, 0.25);
    border-color: var(--accent);
  }

  .trigger-icon-wrap {
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--accent);
    flex-shrink: 0;
  }

  .trigger-content {
    display: flex;
    flex-direction: column;
    justify-content: center;
    flex: 1;
    min-width: 0;
    line-height: 1.15;
  }

  .trigger-sublabel {
    font-size: 10px;
    font-weight: 500;
    color: var(--text-muted);
    letter-spacing: 0.02em;
  }

  .trigger-name {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .inactive-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background-color: var(--warning, #f59e0b);
    flex-shrink: 0;
  }

  .branch-trigger-btn :global(.chevron-icon) {
    color: var(--text-muted);
    flex-shrink: 0;
    transition: transform 0.15s ease;
  }

  .branch-trigger-btn[aria-expanded="true"] :global(.chevron-icon) {
    transform: rotate(180deg);
  }

  /* Backdrop */
  .menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 100;
    background: transparent;
  }

  /* GitHub Desktop Popover Dropdown */
  .gh-dropdown-popover {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 101;
    width: 380px;
    max-width: 90vw;
    display: flex;
    flex-direction: column;
    background-color: var(--bg);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.24), 0 0 0 1px rgba(0, 0, 0, 0.06);
    overflow: hidden;
  }

  :global([data-theme="dark"]) .gh-dropdown-popover {
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.65), 0 0 0 1px rgba(255, 255, 255, 0.1);
  }

  /* GitHub Desktop Tabs (Branches / Pull Requests) */
  .gh-tab-bar {
    display: flex;
    border-bottom: 1px solid var(--border);
    background-color: var(--bg);
  }

  .gh-tab {
    flex: 1;
    padding: 10px 0;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-muted);
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    cursor: pointer;
    text-align: center;
    transition: color 0.15s ease;
  }

  .gh-tab:hover {
    color: var(--text);
  }

  .gh-tab.active {
    color: var(--text);
    border-bottom-color: var(--accent);
  }

  /* Filter bar with Search input and 'New Branch' button */
  .gh-filter-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border-subtle, var(--border));
    background-color: var(--bg);
  }

  .gh-input-wrapper {
    position: relative;
    flex: 1;
    display: flex;
    align-items: center;
  }

  .gh-input-wrapper :global(.gh-search-icon) {
    position: absolute;
    left: 8px;
    color: var(--text-muted);
    pointer-events: none;
  }

  .gh-filter-input {
    width: 100%;
    height: 28px;
    padding: 0 8px 0 28px;
    font-size: 12px;
    background-color: var(--bg-subtle);
    border: 1px solid var(--border);
    border-radius: 5px;
    color: var(--text);
    transition: border-color 0.15s, box-shadow 0.15s;
  }

  .gh-filter-input:focus {
    outline: none;
    border-color: var(--accent);
    background-color: var(--bg);
    box-shadow: 0 0 0 2px rgba(24, 115, 213, 0.2);
  }

  .gh-new-branch-btn {
    height: 28px;
    padding: 0 10px;
    font-size: 11.5px;
    font-weight: 500;
    color: var(--text);
    background-color: var(--bg-subtle);
    border: 1px solid var(--border);
    border-radius: 5px;
    cursor: pointer;
    white-space: nowrap;
    transition: background-color 0.15s, border-color 0.15s;
  }

  .gh-new-branch-btn:hover {
    background-color: var(--bg-muted);
    border-color: var(--border);
  }

  /* Scrollable branch list */
  .gh-branch-list-scroll {
    max-height: 380px;
    overflow-y: auto;
    padding: 6px 0;
  }

  .gh-section-header {
    padding: 6px 14px 4px;
    font-size: 11px;
    font-weight: 700;
    color: var(--text);
    letter-spacing: -0.01em;
  }

  .gh-branch-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 6px 14px;
    border: none;
    background: transparent;
    color: var(--text);
    font-size: 12.5px;
    cursor: pointer;
    text-align: left;
    transition: background-color 0.12s;
  }

  .gh-branch-row:hover,
  .gh-branch-row:focus-visible {
    background-color: var(--bg-muted);
    outline: none;
  }

  .gh-branch-row.selected {
    background-color: var(--accent-bg, rgba(24, 115, 213, 0.08));
  }

  .gh-row-left {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    flex: 1;
  }

  .gh-row-left :global(.gh-row-icon) {
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .gh-branch-row:hover :global(.gh-row-icon),
  .gh-branch-row.selected :global(.gh-row-icon) {
    color: var(--accent);
  }

  .gh-branch-title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12.5px;
    color: var(--text);
  }

  .gh-branch-row.selected .gh-branch-title {
    font-weight: 600;
    color: var(--accent);
  }

  .gh-head-badge {
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    padding: 1px 4px;
    border-radius: 3px;
    background-color: rgba(125, 125, 125, 0.16);
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .gh-row-right {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
    margin-left: 8px;
  }

  .gh-date {
    font-size: 11.5px;
    color: var(--text-muted);
  }

  .gh-check {
    font-size: 12.5px;
    font-weight: 700;
    color: var(--accent);
  }

  .gh-empty-list {
    padding: 24px 16px;
    text-align: center;
    font-size: 12px;
    color: var(--text-muted);
  }

  /* Inactive / Checkout footer */
  .gh-footer-bar {
    padding: 8px 12px;
    border-top: 1px solid var(--border);
    background-color: var(--bg-subtle);
  }

  .gh-checkout-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    width: 100%;
    height: 28px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background-color: var(--bg);
    color: var(--text);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
  }

  .gh-checkout-btn:hover {
    background-color: var(--bg-muted);
  }

  /* PR placeholder */
  .gh-pr-empty-view {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 36px 20px;
    text-align: center;
    gap: 6px;
  }

  .gh-pr-empty-view :global(.pr-icon) {
    color: var(--text-muted);
    margin-bottom: 4px;
  }

  .pr-empty-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text);
  }

  .pr-empty-subtitle {
    font-size: 11.5px;
    color: var(--text-muted);
    max-width: 260px;
    line-height: 1.4;
  }

  /* Modal Styles */
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
    backdrop-filter: blur(2px);
  }

  .branch-dialog {
    width: 480px;
    max-height: 80vh;
    background-color: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--r-lg, 10px);
    box-shadow: var(--shadow-pop, 0 16px 36px rgba(0, 0, 0, 0.35));
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding: 16px;
  }

  .dialog-title {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 14px;
    margin-bottom: 8px;
  }

  .dialog-close-btn {
    border: none;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    border-radius: var(--r-sm, 6px);
    padding: 4px;
  }

  .dialog-close-btn:hover {
    color: var(--text);
    background-color: var(--bg-muted);
  }

  .dialog-desc {
    margin-bottom: 12px;
    line-height: 1.4;
  }

  .dialog-search {
    display: flex;
    gap: 8px;
    margin-bottom: 12px;
  }

  .dialog-branch-input {
    flex: 1;
    height: 32px;
    padding: 0 10px;
    background-color: var(--bg-subtle);
    border: 1px solid var(--border);
    border-radius: var(--r-sm, 6px);
    color: var(--text);
    font-size: 12px;
  }

  .dialog-branch-input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .add-branch-modal-btn {
    height: 32px;
    padding: 0 12px;
    background-color: var(--accent);
    color: #fff;
    border: none;
    border-radius: var(--r-sm, 6px);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
  }

  .branches-container {
    flex: 1;
    overflow-y: auto;
    max-height: 280px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm, 6px);
    padding: 4px;
    background-color: var(--bg);
  }

  .branch-picker-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 8px;
    border-radius: 4px;
  }

  .branch-picker-item:hover {
    background-color: var(--bg-muted);
  }

  .branch-picker-name {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--text);
  }

  .pick-btn {
    font-size: 11px;
    padding: 2px 8px;
    background-color: var(--bg-subtle);
    border: 1px solid var(--border);
    border-radius: var(--r-sm, 6px);
    color: var(--text);
    cursor: pointer;
  }

  .pick-btn:hover {
    background-color: var(--accent);
    color: #fff;
    border-color: var(--accent);
  }
</style>
