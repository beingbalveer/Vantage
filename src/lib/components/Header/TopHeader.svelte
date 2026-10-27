<script lang="ts">
  import { get } from "svelte/store";
  import { onDestroy, onMount } from "svelte";
  import { repos, statuses, loadRepos, refreshSingleStatus } from "../../domain/stores/repos";
  import {
    panes,
    focusedPaneIndex,
    toggleTerminalInPane,
    setPaneTab,
  } from "../../domain/stores/panes";
  import { paletteOpen, settingsOpen, shortcutsHelpOpen, sidebarCollapsed, addToast } from "../../app/ui";
  import { setOperationRunning, setOperationSuccess, setOperationError } from "../../domain/stores/operation";
  import { formatRelativeTime } from "../../domain/format";
  import { registerDismiss } from "../../app/commands";
  import * as api from "../../platform/api";
  import Icon from "../common/Icon.svelte";
  import { Modal } from "../common";
  import BranchDropdown from "../Pane/BranchDropdown.svelte";

  let currentPane = $derived($panes[$focusedPaneIndex]);
  let currentTab = $derived(currentPane?.tab || "history");
  let isTerminalOpen = $derived(currentPane?.terminalOpen || false);

  let activeRepo = $derived($repos.find((r) => r.id === currentPane?.repoId) || $repos[0] || null);
  let activeStatus = $derived(activeRepo ? $statuses[activeRepo.id] || null : null);
  let currentBranch = $derived(activeRepo?.branch || activeStatus?.branch || "HEAD");
  let isInactiveView = $derived(activeStatus?.isCheckedOut === false);

  let showMore = $state(false);

  // Busy state per remote operation so button reports its own progress.
  let busyOp = $state<null | "fetch" | "pull" | "push">(null);
  let isSyncing = $derived(busyOp !== null);

  // Timestamps of last successful fetch per repo ID
  let lastFetchedMap = $state<Record<string, number>>({});
  let clockNow = $state(Date.now());

  $effect(() => {
    const timer = setInterval(() => {
      clockNow = Date.now();
    }, 30000);
    return () => clearInterval(timer);
  });

  // Calculate the primary action according to git state (GitHub Desktop style)
  let actionType = $derived.by<"pull" | "push" | "fetch">(() => {
    if (!activeStatus) return "fetch";
    if (activeStatus.behind > 0) return "pull";
    if (activeStatus.ahead > 0 || !activeStatus.upstream) return "push";
    return "fetch";
  });

  let hasUpstream = $derived(!!activeStatus?.upstream);

  let syncLabel = $derived.by(() => {
    if (busyOp === "fetch") return "Fetching…";
    if (busyOp === "pull") return "Pulling…";
    if (busyOp === "push") return hasUpstream ? "Pushing…" : "Publishing…";

    if (actionType === "pull") return "Pull";
    if (actionType === "push") return hasUpstream ? "Push" : "Publish";
    return "Fetch";
  });

  let syncIconName = $derived.by<"refresh" | "arrow-down" | "arrow-up">(() => {
    if (busyOp || actionType === "fetch") return "refresh";
    if (actionType === "pull") return "arrow-down";
    return "arrow-up";
  });

  let syncCount = $derived.by(() => {
    if (busyOp) return 0;
    if (actionType === "pull") return activeStatus?.behind || 0;
    if (actionType === "push" && hasUpstream) return activeStatus?.ahead || 0;
    return 0;
  });

  let syncTooltip = $derived.by(() => {
    void clockNow;
    if (busyOp === "fetch") return "Fetching from origin…";
    if (busyOp === "pull") return "Pulling from origin…";
    if (busyOp === "push") return hasUpstream ? "Pushing to origin…" : "Publishing branch to origin…";

    if (actionType === "pull") {
      const behind = activeStatus?.behind || 0;
      const ahead = activeStatus?.ahead || 0;
      if (ahead > 0) {
        return `Pull ${behind} commit${behind === 1 ? "" : "s"}, push ${ahead} commit${ahead === 1 ? "" : "s"}`;
      }
      return `Pull ${behind} commit${behind === 1 ? "" : "s"} from origin`;
    }

    if (actionType === "push") {
      if (!hasUpstream) return "Publish this branch to origin";
      const ahead = activeStatus?.ahead || 0;
      return `Push ${ahead} commit${ahead === 1 ? "" : "s"} to origin`;
    }

    const lastTime = activeRepo ? lastFetchedMap[activeRepo.id] : null;
    if (lastTime) {
      const rel = formatRelativeTime(new Date(lastTime).toISOString());
      return rel === "just now" ? "Fetch from origin · Last fetched just now" : `Fetch from origin · Last fetched ${rel}`;
    }
    if (activeStatus?.lastActivityAt) {
      return `Fetch from origin · Active ${formatRelativeTime(activeStatus.lastActivityAt)}`;
    }
    return "Fetch from origin";
  });

  function handleToggleTerminal() {
    toggleTerminalInPane($focusedPaneIndex);
  }

  function handleToggleSidebar() {
    $sidebarCollapsed = !$sidebarCollapsed;
  }

  async function runGit(operation: "fetch" | "pull" | "push") {
    if (!activeRepo || isSyncing) return;
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
        lastFetchedMap[target.id] = Date.now();
        setOperationSuccess(`Fetched ${target.name}`);
      } else if (operation === "pull") {
        await api.syncPull(target.id);
        lastFetchedMap[target.id] = Date.now();
        setOperationSuccess(`Pulled ${target.name}`);
      } else {
        await api.syncPush(target.id, !target.hasUpstream);
        lastFetchedMap[target.id] = Date.now();
        setOperationSuccess(target.hasUpstream ? `Pushed ${target.name}` : `Published ${target.branch}`);
      }
      await refreshSingleStatus(target.id);
    } catch (e: any) {
      setOperationError(`${label} failed`, e.message || String(e));
      addToast({ message: e.message || `${label} failed`, kind: "error" });
    } finally {
      busyOp = null;
    }
  }

  function handleWindowKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      if (showMore) showMore = false;
    }
  }

  $effect(() => {
    const unregister = registerDismiss(() => {
      if (showMore) {
        showMore = false;
        return true;
      }
      return false;
    });
    return unregister;
  });

  onDestroy(() => {
    showMore = false;
  });
</script>

<svelte:window onkeydown={handleWindowKeydown} />

<header class="top-header" data-tauri-drag-region>
  <div class="header-left">
    <div class="traffic-lights-spacer" data-tauri-drag-region></div>

    <button
      class="icon-button"
      onclick={handleToggleSidebar}
      title="Toggle repositories (⌘B)"
      aria-label="Toggle repositories"
      aria-expanded={!$sidebarCollapsed}
    >
      <Icon name="sidebar" size={16} />
    </button>

    {#if activeRepo}
      <span class="operation-divider"></span>

      <div class="header-branch">
        <BranchDropdown repo={activeRepo} status={activeStatus} inline />
      </div>

      <span class="operation-divider"></span>

      <!-- Combined GitHub Desktop-style button matching original fetch button style -->
      <button
        class="operation"
        class:is-busy={isSyncing}
        disabled={!activeRepo || isSyncing || (actionType !== "fetch" && isInactiveView)}
        onclick={() => runGit(actionType)}
        title={syncTooltip}
        aria-label={syncLabel}
      >
        <span class="op-icon" class:spinning={isSyncing}>
          <Icon name={syncIconName} size={14} />
        </span>
        {#if syncCount > 0}
          <span class="op-count">{syncCount}</span>
        {/if}
      </button>
    {/if}
  </div>

  <div class="drag-spacer" data-tauri-drag-region></div>

  <div class="header-right">
    <button
      class="icon-button"
      onclick={handleToggleTerminal}
      disabled={!activeRepo}
      title={isTerminalOpen ? "Hide terminal" : "Show terminal"}
      aria-label="Toggle terminal"
      aria-pressed={isTerminalOpen}
    >
      <Icon name="terminal" size={16} />
    </button>

    <span class="operation-divider"></span>

    <button
      class="icon-button"
      onclick={() => {
        showMore = !showMore;
      }}
      title="More commands"
      aria-label="More commands"
      aria-haspopup="menu"
      aria-expanded={showMore}
    >
      <Icon name="more-horizontal" size={16} />
    </button>
    <button
      class="icon-button"
      onclick={() => ($settingsOpen = true)}
      title="Preferences (⌘,)"
      aria-label="Preferences"
    >
      <Icon name="gear" size={16} />
    </button>

    {#if showMore}
      <div class="header-menu-backdrop" role="presentation" onclick={() => (showMore = false)}></div>
      <div class="more-menu" role="menu" aria-label="More commands" tabindex="-1">
        {#if activeRepo}
          <button
            class="more-option"
            role="menuitem"
            disabled={isSyncing}
            onclick={() => {
              showMore = false;
              runGit("fetch");
            }}
          >
            <Icon name="refresh" size={14} />
            <span>Fetch from origin</span>
          </button>
          <button
            class="more-option"
            role="menuitem"
            disabled={isSyncing || isInactiveView}
            onclick={() => {
              showMore = false;
              runGit("pull");
            }}
          >
            <Icon name="arrow-down" size={14} />
            <span>Pull from origin</span>
            {#if activeStatus?.behind && activeStatus.behind > 0}
              <span class="menu-shortcut">{activeStatus.behind}</span>
            {/if}
          </button>
          <button
            class="more-option"
            role="menuitem"
            disabled={isSyncing || isInactiveView}
            onclick={() => {
              showMore = false;
              runGit("push");
            }}
          >
            <Icon name="arrow-up" size={14} />
            <span>{hasUpstream ? "Push to origin" : "Publish branch"}</span>
            {#if activeStatus?.ahead && activeStatus.ahead > 0}
              <span class="menu-shortcut">{activeStatus.ahead}</span>
            {/if}
          </button>
          <div class="menu-divider"></div>
        {/if}
        <button
          class="more-option"
          role="menuitem"
          onclick={() => {
            setPaneTab($focusedPaneIndex, "history");
            showMore = false;
          }}
        >
          Changes and history
        </button>
        <button
          class="more-option"
          role="menuitem"
          onclick={() => {
            setPaneTab($focusedPaneIndex, "files");
            showMore = false;
          }}
        >
          Browse project files
        </button>
        <button
          class="more-option"
          role="menuitem"
          onclick={() => {
            $paletteOpen = true;
            showMore = false;
          }}
        >
          Command palette <span class="menu-shortcut">⌘K</span>
        </button>
        <button
          class="more-option"
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

<style>
  .top-header {
    /* 18px center line; Tauri's native titlebar inset is 18 (not CSS top). */
    --header-height: 36px;
    height: var(--header-height);
    min-height: var(--header-height);
    flex-shrink: 0;
    padding: 0 10px;
    display: flex;
    align-items: center;
    background: var(--chrome);
    border: 0;
    position: relative;
    color: #353a40;
    z-index: 40;
    user-select: none;
  }

  .header-left {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
    flex-shrink: 0;
  }

  .traffic-lights-spacer {
    width: 68px;
    flex-shrink: 0;
  }

  .header-branch {
    display: flex;
    align-items: center;
    min-width: 0;
    max-width: 240px;
    flex-shrink: 0;
  }

  .icon-button {
    width: 26px;
    height: 26px;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    border: 0;
    background: none;
    border-radius: var(--r-sm);
    color: #3d454e;
    cursor: pointer;
    padding: 0;
  }
  .icon-button:hover:not(:disabled) {
    background: #e1e3e5;
    color: #171d24;
  }
  :global([data-theme="dark"]) .icon-button {
    color: #c9d1d9;
  }
  :global([data-theme="dark"]) .icon-button:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.1);
    color: #f0f6fc;
  }
  .icon-button:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .operation {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    min-width: 26px;
    height: 26px;
    padding: 0 6px;
    color: #3d454e;
    background: transparent;
    border: 0;
    border-radius: var(--r-sm);
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
    min-width: 16px;
    height: 16px;
    padding: 0 4px;
    font-size: 10px;
    font-weight: 600;
    line-height: 1;
    border-radius: var(--r-sm);
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
    margin: 0 4px;
    flex-shrink: 0;
  }
  :global([data-theme="dark"]) .operation-divider {
    background: #3d444d;
  }

  .drag-spacer {
    flex: 1;
    height: 100%;
    min-width: 20px;
  }

  .header-right {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
    position: relative;
  }

  .header-menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 110;
    background: transparent;
  }

  .more-menu {
    position: absolute;
    top: calc(100% + 8px);
    right: 0;
    min-width: 220px;
    padding: 5px;
    background: var(--bg);
    border-radius: var(--r-sm);
    box-shadow: 0 7px 28px rgba(0, 0, 0, 0.11), 0 0 0 1px rgba(0, 0, 0, 0.03);
    z-index: 120;
    max-height: 420px;
    overflow: auto;
  }

  .menu-divider {
    height: 1px;
    background: var(--border-subtle, #e1e4e8);
    margin: 4px 6px;
  }
  :global([data-theme="dark"]) .menu-divider {
    background: var(--border-subtle, #2f353d);
  }

  .more-option {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 9px;
    padding: 8px 10px;
    border: 0;
    background: none;
    border-radius: var(--r-sm);
    justify-content: flex-start;
    text-align: left;
    font-size: 13px;
    color: #454b53;
    white-space: nowrap;
    cursor: pointer;
  }
  :global([data-theme="dark"]) .more-option {
    color: #c9d1d9;
  }
  .more-option:hover:not(:disabled),
  .more-option:focus-visible:not(:disabled) {
    background: #e9ebed;
    outline: none;
  }
  :global([data-theme="dark"]) .more-option:hover:not(:disabled),
  :global([data-theme="dark"]) .more-option:focus-visible:not(:disabled) {
    background: rgba(255, 255, 255, 0.08);
  }
  .more-option:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .more-option :global(svg) {
    width: 15px;
    height: 15px;
    color: #90949b;
    flex-shrink: 0;
  }
  .more-option > span:not(.menu-shortcut) {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .menu-shortcut {
    margin-left: auto;
    color: var(--text-muted);
    font-size: 11px;
  }

  @media (max-width: 1100px) {
    .top-header {
      padding: 0 12px;
    }
  }
</style>
