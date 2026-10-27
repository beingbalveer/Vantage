<script lang="ts">
  import { repos, loadRepos } from "../../domain/stores/repos";
  import { openRepoInFocusedPane } from "../../domain/stores/panes";
  import { addToast, openDialog, branchesVersion } from "../../app/ui";
  import * as api from "../../platform/api";
  import type { BranchInfo, RepoEntry, RepoStatus } from "../../platform/types";
  import Icon from "../common/Icon.svelte";

  interface Props {
    repo: RepoEntry | null;
    status: RepoStatus | null;
    /** Inline mode sits in the app header: content-width trigger, compact row. */
    inline?: boolean;
  }

  let { repo, status, inline = false }: Props = $props();

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
    const _rev = $branchesVersion;
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

  function handleNewBranchFrom(startPoint: string) {
    if (!repo) return;
    closeMenu();
    openDialog({
      type: "branch_create",
      repoId: repo.id,
      data: { startPoint: startPoint || "HEAD" },
    });
  }

  function handleRenameBranch(branch: string) {
    if (!repo) return;
    closeMenu();
    openDialog({
      type: "branch_rename",
      repoId: repo.id,
      data: { from: branch },
    });
  }

  function handleDeleteBranch(branch: string) {
    if (!repo) return;
    closeMenu();
    openDialog({
      type: "branch_delete",
      repoId: repo.id,
      data: { name: branch },
    });
  }
</script>

<div class="branch-dropdown-wrapper" class:inline>
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
    <span class="branch-label">{currentBranch}</span>
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
            <span class="branch-option-name">{b.name}</span>
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
        class="branch-option create-branch-action"
        role="menuitem"
        title={`New branch from ${currentBranch}…`}
        onclick={() => handleNewBranchFrom("HEAD")}
      >
        <Icon name="plus" size={13} />
        <span>New branch from {currentBranch}…</span>
      </button>

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
              <span class="branch-picker-name text-xs">
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
                title="Add branch view"
                aria-label={`Add branch view for ${b.name}`}
              >
                <Icon name="plus" size={12} />
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

  /* Header placement: hug the label instead of filling the pane header. */
  .branch-dropdown-wrapper.inline {
    width: auto;
    max-width: 240px;
  }
  .branch-dropdown-wrapper.inline .branch-trigger-btn {
    width: auto;
    min-height: 26px;
    height: 26px;
    padding: 0 6px;
    gap: 5px;
  }

  .branch-trigger-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    height: 100%;
    min-height: 28px;
    padding: 0 10px;
    border-radius: 0;
    background-color: transparent;
    border: 0;
    color: var(--text);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: background-color 0.15s;
    user-select: none;
    text-align: left;
  }

  .branch-trigger-btn:hover,
  .branch-trigger-btn[aria-expanded="true"] {
    background-color: var(--bg-muted);
  }

  .branch-trigger-btn:focus-visible {
    outline: none;
    box-shadow: inset 0 0 0 1px var(--accent);
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
    font-family: var(--font-ui);
    font-size: 12px;
    font-weight: 500;
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
    top: calc(100% + 6px);
    left: 0;
    z-index: 101;
    width: 280px;
    min-width: 240px;
    max-width: 360px;
    max-height: 420px;
    display: flex;
    flex-direction: column;
    background-color: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--r-md, 8px);
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.22), 0 0 0 1px rgba(0, 0, 0, 0.08);
    padding: 6px;
    overflow: hidden;
  }
  :global([data-theme="dark"]) .branch-menu {
    box-shadow: 0 12px 36px rgba(0, 0, 0, 0.6), 0 0 0 1px rgba(255, 255, 255, 0.1);
  }

  .branch-menu-header {
    padding: 6px 10px 6px;
  }

  .menu-title {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted);
  }

  .menu-search {
    padding: 4px 6px 6px;
  }

  .branch-filter-input {
    width: 100%;
    height: 30px;
    padding: 0 10px;
    font-size: 12.5px;
    background-color: var(--bg-subtle);
    border: 1px solid var(--border);
    border-radius: var(--r-sm, 6px);
    color: var(--text);
  }

  .branch-filter-input:focus {
    outline: none;
    border-color: var(--accent);
    background-color: var(--bg);
    box-shadow: 0 0 0 2px rgba(24, 115, 213, 0.2);
  }

  .branch-list {
    flex: 1;
    overflow-y: auto;
    max-height: 260px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .branch-option {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 7px 10px;
    border: none;
    background: transparent;
    color: var(--text);
    font-family: var(--font-ui);
    font-size: 12.5px;
    font-weight: 500;
    border-radius: var(--r-sm, 6px);
    cursor: pointer;
    text-align: left;
    white-space: nowrap;
    overflow: hidden;
    transition: background-color 0.12s ease, color 0.12s ease;
  }
  .branch-option > span {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .branch-option:hover,
  .branch-option:focus-visible {
    background-color: var(--bg-muted);
    color: var(--text);
    outline: none;
  }

  .branch-option.selected {
    color: var(--accent);
    font-weight: 600;
    background-color: var(--accent-bg, rgba(24, 115, 213, 0.08));
  }

  .branch-option-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-ui);
    font-size: 12.5px;
    line-height: 1.3;
    letter-spacing: -0.01em;
  }

  .head-pill {
    font-size: 9.5px;
    text-transform: uppercase;
    font-weight: 700;
    padding: 2px 5px;
    background-color: rgba(125, 125, 125, 0.18);
    color: var(--text);
    border-radius: var(--r-sm);
    letter-spacing: 0.04em;
  }

  .branch-check {
    font-size: 13px;
    font-weight: 700;
    color: var(--accent);
    width: 16px;
    text-align: right;
  }

  .empty-notice {
    padding: 14px 8px;
    text-align: center;
    font-size: 12px;
    color: var(--text-muted);
  }

  .menu-divider {
    height: 1px;
    background-color: var(--border);
    margin: 6px 0;
    flex-shrink: 0;
  }

  .checkout-action {
    color: var(--text);
    font-weight: 500;
  }

  .add-view-action {
    color: var(--accent);
    font-weight: 500;
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
    border-radius: var(--r-sm);
  }

  .branch-picker-item:hover {
    background-color: var(--bg-muted);
  }

  .branch-picker-name {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--text);
    font-family: var(--font-ui);
    letter-spacing: -0.01em;
  }

  .pick-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    padding: 0;
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
