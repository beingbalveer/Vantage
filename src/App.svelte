<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { loadRepos, repos, statuses, initRepoListeners } from "./lib/stores/repos";
  import { loadSettings } from "./lib/stores/settings";
  import { panes, focusedPaneIndex, setPaneTab } from "./lib/stores/panes";
  import { dispatchMenuCommand } from "./lib/menu";
  import {
    paletteOpen,
    addRepoDialogOpen,
    settingsOpen,
    shortcutsHelpOpen,
    sidebarCollapsed,
    addToast,
  } from "./lib/stores/ui";
  import { operationStatus } from "./lib/stores/operation";
  import { viewportPaneWidths, savedPaneWidth } from "./lib/layout";
  import { matchShortcut } from "./lib/keys";
  import * as api from "./lib/api";
  import { checkForAppUpdates } from "./lib/updater";
  import { createHorizontalResizer, createHorizontalResizerKeyboard } from "./lib/resizer";
  import Sidebar from "./lib/components/Sidebar/Sidebar.svelte";
  import Pane from "./lib/components/Pane/Pane.svelte";
  import TopHeader from "./lib/components/Header/TopHeader.svelte";
  import CommandPalette from "./lib/components/Palette/CommandPalette.svelte";
  import AddRepoDialog from "./lib/components/Dialogs/AddRepoDialog.svelte";
  import SettingsDialog from "./lib/components/Dialogs/SettingsDialog.svelte";
  import ShortcutsHelpDialog from "./lib/components/Dialogs/ShortcutsHelpDialog.svelte";
  import RevertDialog from "./lib/components/Dialogs/RevertDialog.svelte";
  import ResetDialog from "./lib/components/Dialogs/ResetDialog.svelte";
  import DiscardDialog from "./lib/components/Dialogs/DiscardDialog.svelte";
  import StashDialog from "./lib/components/Dialogs/StashDialog.svelte";
  import CheckoutDialog from "./lib/components/Dialogs/CheckoutDialog.svelte";
  import RepoIconDialog from "./lib/components/Dialogs/RepoIconDialog.svelte";
  import RemoveRepoDialog from "./lib/components/Dialogs/RemoveRepoDialog.svelte";
  import CreateBranchDialog from "./lib/components/Dialogs/CreateBranchDialog.svelte";
  import RenameBranchDialog from "./lib/components/Dialogs/RenameBranchDialog.svelte";
  import DeleteBranchDialog from "./lib/components/Dialogs/DeleteBranchDialog.svelte";
  import ToastContainer from "./lib/components/common/ToastContainer.svelte";

  let gitVersion = $state("");
  let statusText = $state("Ready");
  let unlistenMenu: (() => void) | undefined;

  // Persistent, non-transient surfacing of a missing/failed Git environment
  // (ALPH-150): the status bar shows the failure with a details affordance and
  // a retry, instead of a toast that disappears.
  let envError = $state<string | null>(null);
  let showEnvDetails = $state(false);

  let operation = $derived($operationStatus);
  let statusLabel = $derived(operation.status === "idle" ? statusText : operation.message);

  let repoList = $derived($repos);
  let statusMap = $derived($statuses);
  let currentPane = $derived($panes[0]);
  let activeRepo = $derived(repoList.find((r) => r.id === currentPane?.repoId) || repoList[0] || null);
  let activeStatus = $derived(activeRepo ? statusMap[activeRepo.id] || null : null);

  // Slidable Sidebar Width
  let sidebarWidth = $state(savedPaneWidth("vantage_sidebar_width", viewportPaneWidths().sidebar));

  const startSidebarResize = createHorizontalResizer({
    getWidth: () => sidebarWidth,
    setWidth: (w) => (sidebarWidth = w),
    minWidth: 160,
    maxWidth: 450,
    storageKey: "vantage_sidebar_width",
  });

  const nudgeSidebarResize = createHorizontalResizerKeyboard({
    getWidth: () => sidebarWidth,
    setWidth: (w) => (sidebarWidth = w),
    minWidth: 160,
    maxWidth: 450,
    storageKey: "vantage_sidebar_width",
  });

  async function checkEnv() {
    try {
      const env = await api.envCheck();
      gitVersion = env.gitVersion;
      envError = null;
      if (env.startupWarning) {
        addToast({
          message: env.startupWarning,
          kind: "error",
          details: env.startupWarning,
        });
      }
    } catch (e: any) {
      // Not silent: the failure is kept in the status bar and stays there.
      console.error("Failed to check env:", e);
      envError = e?.message || String(e);
      gitVersion = "";
    }
  }

  onMount(async () => {
    // Native menu clicks arrive as ids and reuse the palette/shortcut stores.
    unlistenMenu = await api.onMenuCommand((command) => {
      void dispatchMenuCommand(command);
    });

    await loadSettings();
    await loadRepos();
    initRepoListeners();

    const currentRepos = $repos;
    if (currentRepos.length > 0) {
      panes.update((list) => {
        if (!list[0].repoId) {
          return list.map((p, i) => (i === 0 ? { ...p, repoId: currentRepos[0].id } : { ...p }));
        }
        return list;
      });
    }

    await checkEnv();

    // Check for updates silently on startup
    checkForAppUpdates(false).catch(() => {});
  });

  onDestroy(() => {
    unlistenMenu?.();
  });

  function handleGlobalKeydown(e: KeyboardEvent) {
    const isInput =
      e.target instanceof HTMLInputElement ||
      e.target instanceof HTMLTextAreaElement ||
      (e.target as HTMLElement)?.isContentEditable;

    if (matchShortcut(e, "Mod+K") || matchShortcut(e, "Ctrl+Shift+P")) {
      e.preventDefault();
      $paletteOpen = !$paletteOpen;
      return;
    }

    if (matchShortcut(e, "Mod+P")) {
      e.preventDefault();
      setPaneTab($focusedPaneIndex, "files");
      return;
    }

    if (matchShortcut(e, "Mod+O")) {
      e.preventDefault();
      $addRepoDialogOpen = true;
      return;
    }

    if (matchShortcut(e, "Mod+B")) {
      e.preventDefault();
      $sidebarCollapsed = !$sidebarCollapsed;
      return;
    }

    if (matchShortcut(e, "Mod+,")) {
      e.preventDefault();
      $settingsOpen = true;
      return;
    }

    if (!isInput && e.key === "?") {
      e.preventDefault();
      $shortcutsHelpOpen = true;
    }
  }
</script>

<svelte:window onkeydown={handleGlobalKeydown} />

<div class="app-layout">
  <!-- Single window header; height is owned by TopHeader. -->
  <TopHeader />

  <!-- Workspace: Resizable Sidebar + Single Full-width Main Pane -->
  <div class="workspace">
    <Sidebar width={sidebarWidth} />

    {#if !$sidebarCollapsed}
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <div
        class="split-handle"
        role="separator"
        aria-orientation="vertical"
        aria-label="Resize repositories pane"
        tabindex="0"
        title="Drag to resize sidebar (Double-click to reset, Arrow keys to nudge)"
        onpointerdown={startSidebarResize}
        onkeydown={nudgeSidebarResize}
        ondblclick={() => {
          sidebarWidth = viewportPaneWidths().sidebar;
          try { localStorage.setItem("vantage_sidebar_width", String(sidebarWidth)); } catch {}
        }}
      ></div>
    {/if}

    <main class="panes-host">
      <Pane paneIndex={0} repo={activeRepo} status={activeStatus} />
    </main>
  </div>

  <footer class="status-bar" role="status" aria-live="polite">
    <span class="status-label truncate">{statusLabel}</span>
    <span class="status-spacer"></span>
    {#if envError}
      <button
        type="button"
        class="status-error"
        aria-expanded={showEnvDetails}
        onclick={() => (showEnvDetails = !showEnvDetails)}
        title={envError}
      >
        <span class="status-dot"></span>
        Git unavailable
      </button>
    {:else if gitVersion}
      <span class="status-git">{gitVersion}</span>
    {/if}

    {#if envError && showEnvDetails}
      <div class="status-details">
        <span class="status-details-msg font-mono">{envError}</span>
        <button type="button" class="status-retry" onclick={checkEnv}>Retry</button>
      </div>
    {/if}
  </footer>

  <!-- Overlays -->
  <CommandPalette />
  <AddRepoDialog />
  <SettingsDialog />
  <ShortcutsHelpDialog />
  <RevertDialog />
  <ResetDialog />
  <DiscardDialog />
  <StashDialog />
  <CheckoutDialog />
  <RepoIconDialog />
  <RemoveRepoDialog />
  <CreateBranchDialog />
  <RenameBranchDialog />
  <DeleteBranchDialog />
  <ToastContainer />
</div>

<style>
  .app-layout {
    width: 100vw;
    height: 100vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    background-color: var(--chrome);
  }

  .workspace {
    flex: 1;
    display: flex;
    overflow: hidden;
  }

  .split-handle {
    width: 8px;
    margin: 0 -4px;
    height: 100%;
    position: relative;
    cursor: col-resize;
    z-index: 50;
    flex-shrink: 0;
    user-select: none;
    background: transparent;
  }
  .split-handle::after {
    content: "";
    position: absolute;
    top: 0;
    bottom: 0;
    left: 3.5px;
    width: 1px;
    background-color: var(--border-subtle);
    transition: background-color 100ms ease;
  }
  .split-handle:hover::after,
  .split-handle:active::after {
    background-color: var(--accent);
  }

  .panes-host {
    flex: 1;
    display: flex;
    overflow: hidden;
    min-width: 0;
  }

  .status-bar {
    flex-shrink: 0;
    height: 24px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px;
    background-color: var(--bg-subtle);
    border-top: 1px solid var(--border-subtle);
    color: var(--text-muted);
    font-size: 11px;
    line-height: 16px;
    position: relative;
    user-select: none;
  }
  .status-label {
    min-width: 0;
    max-width: 60%;
  }
  .status-spacer {
    flex: 1;
  }
  .status-git {
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .status-error {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: none;
    border: none;
    padding: 0;
    color: var(--danger);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    white-space: nowrap;
  }
  .status-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background-color: var(--danger);
    flex-shrink: 0;
  }

  .status-details {
    position: absolute;
    right: 8px;
    bottom: 28px;
    z-index: 200;
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-width: 440px;
    padding: 10px 12px;
    background-color: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    box-shadow: var(--shadow-pop);
  }
  .status-details-msg {
    color: var(--text);
    font-size: 11px;
    line-height: 16px;
    white-space: pre-wrap;
    word-break: break-word;
    user-select: text;
  }
  .status-retry {
    align-self: flex-end;
    height: 22px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background-color: var(--bg);
    color: var(--text);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
  }
  .status-retry:hover {
    background-color: var(--bg-muted);
  }
</style>
