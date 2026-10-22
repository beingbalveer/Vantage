<script lang="ts">
  import { onMount } from "svelte";
  import { loadRepos, repos, statuses, initRepoListeners } from "./lib/stores/repos";
  import { loadSettings } from "./lib/stores/settings";
  import {
    panes,
    paneCount,
    focusedPaneIndex,
    cyclePaneCount,
    closePane,
    focusPane,
    setPaneCount,
    setPaneTab,
  } from "./lib/stores/panes";
  import {
    paletteOpen,
    addRepoDialogOpen,
    settingsOpen,
    shortcutsHelpOpen,
    sidebarCollapsed,
    addToast,
  } from "./lib/stores/ui";
  import { viewportPaneWidths, savedPaneWidth } from "./lib/layout";
  import { matchShortcut } from "./lib/keys";
  import * as api from "./lib/api";
  import type { Layout, RepoEntry, RepoStatus } from "./lib/types";
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
  import RepoIconDialog from "./lib/components/Dialogs/RepoIconDialog.svelte";
  import RemoveRepoDialog from "./lib/components/Dialogs/RemoveRepoDialog.svelte";
  import ToastContainer from "./lib/components/common/ToastContainer.svelte";

  let gitVersion = $state("");
  let statusText = $state("Ready");

  let repoList = $derived($repos);
  let statusMap = $derived($statuses);

  // The workspace renders exactly the panes the stores say are visible, so the
  // focused index can never point past the last rendered pane.
  let visiblePanes = $derived($panes.slice(0, $paneCount));

  function repoForPane(repoId: string | null): RepoEntry | null {
    if (!repoId) return null;
    return repoList.find((r) => r.id === repoId) || null;
  }

  function statusForRepo(repoId: string | null): RepoStatus | null {
    if (!repoId) return null;
    return statusMap[repoId] || null;
  }

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

  // ---- Pane column widths ----
  // Panes share the workspace; each divider resizes only its two neighbours.
  // Explicit pixel widths are captured after a drag, otherwise panes split the
  // available width equally. Flex-shrink keeps the layout inside the viewport at
  // narrow widths, so panes simply get smaller instead of overflowing.
  const MIN_PANE_WIDTH = 220;

  let hostWidth = $state(0);
  let paneWidths = $state<number[]>([0, 0, 0]);

  function resolvedPaneWidths(): number[] {
    const count = Math.max(1, $paneCount);
    const total = hostWidth > 0 ? hostWidth : 1200;
    const equal = Math.round(total / count);
    const out: number[] = [];
    for (let i = 0; i < count; i++) {
      const w = paneWidths[i];
      out.push(typeof w === "number" && w > 0 ? w : equal);
    }
    return out;
  }

  function resizePanePair(index: number, nextLeft: number) {
    const widths = resolvedPaneWidths();
    const left = widths[index];
    const right = widths[index + 1];
    if (left === undefined || right === undefined) return;
    const pair = left + right;
    if (pair < MIN_PANE_WIDTH * 2) return;
    const clamped = Math.max(MIN_PANE_WIDTH, Math.min(pair - MIN_PANE_WIDTH, nextLeft));
    const updated = widths.slice();
    updated[index] = Math.round(clamped);
    updated[index + 1] = Math.round(pair - clamped);
    paneWidths = updated;
  }

  function createPaneSplitter(index: number) {
    return createHorizontalResizer({
      getWidth: () => resolvedPaneWidths()[index] ?? MIN_PANE_WIDTH,
      setWidth: (w) => resizePanePair(index, w),
      minWidth: MIN_PANE_WIDTH,
      maxWidth: 100000,
    });
  }

  function createPaneSplitterKeyboard(index: number) {
    return createHorizontalResizerKeyboard({
      getWidth: () => resolvedPaneWidths()[index] ?? MIN_PANE_WIDTH,
      setWidth: (w) => resizePanePair(index, w),
      minWidth: MIN_PANE_WIDTH,
      maxWidth: 100000,
    });
  }

  // Reused across renders (at most two dividers for three panes).
  const paneSplitters = [createPaneSplitter(0), createPaneSplitter(1)];
  const paneSplittersKeyboard = [createPaneSplitterKeyboard(0), createPaneSplitterKeyboard(1)];

  // ---- Layout persistence ----
  // Reuse the existing backend Layout (layout_get/layout_set): pane count and
  // per-pane repo/tab are persisted so the workspace survives a restart.
  let layoutReady = $state(false);
  let layoutSaveTimer: ReturnType<typeof setTimeout> | undefined;

  function persistLayout() {
    if (!layoutReady) return;
    const layout: Layout = {
      paneCount: $paneCount,
      panes: $panes.slice(0, $paneCount).map((p) => ({
        repoId: p.repoId,
        tab: p.tab,
        widths: null,
      })),
      sidebarWidth,
      sidebarCollapsed: $sidebarCollapsed,
      terminalHeight: 240,
      terminalOpen: $panes.slice(0, $paneCount).some((p) => p.terminalOpen),
      windowBounds: null,
    };
    api.layoutSet(layout).catch(() => {});
  }

  $effect(() => {
    const signature =
      `${$paneCount}|` +
      $panes
        .slice(0, $paneCount)
        .map((p) => `${p.id}:${p.repoId}:${p.tab}:${p.terminalOpen}`)
        .join(",");
    void signature;
    if (!layoutReady) return;
    clearTimeout(layoutSaveTimer);
    layoutSaveTimer = setTimeout(persistLayout, 500);
  });

  onMount(async () => {
    await loadSettings();
    await loadRepos();
    initRepoListeners();

    // Restore the last session's panes (count + repo/tab) when one was saved.
    try {
      const layout = await api.layoutGet();
      const count = Math.max(1, Math.min(3, layout.paneCount || 1));
      const savedPanes = Array.isArray(layout.panes) ? layout.panes : [];
      const knownRepos = $repos;
      if (count > 1 || savedPanes.some((p) => p?.repoId)) {
        setPaneCount(count);
        panes.update((list) =>
          list.map((p, i) => {
            const saved = savedPanes[i];
            if (i >= count || !saved) return { ...p };
            const repoId =
              saved.repoId && knownRepos.some((r) => r.id === saved.repoId)
                ? saved.repoId
                : p.repoId;
            return { ...p, repoId, tab: saved.tab === "files" ? "files" : "history" };
          })
        );
      }
    } catch (e) {
      console.warn("Failed to restore pane layout:", e);
    }

    // Fallback: give the first pane a repo when nothing was restored.
    const currentRepos = $repos;
    if (currentRepos.length > 0) {
      panes.update((list) => {
        if (!list[0].repoId) {
          return list.map((p, i) => (i === 0 ? { ...p, repoId: currentRepos[0].id } : { ...p }));
        }
        return list;
      });
    }

    layoutReady = true;

    try {
      const env = await api.envCheck();
      gitVersion = env.gitVersion;
      if (env.startupWarning) {
        addToast({
          message: env.startupWarning,
          kind: "error",
          details: env.startupWarning,
        });
      }
    } catch (e) {
      console.error("Failed to check env:", e);
    }

    // Check for updates silently on startup
    checkForAppUpdates(false).catch(() => {});
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

    // Panes: cycle 1→2→3→1, focus 1/2/3, close the focused pane.
    if (matchShortcut(e, "Mod+\\")) {
      e.preventDefault();
      cyclePaneCount();
      return;
    }

    if (matchShortcut(e, "Mod+1")) {
      e.preventDefault();
      focusPane(0);
      return;
    }

    if (matchShortcut(e, "Mod+2")) {
      e.preventDefault();
      focusPane(1);
      return;
    }

    if (matchShortcut(e, "Mod+3")) {
      e.preventDefault();
      focusPane(2);
      return;
    }

    if (matchShortcut(e, "Mod+W")) {
      e.preventDefault();
      if ($paneCount > 1) {
        closePane($focusedPaneIndex);
      } else {
        addToast({ message: "Cannot close the only open pane", kind: "info" });
      }
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

  <!-- Workspace: Resizable Sidebar + 1-3 side-by-side panes -->
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

    <main class="panes-host" bind:clientWidth={hostWidth}>
      {#each visiblePanes as pane, i (pane.id)}
        {#if i > 0}
          <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
          <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
          <div
            class="pane-split-handle"
            role="separator"
            aria-orientation="vertical"
            aria-label="Resize pane {i} and pane {i + 1}"
            tabindex="0"
            title="Drag to resize panes (Arrow keys to nudge)"
            onpointerdown={paneSplitters[i - 1]}
            onkeydown={paneSplittersKeyboard[i - 1]}
          ></div>
        {/if}
        <div class="pane-slot" style="flex: 1 1 {resolvedPaneWidths()[i]}px;">
          <Pane
            paneIndex={i}
            repo={repoForPane(pane.repoId)}
            status={statusForRepo(pane.repoId)}
          />
        </div>
      {/each}
    </main>
  </div>

  <!-- Overlays -->
  <CommandPalette />
  <AddRepoDialog />
  <SettingsDialog />
  <ShortcutsHelpDialog />
  <RevertDialog />
  <ResetDialog />
  <DiscardDialog />
  <RepoIconDialog />
  <RemoveRepoDialog />
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

  .pane-slot {
    display: flex;
    min-width: 0;
    height: 100%;
    overflow: hidden;
  }

  /* Vertical divider between adjacent panes. */
  .pane-split-handle {
    flex: 0 0 4px;
    width: 4px;
    height: 100%;
    position: relative;
    cursor: col-resize;
    z-index: 6;
    flex-shrink: 0;
    user-select: none;
    background-color: var(--border-subtle);
    transition: background-color 100ms ease;
  }
  .pane-split-handle:hover,
  .pane-split-handle:active,
  .pane-split-handle:focus-visible {
    background-color: var(--accent);
    outline: none;
  }
</style>
