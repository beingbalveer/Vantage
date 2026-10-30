<script lang="ts">
  import { get } from "svelte/store";
  import { onDestroy, onMount } from "svelte";
  import { repos, statuses, loadRepos, refreshSingleStatus } from "../../domain/stores/repos";
  import {
    panes,
    paneCount,
    focusedPaneIndex,
    toggleTerminalInPane,
    setPaneTab,
  } from "../../domain/stores/panes";
  import { paneLeftColumnWidths, defaultLeftColumnWidth } from "../../app/layout";
  import BranchMenu from "./BranchMenu.svelte";
  import { paletteOpen, settingsOpen, shortcutsHelpOpen, addToast } from "../../app/ui";
  import { setOperationRunning, setOperationSuccess, setOperationError } from "../../domain/stores/operation";
  import { formatRelativeTimeLong } from "../../domain/format";
  import { registerDismiss } from "../../app/commands";
  import * as api from "../../platform/api";
  import Icon from "../common/Icon.svelte";
  import { Modal } from "../common";

  interface Props {
    hostWidth?: number;
  }

  let { hostWidth = 0 }: Props = $props();

  // Dev runs (`npm run dev:app`, `npm run dev:bundle`) mark themselves so they
  // are never mistaken for the release app.
  const isDevBuild =
    import.meta.env.DEV || import.meta.env.VITE_DEV_BUILD === "1";

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

  let clockNow = $state(Date.now());

  $effect(() => {
    const timer = setInterval(() => {
      clockNow = Date.now();
    }, 30000);
    return () => clearInterval(timer);
  });

  let hasUpstream = $derived(!!activeStatus?.upstream);

  // The header sits above the workspace, so place the Fetch control exactly on
  // the leading column's right edge (sidebar + focused pane + pane left col).
  const SIDEBAR_COLLAPSED_WIDTH = 60;
  const PANE_HANDLE_WIDTH = 4;
  let sidebarSlot = SIDEBAR_COLLAPSED_WIDTH;
  let singlePaneWidth = $derived.by(() => {
    const count = $paneCount;
    return count > 1 ? Math.max(0, (hostWidth - (count - 1) * PANE_HANDLE_WIDTH) / count) : 0;
  });
  let focusedIndex = $derived(Math.min($focusedPaneIndex, $paneCount - 1));
  let paneOffset = $derived(focusedIndex * (singlePaneWidth + PANE_HANDLE_WIDTH));
  let focusedLeftCol = $derived(
    $paneLeftColumnWidths[focusedIndex] ?? defaultLeftColumnWidth()
  );
  const HEADER_SAFE_LEFT = 84;
  let paneStart = $derived(sidebarSlot + paneOffset);
  let branchLeft = $derived(Math.max(paneStart, HEADER_SAFE_LEFT));
  let branchWidth = $derived(Math.max(0, focusedLeftCol - (branchLeft - paneStart)));
  let fetchLeft = $derived(paneStart + focusedLeftCol);

  /** Relative time of the last fetch/pull, read from FETCH_HEAD. */
  let lastFetchedLabel = $derived.by(() => {
    void clockNow;
    const at = activeStatus?.lastFetchedAt;
    return at ? formatRelativeTimeLong(at) : "not fetched yet";
  });

  let syncTooltip = $derived.by(() => {
    void clockNow;
    if (busyOp === "fetch") return "Fetching from origin…";
    if (busyOp) return "Another remote operation is running…";
    const last = activeStatus?.lastFetchedAt;
    return last
      ? `Fetch · Last fetched ${formatRelativeTimeLong(last)}`
      : "Fetch";
  });

  function handleToggleTerminal() {
    toggleTerminalInPane($focusedPaneIndex);
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
        setOperationSuccess(`Fetched ${target.name}`);
      } else if (operation === "pull") {
        await api.syncPull(target.id);
        setOperationSuccess(`Pulled ${target.name}`);
      } else {
        await api.syncPush(target.id, !target.hasUpstream);
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

    {#if activeRepo}
      <BranchMenu
        repo={activeRepo}
        status={activeStatus}
        left={branchLeft}
        width={branchWidth}
      />

      <!-- Fetch: icon + action and the last-fetch time from FETCH_HEAD -->
      <button
        class="operation fetch-button"
        style="left: {fetchLeft}px"
        class:is-busy={isSyncing}
        disabled={!activeRepo || isSyncing}
        onclick={() => runGit("fetch")}
        title={syncTooltip}
        aria-label={activeStatus?.lastFetchedAt
          ? `Fetch, last fetched ${lastFetchedLabel}`
          : "Fetch"}
      >
        <span class="op-icon fetch-icon" class:spinning={busyOp === "fetch"}>
          <Icon name="refresh" size={18} />
        </span>
        <span class="fetch-text">
          <span class="fetch-title text-strong">
            {busyOp === "fetch" ? "Fetching…" : "Fetch"}
          </span>
          <span class="fetch-sub text-xs text-muted">{lastFetchedLabel}</span>
        </span>
      </button>
    {/if}
  </div>

  <div class="drag-spacer" data-tauri-drag-region></div>

  <div class="header-right">
    {#if isDevBuild}
      <span class="dev-badge text-xs" title="Development build" aria-label="Development build">DEV</span>
    {/if}

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
    --header-height: 43px;
    height: var(--header-height);
    min-height: var(--header-height);
    flex-shrink: 0;
    padding: 0 10px;
    display: flex;
    align-items: center;
    background: var(--chrome);
    /* Hairline fence so the chrome reads separately from the workspace. */
    border: 0;
    border-bottom: 1px solid var(--border);
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

  .dev-badge {
    display: inline-flex;
    align-items: center;
    height: 16px;
    padding: 0 5px;
    background: var(--warning-bg);
    color: var(--warning);
    font-weight: 700;
    letter-spacing: 0.06em;
    border-radius: var(--r-sm);
    flex-shrink: 0;
    user-select: none;
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

  /* Two-line fetch control: icon left, action and last-fetch time stacked. */
  .fetch-button {
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    flex-direction: row;
    align-items: center;
    justify-content: flex-start;
    gap: 10px;
    min-width: 160px;
    height: 34px;
    padding: 0 10px;
    background: var(--bg-muted);
    border-left: 1px solid var(--border);
    border-right: 1px solid var(--border);
  }
  .fetch-icon {
    flex-shrink: 0;
  }
  .fetch-text {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0;
    min-width: 0;
  }
  .fetch-title {
    line-height: 1.15;
  }
  .fetch-sub {
    line-height: 1.15;
    font-weight: 400;
    white-space: nowrap;
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
