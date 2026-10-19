<script lang="ts">
  import type { RepoEntry, RepoStatus, BranchInfo } from "../../types";
  import * as api from "../../api";
  import { paneCount, panes, toggleTerminalInPane, setPaneTab, closePane as removePane } from "../../stores/panes";
  import { repos, loadRepos, removeRepo } from "../../stores/repos";
  import { addToast, openDialog, branchesVersion } from "../../stores/ui";
  import IconButton from "../common/IconButton.svelte";
  import { get } from "svelte/store";

  interface Props {
    paneIndex: number;
    repo: RepoEntry | null;
    status: RepoStatus | null;
  }

  let { paneIndex, repo, status }: Props = $props();

  let branches = $state<BranchInfo[]>([]);
  let showBranchMenu = $state(false);
  let showRepoMenu = $state(false);
  let showBranchModal = $state(false);
  let branchFilter = $state("");
  let isSyncing = $state(false);

  let filteredBranches = $derived(
    branches.filter(
      (b) => !branchFilter || b.name.toLowerCase().includes(branchFilter.toLowerCase())
    )
  );

  let currentPane = $derived($panes[paneIndex]);
  let currentTab = $derived(currentPane?.tab || "history");
  let isTerminalOpen = $derived(currentPane?.terminalOpen || false);

  async function handleCopyRepoPath() {
    if (!repo) return;
    showRepoMenu = false;
    await navigator.clipboard.writeText(repo.path);
    addToast({ message: "Repository path copied to clipboard", kind: "success" });
  }

  async function handleRevealRepo() {
    if (!repo) return;
    showRepoMenu = false;
    try {
      await api.revealInFileManager(repo.path);
    } catch (e: any) {
      addToast({ message: e.message || "Failed to reveal in file manager", kind: "error" });
    }
  }

  function handleRemoveRepo() {
    if (!repo) return;
    showRepoMenu = false;
    openDialog({ type: "remove_repo", repoId: repo.id, data: { repo } });
  }

  $effect(() => {
    // Re-fetch whenever a branch create/rename/delete bumps the version.
    const _branchRev = $branchesVersion;
    if (repo?.id) {
      api.branchList(repo.id).then((b) => {
        branches = b;
      }).catch(() => {});
    }
  });

  function handleNewBranch() {
    if (!repo) return;
    showBranchMenu = false;
    const startPoint = status?.branch || repo.branch || "HEAD";
    openDialog({ type: "branch_create", repoId: repo.id, data: { startPoint } });
  }

  async function handleAddBranchView(targetBranch: string) {
    if (!repo) return;
    const b = targetBranch.trim();
    if (!b) return;
    showBranchMenu = false;
    showRepoMenu = false;
    showBranchModal = false;
    try {
      await api.repoAdd(repo.path, b);
      addToast({ message: `Added view for branch ${b}`, kind: "success" });
      await loadRepos();
    } catch (e: any) {
      addToast({ message: e.message || "Failed to add branch view", kind: "error" });
    }
  }

  function handleOpenBranchPicker() {
    showRepoMenu = false;
    branchFilter = "";
    showBranchModal = true;
  }

  async function handleCheckout(branch: string) {
    if (!repo) return;
    showBranchMenu = false;
    try {
      await api.checkout(repo.id, branch);
      addToast({ message: `Checked out ${branch}`, kind: "success" });
    } catch (e: any) {
      addToast({ message: e.message || "Checkout failed", kind: "error" });
    }
  }

  async function handleSync() {
    if (!repo || isSyncing) return;
    isSyncing = true;
    try {
      if (status?.ahead && status.ahead > 0 && (!status.behind || status.behind === 0)) {
        await api.syncPush(repo.id, false);
        addToast({ message: `Pushed commits to origin`, kind: "success" });
      } else if (status?.behind && status.behind > 0 && (!status.ahead || status.ahead === 0)) {
        await api.syncPull(repo.id);
        addToast({ message: `Pulled latest commits`, kind: "success" });
      } else if (!status?.upstream) {
        await api.syncPush(repo.id, true);
        addToast({ message: `Published branch`, kind: "success" });
      } else {
        await api.syncFetch(repo.id);
        addToast({ message: `Fetched from origin`, kind: "info" });
      }
    } catch (e: any) {
      addToast({ message: e.message || "Sync operation failed", kind: "error" });
    } finally {
      isSyncing = false;
    }
  }

  function closePane() {
    removePane(paneIndex);
  }
</script>

<div class="pane-header">
  <div class="left-section">
    {#if $paneCount > 1}
      <span class="pane-indicator">
        {paneIndex === 0 ? "①" : paneIndex === 1 ? "②" : "③"}
      </span>
    {/if}

    <span class="repo-title text-strong">
      {repo ? repo.name : "No repository selected"}
    </span>
    {#if repo?.branch}
      <span class="repo-branch-tag font-mono">⎇ {repo.branch}</span>
    {/if}

    {#if repo}
      <div class="repo-actions-wrap">
        <button
          type="button"
          class="repo-menu-trigger"
          title="Repository options"
          onclick={() => (showRepoMenu = !showRepoMenu)}
        >
          ⋯
        </button>

        {#if showRepoMenu}
          <div
            class="header-menu-backdrop"
            role="presentation"
            onclick={() => (showRepoMenu = false)}
            onkeydown={(e) => {
              if (e.key === 'Escape') showRepoMenu = false;
            }}
          ></div>
          <div
            class="repo-header-dropdown"
            role="menu"
            tabindex="-1"
            onclick={(e) => e.stopPropagation()}
            onkeydown={(e) => {
              if (e.key === 'Escape') showRepoMenu = false;
            }}
          >
            <button class="menu-dropdown-item" onclick={handleOpenBranchPicker}>
              Add another branch view…
            </button>
            <div class="menu-dropdown-divider"></div>
            <button class="menu-dropdown-item" onclick={handleCopyRepoPath}>
              Copy path
            </button>
            <button class="menu-dropdown-item" onclick={handleRevealRepo}>
              Reveal in file manager
            </button>
            <div class="menu-dropdown-divider"></div>
            <button class="menu-dropdown-item danger" onclick={handleRemoveRepo}>
              Remove from Relay…
            </button>
          </div>
        {/if}
      </div>
    {/if}

    <!-- Branch selector -->
    {#if repo}
      <div class="branch-wrap">
        <button
          class="branch-btn"
          onclick={() => (showBranchMenu = !showBranchMenu)}
        >
          ⎇ {status?.branch || "main"} ▾
        </button>

        {#if showBranchMenu}
          <div class="branch-dropdown" role="menu">
            <div class="branch-list">
              {#each branches as b}
                <div class="branch-menu-row {b.isHead ? 'current' : ''}">
                  <button
                    class="branch-item"
                    onclick={() => handleCheckout(b.name)}
                    title="Checkout {b.name} in working directory"
                  >
                    {#if b.isHead}✓ {/if}{b.name}
                  </button>
                  <button
                    class="add-branch-view-btn"
                    title="Add {b.name} as a view in sidebar"
                    onclick={(e) => {
                      e.stopPropagation();
                      handleAddBranchView(b.name);
                    }}
                  >
                    + View
                  </button>
                </div>
              {/each}
            </div>
            <div class="branch-dropdown-divider"></div>
            <button
              class="branch-item"
              onclick={handleNewBranch}
              title="Create a new branch from the current one"
            >
              + New branch from {status?.branch || repo.branch || "HEAD"}…
            </button>
          </div>
        {/if}
      </div>

      <!-- Sync button -->
      <button
        class="sync-btn {status?.ahead || status?.behind ? 'primary' : ''}"
        onclick={handleSync}
        disabled={isSyncing}
      >
        {#if isSyncing}
          <span>⟳ Syncing…</span>
        {:else if !status?.upstream}
          <span>⇪ Publish branch</span>
        {:else if status?.ahead && status.ahead > 0 && (!status.behind || status.behind === 0)}
          <span>↑ Push origin {status.ahead}</span>
        {:else if status?.behind && status.behind > 0 && (!status.ahead || status.ahead === 0)}
          <span>↓ Pull origin {status.behind}</span>
        {:else if status?.ahead && status?.behind}
          <span>⇅ Diverged ↑{status.ahead} ↓{status.behind}</span>
        {:else}
          <span>⟳ Fetch origin</span>
        {/if}
      </button>
    {/if}
  </div>

  <div class="right-section">
    <!-- Tabs: History | Files -->
    {#if repo}
      <div class="segmented-tabs">
        <button
          class="tab-btn {currentTab === 'history' ? 'active' : ''}"
          onclick={() => setPaneTab(paneIndex, "history")}
        >
          History
        </button>
        <button
          class="tab-btn {currentTab === 'files' ? 'active' : ''}"
          onclick={() => setPaneTab(paneIndex, "files")}
        >
          Files
        </button>
      </div>

      <!-- Terminal Drawer Button -->
      <IconButton
        title="Toggle terminal (Mod+`)"
        active={isTerminalOpen}
        onclick={() => toggleTerminalInPane(paneIndex)}
      >
        <span>&gt;_</span>
      </IconButton>
    {/if}

    <!-- Close Pane (if 2+ panes) -->
    {#if $paneCount > 1}
      <IconButton title="Close pane" onclick={closePane}>
        ✕
      </IconButton>
    {/if}
  </div>

  <!-- Add Branch View Modal -->
  {#if showBranchModal && repo}
    <div
      class="modal-backdrop"
      role="presentation"
      onclick={(e) => {
        e.stopPropagation();
        showBranchModal = false;
      }}
      onkeydown={(e) => {
        if (e.key === 'Escape') showBranchModal = false;
      }}
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
            class="dialog-close-btn"
            onclick={() => (showBranchModal = false)}
          >
            ✕
          </button>
        </div>
        <p class="dialog-desc text-xs text-muted">
          Add another view of this repository for a different branch. You can compare and browse multiple branches at the same time across split panes.
        </p>

        <div class="dialog-search">
          <input
            type="text"
            placeholder="Search or enter branch name…"
            bind:value={branchFilter}
            class="dialog-branch-input"
            onkeydown={(e) => {
              if (e.key === 'Enter' && branchFilter.trim()) {
                e.preventDefault();
                handleAddBranchView(branchFilter.trim());
              }
            }}
          />
          {#if branchFilter.trim() && !branches.some(b => b.name === branchFilter.trim())}
            <button
              class="add-branch-btn primary"
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
              <div class="branch-picker-item {b.isHead ? 'is-head' : ''}">
                <span class="branch-picker-name font-mono text-xs">
                  ⎇ {b.name}
                  {#if b.isHead}
                    <span class="head-pill text-xs">current</span>
                  {/if}
                </span>
                <button
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
</div>

<style>
  .pane-header {
    height: 44px;
    background-color: var(--bg-subtle);
    border-bottom: 1px solid var(--border);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 12px;
    gap: 12px;
    flex-shrink: 0;
  }

  .left-section,
  .right-section {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .pane-indicator {
    font-size: 14px;
    color: var(--accent);
    font-weight: 700;
  }

  .repo-title {
    font-size: 13px;
    color: var(--text);
  }

  .branch-wrap {
    position: relative;
  }

  .branch-btn {
    height: 24px;
    padding: 0 8px;
    border-radius: var(--r-md);
    background-color: var(--bg);
    border: 1px solid var(--border);
    color: var(--text);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
  }
  .branch-btn:hover {
    background-color: var(--bg-muted);
  }

  .branch-dropdown {
    position: absolute;
    top: 28px;
    left: 0;
    width: 200px;
    background-color: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    box-shadow: var(--shadow-pop);
    z-index: 100;
    padding: 4px;
  }

  .branch-item {
    width: 100%;
    height: 26px;
    text-align: left;
    background: transparent;
    border: none;
    border-radius: var(--r-sm);
    padding: 0 8px;
    font-size: 12px;
    color: var(--text);
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .branch-dropdown-divider {
    height: 1px;
    background-color: var(--border-subtle);
    margin: 4px 0;
  }
  .branch-item:hover {
    background-color: var(--bg-muted);
  }

  .sync-btn {
    height: 24px;
    padding: 0 10px;
    border-radius: var(--r-md);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    border: 1px solid var(--border);
    background-color: var(--bg);
    color: var(--text);
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .sync-btn.primary {
    background-color: var(--accent);
    color: var(--accent-fg);
    border-color: transparent;
  }
  .sync-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .segmented-tabs {
    display: flex;
    background-color: var(--bg-muted);
    border-radius: var(--r-md);
    padding: 2px;
    gap: 2px;
  }

  .tab-btn {
    height: 22px;
    padding: 0 10px;
    border-radius: var(--r-sm);
    border: none;
    background: transparent;
    color: var(--text-muted);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
  }
  .tab-btn.active {
    background-color: var(--bg);
    color: var(--text);
    font-weight: 600;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.05);
  }

  .repo-actions-wrap {
    position: relative;
    display: flex;
    align-items: center;
  }

  .repo-menu-trigger {
    height: 22px;
    padding: 0 6px;
    background: transparent;
    border: none;
    border-radius: var(--r-sm);
    color: var(--text-muted);
    font-size: 14px;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .repo-menu-trigger:hover {
    background-color: var(--bg-muted);
    color: var(--text);
  }

  .header-menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 99;
  }

  .repo-header-dropdown {
    position: absolute;
    top: 28px;
    left: 0;
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

  .menu-dropdown-item {
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
  .menu-dropdown-item:hover {
    background-color: var(--bg-subtle);
  }
  .menu-dropdown-item.danger {
    color: var(--danger);
  }
  .menu-dropdown-item.danger:hover {
    background-color: var(--danger-bg);
  }

  .menu-dropdown-divider {
    height: 1px;
    background-color: var(--border-subtle);
    margin: 3px 0;
  }

  .repo-branch-tag {
    font-size: 11px;
    font-weight: 500;
    padding: 1px 6px;
    border-radius: var(--r-sm);
    background-color: var(--accent-bg);
    color: var(--accent);
    border: 1px solid var(--border);
  }

  .branch-menu-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 4px;
    padding: 2px 4px;
    border-radius: var(--r-sm);
  }
  .branch-menu-row:hover {
    background-color: var(--bg-muted);
  }
  .branch-menu-row.current {
    background-color: var(--accent-bg);
  }

  .add-branch-view-btn {
    height: 20px;
    padding: 0 6px;
    font-size: 10px;
    font-weight: 600;
    border-radius: var(--r-sm);
    border: 1px solid var(--border);
    background-color: var(--bg);
    color: var(--text);
    cursor: pointer;
    white-space: nowrap;
    flex-shrink: 0;
  }
  .add-branch-view-btn:hover {
    background-color: var(--accent);
    color: var(--accent-fg);
    border-color: var(--accent);
  }

  .modal-backdrop {
    position: fixed;
    inset: 0;
    background-color: rgba(0, 0, 0, 0.45);
    z-index: 500;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .branch-dialog {
    width: 380px;
    max-height: 480px;
    background-color: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--r-lg);
    box-shadow: var(--shadow-pop);
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .dialog-title {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .dialog-close-btn {
    background: transparent;
    border: none;
    cursor: pointer;
    color: var(--text-muted);
    font-size: 14px;
    padding: 2px 6px;
    border-radius: var(--r-sm);
  }
  .dialog-close-btn:hover {
    background-color: var(--bg-muted);
    color: var(--text);
  }

  .dialog-desc {
    line-height: 1.4;
  }

  .dialog-search {
    display: flex;
    gap: 8px;
  }

  .dialog-branch-input {
    flex: 1;
    height: 30px;
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
    height: 30px;
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

  .empty-branches {
    padding: 16px;
    text-align: center;
  }
</style>
