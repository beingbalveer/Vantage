<script lang="ts">
  import { get } from "svelte/store";
  import { onDestroy } from "svelte";
  import { repos, statuses, loadRepos, refreshSingleStatus } from "../../stores/repos";
  import {
    panes,
    focusedPaneIndex,
    toggleTerminalInPane,
    setPaneTab,
    openRepoInFocusedPane,
  } from "../../stores/panes";
  import { paletteOpen, settingsOpen, shortcutsHelpOpen, sidebarCollapsed, addToast, requestCheckout } from "../../stores/ui";
  import { setOperationRunning, setOperationSuccess, setOperationError } from "../../stores/operation";
  import * as api from "../../api";
  import type { BranchInfo } from "../../types";
  import Icon from "../common/Icon.svelte";

  let currentPane = $derived($panes[$focusedPaneIndex]);
  let currentTab = $derived(currentPane?.tab || "history");
  let isTerminalOpen = $derived(currentPane?.terminalOpen || false);

  let activeRepo = $derived($repos.find((r) => r.id === currentPane?.repoId) || $repos[0] || null);
  let activeStatus = $derived(activeRepo ? $statuses[activeRepo.id] || null : null);
  let currentBranch = $derived(activeRepo?.branch || activeStatus?.branch || "HEAD");
  let isInactiveView = $derived(activeStatus?.isCheckedOut === false);

  let branches = $state<BranchInfo[]>([]);
  let showBranchMenu = $state(false);
  let showBranchModal = $state(false);
  let branchFilter = $state("");
  let showMore = $state(false);

  // Busy state per remote operation so each button reports its own progress.
  let busyOp = $state<null | "fetch" | "pull" | "push">(null);
  let isSyncing = $derived(busyOp !== null);

  let branchTriggerEl = $state<HTMLButtonElement | null>(null);
  let branchMenuEl = $state<HTMLDivElement | null>(null);

  let filteredBranches = $derived(
    branches.filter(
      (b) => !branchFilter || b.name.toLowerCase().includes(branchFilter.toLowerCase())
    )
  );

  // All views (entries) that point at the same working copy as the active view.
  let siblingViews = $derived(
    activeRepo ? $repos.filter((r) => r.path === activeRepo.path) : []
  );

  // A request belongs to a particular repository selection and generation.
  let branchGeneration = 0;
  async function refreshBranches(id: string) {
    const generation = ++branchGeneration;
    try {
      const result = await api.branchList(id);
      if (generation === branchGeneration && activeRepo?.id === id) branches = result;
    } catch {
      if (generation === branchGeneration && activeRepo?.id === id) branches = [];
    }
  }
  $effect(() => {
    const id = activeRepo?.id;
    branches = [];
    showBranchMenu = false;
    if (id) void refreshBranches(id);
    return () => { branchGeneration++; };
  });

  function closeBranchMenu() {
    showBranchMenu = false;
    if (branchMenuEl) branchMenuEl.scrollTop = 0;
  }

  function openBranchMenu(focusTarget: "selected" | "first" = "selected") {
    showBranchMenu = true;
    showMore = false;
    queueMicrotask(() => {
      if (!branchMenuEl) return;
      const options = Array.from(
        branchMenuEl.querySelectorAll<HTMLElement>(".branch-option")
      );
      if (options.length === 0) return;
      const selected = branchMenuEl.querySelector<HTMLElement>(".branch-option.selected");
      (focusTarget === "selected" && selected ? selected : options[0]).focus();
    });
  }

  function handleTriggerKeydown(e: KeyboardEvent) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      openBranchMenu("selected");
    }
  }

  function handleMenuKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      closeBranchMenu();
      branchTriggerEl?.focus();
      return;
    }
    if (e.key === "Tab") {
      closeBranchMenu();
      return;
    }
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (!branchMenuEl) return;
      const items = Array.from(branchMenuEl.querySelectorAll<HTMLElement>(".branch-option"));
      const i = items.indexOf(document.activeElement as HTMLElement);
      items[(i + (e.key === "ArrowDown" ? 1 : -1) + items.length) % items.length]?.focus();
    }
  }

  /**
   * Selecting a branch never checks out on disk implicitly. If a view already
   * exists for that branch we focus it; otherwise we add a new branch view,
   * which mirrors the backend's `repo_add(path, branch)` contract.
   */
  async function selectBranch(branch: string) {
    if (!activeRepo) return;
    closeBranchMenu();
    if (branch === currentBranch) {
      branchTriggerEl?.focus();
      return;
    }
    const existing = siblingViews.find((r) => r.branch === branch);
    if (existing) {
      openRepoInFocusedPane(existing.id);
      return;
    }
    try {
      const entry = await api.repoAdd(activeRepo.path, branch);
      await loadRepos();
      openRepoInFocusedPane(entry.id);
      addToast({ message: `Opened ${branch}`, kind: "success" });
    } catch (e: any) {
      addToast({ message: e.message || "Failed to open branch", kind: "error" });
      branchTriggerEl?.focus();
    }
  }

  async function handleAddBranchView(targetBranch: string) {
    if (!activeRepo) return;
    const b = targetBranch.trim();
    if (!b) return;
    showBranchMenu = false;
    showBranchModal = false;
    try {
      const entry = await api.repoAdd(activeRepo.path, b);
      addToast({ message: `Added view for branch ${b}`, kind: "success" });
      await loadRepos();
      openRepoInFocusedPane(entry.id);
    } catch (e: any) {
      addToast({ message: e.message || "Failed to add branch view", kind: "error" });
    }
  }

  function handleOpenBranchPicker() {
    showBranchMenu = false;
    branchFilter = "";
    showBranchModal = true;
  }

  function handleToggleTerminal() {
    toggleTerminalInPane($focusedPaneIndex);
  }

  function handleToggleSidebar() {
    $sidebarCollapsed = !$sidebarCollapsed;
  }

  let isCheckingOut = $state(false);
  async function checkoutCurrentBranch() {
    if (!activeRepo || isCheckingOut || isSyncing) return;
    const target = { id: activeRepo.id, branch: currentBranch };
    closeBranchMenu();
    if ((activeStatus?.dirtyCount ?? 0) > 0) {
      // One prompt resolves the dirty working tree, then continues the checkout.
      requestCheckout({
        repoId: target.id,
        repoName: activeRepo.name,
        target: target.branch,
        dirtyCount: activeStatus?.dirtyCount ?? 0,
      });
      return;
    }
    isCheckingOut = true;
    try {
      await api.checkout(target.id, target.branch);
      // All branch views of the same working copy need new isCheckedOut status.
      await loadRepos();
      if (activeRepo?.id === target.id) await refreshBranches(target.id);
      addToast({ message: `Checked out ${target.branch}`, kind: "success" });
    } catch (e: any) {
      addToast({ message: e.message || "Checkout failed", kind: "error" });
    } finally { isCheckingOut = false; }
  }

  async function runGit(operation: "fetch" | "pull" | "push") {
    if (!activeRepo || isSyncing || isCheckingOut) return;
    const label = operation === "fetch" ? "Fetch" : operation === "pull" ? "Pull" : "Push";
    if (operation !== "fetch" && isInactiveView) {
      addToast({
        message: "Checkout this branch before pulling or pushing.",
        kind: "error",
      });
      return;
    }
    const target = { id: activeRepo.id, name: activeRepo.name, branch: currentBranch, hasUpstream: !!activeStatus?.upstream };
    busyOp = operation;
    setOperationRunning(`${label} · ${target.name}`, target.id, target.branch);
    try {
      if (operation === "fetch") {
        await api.syncFetch(target.id);
        setOperationSuccess(`Fetched ${target.name}`);
      } else if (operation === "pull") {
        await api.syncPull(target.id);
        setOperationSuccess(`Pulled ${target.name}`);
      } else {
        await api.syncPush(target.id, !target.hasUpstream);
        setOperationSuccess(target.hasUpstream ? `Pushed ${target.name}` : `Published ${target.branch}`);
      }
      await refreshSingleStatus(target.id);
      if (activeRepo?.id === target.id) await refreshBranches(target.id);
    } catch (e: any) {
      setOperationError(`${label} failed`, e.message || String(e));
      addToast({ message: e.message || `${label} failed`, kind: "error" });
    } finally {
      busyOp = null;
    }
  }

  function handleWindowKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      if (showBranchMenu) closeBranchMenu();
      if (showMore) showMore = false;
    }
  }

  onDestroy(() => {
    branchGeneration++;
    showBranchMenu = false;
    showMore = false;
  });
</script>

<svelte:window onkeydown={handleWindowKeydown} />

<header class="top-header" data-tauri-drag-region>
  <div class="traffic-lights-spacer" data-tauri-drag-region></div>

  <button
    class="icon-button"
    onclick={handleToggleSidebar}
    title="Toggle repositories (⌘B)"
    aria-label="Toggle repositories"
    aria-expanded={!$sidebarCollapsed}
  >
    <Icon name="sidebar" size={18} />
  </button>

  {#if activeRepo}
    <div class="header-context">
      <span class="repo-heading" title={activeRepo.path}>{activeRepo.name}</span>
      <span class="head-divider"></span>
      <div class="branch-heading">
        <Icon name="git-branch" size={17} class="branch-glyph" />
        <button
          bind:this={branchTriggerEl}
          class="branch-trigger"
          aria-haspopup="menu"
          aria-expanded={showBranchMenu}
          onclick={() => (showBranchMenu ? closeBranchMenu() : openBranchMenu("selected"))}
          onkeydown={handleTriggerKeydown}
          title={currentBranch}
        >
          {currentBranch}
        </button>
        <Icon name="chevron-down" size={16} class="chevron" />

        {#if showBranchMenu}
          <div
            class="header-menu-backdrop"
            role="presentation"
            onclick={closeBranchMenu}
          ></div>
          <div
            bind:this={branchMenuEl}
            class="branch-menu"
            role="menu"
            aria-label="Branches"
            tabindex="-1"
            onkeydown={handleMenuKeydown}
          >
            <div class="branch-menu-label">Switch branch</div>
            {#each filteredBranches as b (b.name)}
              <button
                class="branch-option"
                class:selected={currentBranch === b.name}
                role="menuitemradio"
                aria-checked={currentBranch === b.name}
                tabindex={currentBranch === b.name ? 0 : -1}
                onclick={() => selectBranch(b.name)}
              >
                <Icon name="git-branch" size={15} />
                <span>{b.name}</span>
                <span class="branch-check">{currentBranch === b.name ? "✓" : ""}</span>
              </button>
            {/each}
            {#if !filteredBranches.length}
              <div class="branch-menu-label">No branches available</div>
            {/if}
            {#if isInactiveView}
              <button class="branch-option" role="menuitem" disabled={isCheckingOut || isSyncing} onclick={checkoutCurrentBranch}>
                <Icon name="check" size={15}/><span>{isCheckingOut ? "Checking out…" : "Check out this branch"}</span>
              </button>
            {/if}
            <button
              class="branch-option add-view"
              role="menuitem"
              onclick={handleOpenBranchPicker}
            >
              <Icon name="plus" size={15} />
              <span>Add branch view…</span>
            </button>
          </div>
        {/if}
      </div>
    </div>
  {/if}

  <div class="operations">
    {#each ["Fetch", "Pull", "Push"] as operation}
      {@const key = operation.toLowerCase() as "fetch" | "pull" | "push"}
      {@const isBusy = busyOp === key}
      <button
        class="operation"
        class:is-busy={isBusy}
        disabled={!activeRepo || isSyncing || isCheckingOut || (key !== "fetch" && isInactiveView)}
        onclick={() => runGit(key)}
        title={key === "fetch"
          ? (isBusy ? "Fetching from origin…" : "Fetch from origin")
          : key === "pull"
            ? (isBusy ? "Pulling from origin…" : "Pull from origin")
            : (isBusy ? "Pushing to origin…" : "Push to origin")}
      >
        <span class="op-icon" class:spinning={isBusy && key === "fetch"}>
          <Icon
            name={key === "fetch" ? "refresh" : key === "pull" ? "arrow-down" : "arrow-up"}
            size={16}
          />
        </span>
        <span class="label">
          {isBusy ? (key === "fetch" ? "Fetching…" : key === "pull" ? "Pulling…" : "Pushing…") : operation}
        </span>
        {#if key === "pull" && activeStatus?.behind && activeStatus.behind > 0}
          <span class="op-count" title="{activeStatus.behind} commit{activeStatus.behind === 1 ? '' : 's'} on server to pull">
            {activeStatus.behind}
          </span>
        {:else if key === "push" && activeStatus?.ahead && activeStatus.ahead > 0}
          <span class="op-count" title="{activeStatus.ahead} unpushed commit{activeStatus.ahead === 1 ? '' : 's'}">
            {activeStatus.ahead}
          </span>
        {/if}
      </button>
    {/each}

    <span class="operation-divider"></span>

    <button
      class="icon-button"
      disabled={!activeRepo}
      onclick={handleOpenBranchPicker}
      title="Add branch view"
      aria-label="Add branch view"
    >
      <Icon name="git-branch" size={17} />
    </button>
    <button
      class="icon-button"
      onclick={handleToggleTerminal}
      disabled={!activeRepo}
      title={isTerminalOpen ? "Hide terminal" : "Show terminal"}
      aria-label="Toggle terminal"
      aria-pressed={isTerminalOpen}
    >
      <Icon name="terminal" size={17} />
    </button>
  </div>

  <div class="right">
    <button
      class="icon-button"
      onclick={() => {
        showBranchMenu = false;
        showMore = !showMore;
      }}
      title="More commands"
      aria-label="More commands"
      aria-haspopup="menu"
      aria-expanded={showMore}
    >
      <Icon name="more-horizontal" size={18} />
    </button>
    <button
      class="icon-button"
      onclick={() => ($settingsOpen = true)}
      title="Preferences (⌘,)"
      aria-label="Preferences"
    >
      <Icon name="gear" size={17} />
    </button>

    {#if showMore}
      <div class="header-menu-backdrop" role="presentation" onclick={() => (showMore = false)}></div>
      <div class="more-menu" role="menu" aria-label="More commands" tabindex="-1">
        <button
          class="branch-option"
          role="menuitem"
          onclick={() => {
            setPaneTab($focusedPaneIndex, "history");
            showMore = false;
          }}
        >
          Changes and history
        </button>
        <button
          class="branch-option"
          role="menuitem"
          onclick={() => {
            setPaneTab($focusedPaneIndex, "files");
            showMore = false;
          }}
        >
          Browse project files
        </button>
        <button
          class="branch-option"
          role="menuitem"
          onclick={() => {
            $paletteOpen = true;
            showMore = false;
          }}
        >
          Command palette <span class="menu-shortcut">⌘K</span>
        </button>
        <button
          class="branch-option"
          role="menuitem"
          onclick={() => {
            $shortcutsHelpOpen = true;
            showMore = false;
          }}
        >
          Keyboard shortcuts <span class="menu-shortcut">?</span>
        </button>
      </div>
    {/if}
  </div>
</header>

{#if showBranchModal && activeRepo}
  <div
    class="modal-backdrop"
    role="presentation"
    onclick={() => (showBranchModal = false)}
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
        <span class="text-strong">Add branch view — {activeRepo.name}</span>
        <button
          type="button"
          class="dialog-close-btn"
          onclick={() => (showBranchModal = false)}
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
                  <span class="head-pill text-xs">current</span>
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
  .top-header {
    height: 50px;
    min-height: 50px;
    flex-shrink: 0;
    padding: 0 20px;
    display: flex;
    align-items: center;
    gap: 13px;
    background: var(--chrome);
    border: 0;
    position: relative;
    color: #353a40;
    z-index: 40;
    user-select: none;
  }

  .traffic-lights-spacer {
    width: 70px;
    flex-shrink: 0;
  }

  .icon-button {
    width: 30px;
    height: 30px;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    border: 0;
    background: none;
    border-radius: 7px;
    color: #3d454e;
    cursor: pointer;
    padding: 0;
  }
  .icon-button:hover:not(:disabled) {
    background: #e1e3e5;
    color: #171d24;
  }
  .icon-button:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .header-context {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
    margin-left: 8px;
  }

  .repo-heading {
    font-size: 16px;
    font-weight: 550;
    letter-spacing: -0.16px;
    color: #20252a;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 320px;
  }

  .head-divider {
    height: 17px;
    width: 1px;
    background: #cfd3d7;
    flex-shrink: 0;
  }

  .branch-heading {
    display: flex;
    align-items: center;
    gap: 8px;
    position: relative;
    min-width: 0;
    /* Container text style from the prototype; the trigger below overrides. */
    font-size: 12px;
    color: #454d56;
  }
  .branch-heading :global(.branch-glyph) {
    width: 17px;
    height: 17px;
    color: #8050b0;
    flex-shrink: 0;
  }
  .branch-heading :global(.chevron) {
    pointer-events: none;
    color: #414b57;
    flex-shrink: 0;
    margin-left: -24px;
  }

  .branch-trigger {
    font-size: 14px;
    font-weight: 500;
    color: #303740;
    border: 0;
    background: transparent;
    border-radius: 6px;
    padding: 7px 28px 7px 4px;
    max-width: 270px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: pointer;
    text-align: left;
  }
  .branch-trigger:hover,
  .branch-trigger[aria-expanded="true"] {
    background: #e6e8ea;
  }
  .branch-trigger:focus-visible {
    outline: 2px solid #9cb3cf;
    outline-offset: 1px;
  }

  .operations {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 3px;
    flex-shrink: 0;
  }

  .operation {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 33px;
    padding: 0 9px;
    font-size: 13px;
    font-weight: 500;
    color: #3d454e;
    background: transparent;
    border: 0;
    border-radius: 7px;
    cursor: pointer;
    white-space: nowrap;
    transition: background 100ms ease, color 100ms ease;
  }
  .operation:hover:not(:disabled) {
    background: #e1e3e5;
    color: #171d24;
  }
  :global([data-theme="dark"]) .operation {
    color: #c9d1d9;
  }
  :global([data-theme="dark"]) .operation:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.1);
    color: #f0f6fc;
  }
  .operation:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .operation.is-busy {
    opacity: 0.9 !important;
    cursor: wait !important;
  }

  .op-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    line-height: 1;
  }
  .op-icon.spinning {
    animation: spin-refresh 0.9s cubic-bezier(0.4, 0, 0.2, 1) infinite;
  }
  @keyframes spin-refresh {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }

  .op-count {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 17px;
    height: 17px;
    padding: 0 5px;
    font-size: 11px;
    font-weight: 600;
    line-height: 1;
    border-radius: 9px;
    background: rgba(0, 0, 0, 0.08);
    color: #3d454e;
    font-variant-numeric: tabular-nums;
  }
  :global([data-theme="dark"]) .op-count {
    background: rgba(255, 255, 255, 0.14);
    color: #e6edf3;
  }

  .operation-divider {
    height: 16px;
    width: 1px;
    background: #cfd3d7;
    margin: 0 8px;
  }

  .right {
    display: flex;
    align-items: center;
    gap: 2px;
    margin-left: 2px;
    padding-left: 8px;
    border-left: 1px solid #cfd3d7;
    position: relative;
  }

  .header-menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 110;
    background: transparent;
  }

  .branch-menu,
  .more-menu {
    position: absolute;
    top: calc(100% + 8px);
    padding: 5px;
    background: var(--bg);
    border-radius: 10px;
    box-shadow: 0 7px 28px rgba(0, 0, 0, 0.11), 0 0 0 1px rgba(0, 0, 0, 0.03);
    z-index: 120;
    max-height: 420px;
    overflow: auto;
  }
  .branch-menu {
    left: 19px;
    min-width: 245px;
    width: max-content;
    max-width: 330px;
  }
  .more-menu {
    right: 0;
    min-width: 220px;
  }

  .branch-menu-label {
    font-size: 11px;
    color: #8b9096;
    padding: 8px 10px 7px;
  }

  .branch-option {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 9px;
    padding: 9px 10px;
    border: 0;
    background: none;
    border-radius: 6px;
    justify-content: flex-start;
    text-align: left;
    font-size: 13px;
    color: #454b53;
    white-space: nowrap;
    cursor: pointer;
  }
  .branch-option:hover,
  .branch-option:focus-visible {
    background: #e9ebed;
    outline: none;
  }
  .branch-option :global(svg) {
    width: 15px;
    height: 15px;
    color: #90949b;
    flex-shrink: 0;
  }
  .branch-option > span:not(.branch-check):not(.menu-shortcut) {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .branch-option.selected {
    background: #f0f1f2;
    color: #30363d;
  }
  .branch-option.selected :global(svg) {
    color: #8953c4;
  }
  .branch-check {
    margin-left: auto;
    color: #626b76;
    font-size: 13px;
    padding-left: 16px;
  }
  .menu-shortcut {
    margin-left: auto;
    color: #8b9096;
    font-size: 11px;
  }
  .branch-option.add-view {
    border-top: 1px solid var(--border-subtle);
    margin-top: 4px;
    padding-top: 10px;
  }

  /* Add-branch-view modal */
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
  .add-branch-modal-btn {
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

  @media (max-width: 1100px) {
    .operation > .label {
      display: none;
    }
    .operation {
      padding: 0 7px;
    }
    .repo-heading {
      max-width: 180px;
    }
    .branch-trigger {
      max-width: 170px;
    }
    .top-header {
      gap: 8px;
      padding-right: 12px;
    }
    .header-context {
      gap: 8px;
    }
  }
</style>
