<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { loadRepos, repos, statuses, initRepoListeners } from "./lib/domain/stores/repos";
  import { loadSettings } from "./lib/domain/stores/settings";
  import {
    panes,
    paneCount,
    focusedPaneIndex,
    cyclePaneCount,
    closePane,
    focusPane,
    setPaneCount,
    setPaneTab,
  } from "./lib/domain/stores/panes";
  import { dispatchMenuCommand } from "./lib/app/menu";
  import {
    paletteOpen,
    addRepoDialogOpen,
    settingsOpen,
    shortcutsHelpOpen,
    addToast,
  } from "./lib/app/ui";
  import { operationStatus } from "./lib/domain/stores/operation";
  import { isFeatureEnabled } from "./lib/config/featureFlags.svelte";
  import { matchShortcut } from "./lib/platform/keys";
  import { dispatchKeyboard } from "./lib/app/commands";
  import * as api from "./lib/platform/api";
  import type { Layout, RepoEntry, RepoStatus } from "./lib/platform/types";
  import { checkForAppUpdates } from "./lib/platform/updater";
  import { createHorizontalResizer, createHorizontalResizerKeyboard } from "./lib/app/resizer";
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
  import LayoutDialog from "./lib/components/Dialogs/LayoutDialog.svelte";

  import TagCreateDialog from "./lib/components/Dialogs/TagCreateDialog.svelte";
  import TagDetailsDialog from "./lib/components/Dialogs/TagDetailsDialog.svelte";

  import ToastContainer from "./lib/components/common/ToastContainer.svelte";
  import Icon from "./lib/components/common/Icon.svelte";

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

  // The workspace renders exactly the panes the stores say are visible, so the
  // focused index can never point past the last rendered pane.
  let visiblePanes = $derived(
    isFeatureEnabled("ALPH_144_MULTI_PANE")
      ? $panes.slice(0, $paneCount)
      : $panes.slice(0, 1)
  );

  function repoForPane(repoId: string | null): RepoEntry | null {
    if (!repoId) return null;
    return repoList.find((r) => r.id === repoId) || null;
  }

  function statusForRepo(repoId: string | null): RepoStatus | null {
    if (!repoId) return null;
    return statusMap[repoId] || null;
  }

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

  // ---- Pane column widths ----
  // Panes share the workspace; each divider resizes only its two neighbours.
  // Explicit pixel widths are captured after a drag, otherwise panes split the
  // available width equally. Flex-shrink keeps the layout inside the viewport at
  // narrow widths, so panes simply get smaller instead of overflowing.
  const MIN_PANE_WIDTH = 220;

  let hostWidth = $state(0);
  let paneWidths = $state<number[]>([0, 0, 0]);

  function resolvedPaneWidths(): number[] {
    const count = Math.max(1, isFeatureEnabled("ALPH_144_MULTI_PANE") ? $paneCount : 1);
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
    const count = isFeatureEnabled("ALPH_144_MULTI_PANE") ? $paneCount : 1;
    const layout: Layout = {
      paneCount: count,
      panes: $panes.slice(0, count).map((p, i) => ({
        repoId: p.repoId,
        tab: p.tab,
        // Persist the resolved split width for this pane so the 1–3 column
        // layout survives a restart instead of always re-splitting equally.
        widths: paneWidths[i] ? [paneWidths[i]] : null,
      })),
      terminalHeight: $panes[0]?.terminalHeight ?? 240,
      terminalOpen: $panes.slice(0, count).some((p) => p.terminalOpen),
      windowBounds: null,
    };
    api.layoutSet(layout).catch(() => {});
  }

  $effect(() => {
    // Every field written by persistLayout must be read here: the effect only
    // re-runs for values it actually tracks. Keeping the signature explicit
    // (rather than calling persistLayout directly) also debounces drags.
    const signature =
      `${$paneCount}|${paneWidths.join(",")}|` +
      $panes
        .slice(0, $paneCount)
        .map((p) => `${p.id}:${p.repoId}:${p.tab}:${p.terminalOpen}:${p.terminalHeight}`)
        .join(",");
    void signature;
    if (!layoutReady) return;
    clearTimeout(layoutSaveTimer);
    layoutSaveTimer = setTimeout(persistLayout, 500);
  });
  onMount(async () => {
    // Native menu clicks arrive as ids and reuse the palette/shortcut stores.
    unlistenMenu = await api.onMenuCommand((command) => {
      void dispatchMenuCommand(command);
    });

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
        // Widths are stored as a single-element array per pane (see persistLayout).
        const restoredWidths = savedPanes
          .slice(0, count)
          .map((p) => (Array.isArray(p?.widths) && p.widths[0] > 0 ? p.widths[0] : 0));
        if (restoredWidths.some((w) => w > 0)) {
          paneWidths = restoredWidths;
        }
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
    await checkEnv();

    // Check for updates silently on startup
    checkForAppUpdates(false).catch(() => {});
  });

  onDestroy(() => {
    unlistenMenu?.();
  });

  function handleGlobalKeydown(e: KeyboardEvent) {
    if (isFeatureEnabled("ALPH_161_SHORTCUT_REGISTRY")) {
      dispatchKeyboard(e);
      return;
    }

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
  <TopHeader {hostWidth} />

  <!-- Workspace: Resizable Sidebar + 1-3 side-by-side panes -->
  <div class="workspace">
    <Sidebar />

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
        <button
          type="button"
          class="status-retry"
          onclick={checkEnv}
          title="Retry Git check"
          aria-label="Retry Git check"
        >
          <Icon name="refresh" size={12} />
        </button>
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
  <LayoutDialog />

  <TagCreateDialog />
  <TagDetailsDialog />

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
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background-color: var(--bg);
    color: var(--text);
    cursor: pointer;
  }
  .status-retry:hover {
    background-color: var(--bg-muted);
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
