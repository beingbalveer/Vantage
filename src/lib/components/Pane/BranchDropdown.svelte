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

  let triggerEl = $state<HTMLButtonElement | null>(null);
  let menuEl = $state<HTMLDivElement | null>(null);

  let filteredBranches = $derived(
    branches.filter(
      (b) => !branchFilter || b.name.toLowerCase().includes(branchFilter.toLowerCase())
    )
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
        if (!menuEl) return;
        const selected = menuEl.querySelector<HTMLElement>(".branch-option.selected");
        if (selected) {
          selected.focus();
        } else {
          const first = menuEl.querySelector<HTMLElement>(".branch-option");
          first?.focus();
        }
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
      const items = Array.from(menuEl.querySelectorAll<HTMLElement>(".branch-option"));
      const i = items.indexOf(document.activeElement as HTMLElement);
      items[(i + (e.key === "ArrowDown" ? 1 : -1) + items.length) % items.length]?.focus();
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
    <Icon name="git-branch" size={14} class="branch-icon" />
    <span class="branch-label font-mono">{currentBranch}</span>
    {#if isInactiveView}
      <span class="inactive-dot" title="Read-only view (not checked out on disk)"></span>
    {/if}
    <Icon name="chevron-down" size={13} class="chevron-icon" />
  </button>

  {#if showMenu}
    <div
      class="menu-backdrop"
      role="presentation"
      onclick={closeMenu}
    ></div>

    <div
      bind:this={menuEl}
      class="branch-menu"
      role="menu"
      aria-label="Branches"
      tabindex="-1"
      onkeydown={handleMenuKeydown}
    >
      <div class="branch-menu-header">
        <span class="menu-title">Switch branch</span>
      </div>

      {#if branches.length > 5}
        <div class="menu-search">
          <input
            type="text"
            placeholder="Filter branches…"
            bind:value={branchFilter}
            class="branch-filter-input"
            onkeydown={(e) => e.stopPropagation()}
          />
        </div>
      {/if}

      <div class="branch-list">
        {#each filteredBranches as b (b.name)}
          <button
            type="button"
            class="branch-option"
            class:selected={currentBranch === b.name}
            role="menuitemradio"
            aria-checked={currentBranch === b.name}
            tabindex={currentBranch === b.name ? 0 : -1}
            onclick={() => selectBranch(b.name)}
          >
            <Icon name="git-branch" size={13} />
            <span class="branch-option-name font-mono">{b.name}</span>
            {#if b.isHead}
              <span class="head-pill">HEAD</span>
            {/if}
            <span class="branch-check">{currentBranch === b.name ? "✓" : ""}</span>
          </button>
        {/each}

        {#if filteredBranches.length === 0}
          <div class="empty-notice">No branches found</div>
        {/if}
      </div>

      {#if isInactiveView}
        <div class="menu-divider"></div>
        <button
          type="button"
          class="branch-option checkout-action"
          role="menuitem"
          disabled={isCheckingOut}
          onclick={checkoutCurrentBranch}
        >
          <Icon name="check" size={13} />
          <span>{isCheckingOut ? "Checking out…" : "Check out this branch"}</span>
        </button>
      {/if}

      <div class="menu-divider"></div>
      <button
        type="button"
        class="branch-option add-view-action"
        role="menuitem"
        onclick={() => {
          showMenu = false;
          branchFilter = "";
          showModal = true;
        }}
      >
        <Icon name="plus" size={13} />
        <span>Add branch view…</span>
      </button>
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

  .branch-trigger-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    height: 28px;
    padding: 0 8px;
    border-radius: var(--radius-sm);
    background-color: var(--bg-surface);
    border: 1px solid var(--border);
    color: var(--text);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: background-color 0.15s, border-color 0.15s;
    user-select: none;
    text-align: left;
  }

  .branch-trigger-btn:hover,
  .branch-trigger-btn[aria-expanded="true"] {
    background-color: var(--bg-hover);
    border-color: var(--border-focus);
  }

  .branch-trigger-btn:focus-visible {
    outline: none;
    box-shadow: 0 0 0 2px var(--accent-alpha);
    border-color: var(--accent);
  }

  .branch-trigger-btn :global(.branch-icon) {
    color: var(--accent);
    flex-shrink: 0;
  }

  .branch-label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    letter-spacing: -0.01em;
  }

  .inactive-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background-color: var(--warning, #f59e0b);
    flex-shrink: 0;
  }

  .branch-trigger-btn :global(.chevron-icon) {
    color: var(--text-faint);
    flex-shrink: 0;
    transition: transform 0.15s ease;
  }

  .branch-trigger-btn[aria-expanded="true"] :global(.chevron-icon) {
    transform: rotate(180deg);
  }

  .menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 100;
    background: transparent;
  }

  .branch-menu {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    right: 0;
    z-index: 101;
    min-width: 200px;
    max-width: 320px;
    max-height: 380px;
    display: flex;
    flex-direction: column;
    background-color: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-lg, 0 10px 25px -5px rgba(0, 0, 0, 0.4));
    padding: 4px;
    overflow: hidden;
  }

  .branch-menu-header {
    padding: 6px 8px 4px;
  }

  .menu-title {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-faint);
  }

  .menu-search {
    padding: 4px 6px;
  }

  .branch-filter-input {
    width: 100%;
    height: 24px;
    padding: 0 6px;
    font-size: 11px;
    background-color: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text);
  }

  .branch-filter-input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .branch-list {
    flex: 1;
    overflow-y: auto;
    max-height: 240px;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .branch-option {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 6px 8px;
    border: none;
    background: transparent;
    color: var(--text-muted);
    font-size: 12px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    text-align: left;
    transition: background-color 0.1s, color 0.1s;
  }

  .branch-option:hover,
  .branch-option:focus-visible {
    background-color: var(--bg-hover);
    color: var(--text);
    outline: none;
  }

  .branch-option.selected {
    color: var(--accent);
    font-weight: 500;
  }

  .branch-option-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11px;
  }

  .head-pill {
    font-size: 9px;
    text-transform: uppercase;
    font-weight: 600;
    padding: 1px 4px;
    background-color: var(--bg-active);
    color: var(--text-muted);
    border-radius: var(--radius-xs, 3px);
  }

  .branch-check {
    font-size: 12px;
    font-weight: 700;
    color: var(--accent);
    width: 14px;
    text-align: right;
  }

  .empty-notice {
    padding: 12px 8px;
    text-align: center;
    font-size: 11px;
    color: var(--text-faint);
  }

  .menu-divider {
    height: 1px;
    background-color: var(--border);
    margin: 4px 0;
    flex-shrink: 0;
  }

  .checkout-action {
    color: var(--text);
  }

  .add-view-action {
    color: var(--accent);
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
    background-color: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-xl);
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
    border-radius: var(--radius-sm);
    padding: 4px;
  }

  .dialog-close-btn:hover {
    color: var(--text);
    background-color: var(--bg-hover);
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
    background-color: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
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
    color: var(--accent-contrast, #fff);
    border: none;
    border-radius: var(--radius-sm);
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
    border-radius: var(--radius-sm);
    padding: 4px;
    background-color: var(--bg);
  }

  .branch-picker-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 8px;
    border-radius: var(--radius-xs, 3px);
  }

  .branch-picker-item:hover {
    background-color: var(--bg-hover);
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
    background-color: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text);
    cursor: pointer;
  }

  .pick-btn:hover {
    background-color: var(--accent);
    color: var(--accent-contrast, #fff);
    border-color: var(--accent);
  }
</style>
