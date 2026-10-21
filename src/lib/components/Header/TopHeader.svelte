<script lang="ts">
  import { get } from "svelte/store";
  import { onDestroy } from "svelte";
  import { repos, statuses, refreshSingleStatus } from "../../stores/repos";
  import {
    panes,
    focusedPaneIndex,
    toggleTerminalInPane,
    setPaneTab,
  } from "../../stores/panes";
  import { paletteOpen, settingsOpen, shortcutsHelpOpen, sidebarCollapsed, addToast } from "../../stores/ui";
  import { setOperationRunning, setOperationSuccess, setOperationError } from "../../stores/operation";
  import * as api from "../../api";
  import Icon from "../common/Icon.svelte";

  let currentPane = $derived($panes[$focusedPaneIndex]);
  let currentTab = $derived(currentPane?.tab || "history");
  let isTerminalOpen = $derived(currentPane?.terminalOpen || false);

  let activeRepo = $derived($repos.find((r) => r.id === currentPane?.repoId) || $repos[0] || null);
  let activeStatus = $derived(activeRepo ? $statuses[activeRepo.id] || null : null);
  let currentBranch = $derived(activeRepo?.branch || activeStatus?.branch || "HEAD");
  let isInactiveView = $derived(activeStatus?.isCheckedOut === false);

  let showMore = $state(false);

  // Busy state per remote operation so each button reports its own progress.
  let busyOp = $state<null | "fetch" | "pull" | "push">(null);
  let isSyncing = $derived(busyOp !== null);

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

  onDestroy(() => {
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
    </div>
  {/if}

  <div class="operations">
    {#each ["Fetch", "Pull", "Push"] as operation}
      {@const key = operation.toLowerCase() as "fetch" | "pull" | "push"}
      {@const isBusy = busyOp === key}
      <button
        class="operation"
        class:is-busy={isBusy}
        disabled={!activeRepo || isSyncing || (key !== "fetch" && isInactiveView)}
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

  .more-menu {
    position: absolute;
    top: calc(100% + 8px);
    right: 0;
    min-width: 220px;
    padding: 5px;
    background: var(--bg);
    border-radius: 10px;
    box-shadow: 0 7px 28px rgba(0, 0, 0, 0.11), 0 0 0 1px rgba(0, 0, 0, 0.03);
    z-index: 120;
    max-height: 420px;
    overflow: auto;
  }

  .more-option {
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
  .more-option:hover,
  .more-option:focus-visible {
    background: #e9ebed;
    outline: none;
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
    color: #8b9096;
    font-size: 11px;
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
    .top-header {
      gap: 8px;
      padding-right: 12px;
    }
    .header-context {
      gap: 8px;
    }
  }
</style>
