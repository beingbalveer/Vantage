<script lang="ts">
  import { untrack } from "svelte";
  import { leftColumnWidth, setLeftColumnWidth, viewportPaneWidths } from "../../app/layout";
  import { setPaneTab } from "../../domain/stores/panes";
  import * as api from "../../platform/api";
  import { normalizeError, createDelayedToggle, type NormalizedError } from "../../domain/asyncState";
  import type { RepoEntry, RepoStatus } from "../../platform/types";
  import CodeViewer from "../Viewer/CodeViewer.svelte";
  import FileTree from "./FileTree.svelte";
  import { createHorizontalResizer, createHorizontalResizerKeyboard } from "../../app/resizer";
  import { registerHandler, ownsPaneKeyboard } from "../../app/commands";
  import Icon from "../common/Icon.svelte";
  import Skeleton from "../common/Skeleton.svelte";
  import Banner from "../common/Banner.svelte";
  import { navigationTarget } from "../../domain/stores/navigation";
  import { addToast } from "../../app/ui";
  import {
    closeFileTab,
    fileTabs,
    getFileTabState,
    openFileTab,
    pruneFileTabs,
  } from "../../domain/stores/fileTabs";
  import FileTabs from "./FileTabs.svelte";
  import { matchShortcut } from "../../platform/keys";

  interface Props {
    repoId: string;
    rev?: string;
    targetFile?: string | null;
    targetLine?: number | null;
    repo?: RepoEntry | null;
    status?: RepoStatus | null;
    paneIndex?: number;
  }

  let {
    repoId,
    rev = "HEAD",
    targetFile = null,
    targetLine: initialTargetLine = null,
    repo = null,
    status = null,
    paneIndex = 0,
  }: Props = $props();

  let fileList = $state<string[]>([]);
  // Open files live in a store keyed by repository, so tabs survive a switch
  // to the Commits view and back.
  let tabState = $derived(getFileTabState($fileTabs, repoId));
  let openTabs = $derived(tabState.tabs);
  let activeFile = $derived(tabState.active);
  let activeTargetLine = $state<number | null>(null);
  let fileTreeRef = $state<any>(null);

  // Tree load state (ALPH-150): a failed tree request must be visible and
  // retryable rather than silently swallowed.
  let treeLoading = $state(false);
  let treeError = $state<NormalizedError | null>(null);
  let treeGen = 0;
  let treeRequests = $state(0);
  let treeInFlight = $derived(treeRequests > 0);
  const treePending = createDelayedToggle(150, (v) => (treeLoading = v));

  async function loadTree(revision: string = rev || "HEAD") {
    if (!repoId) return;
    const gen = ++treeGen;
    treeRequests += 1;
    treePending.begin();
    treeError = null;
    try {
      const files = await api.treeList(repoId, revision);
      if (gen !== treeGen) return;
      fileList = files;
      pruneFileTabs(repoId, files);
      // First visit to a repo opens its first file, like the old single-view
      // behaviour; later visits restore whatever tabs were left open.
      if (files.length > 0 && openTabs.length === 0) {
        openFileTab(repoId, files[0]);
      }
    } catch (e) {
      if (gen !== treeGen) return;
      console.error("Failed to load file tree:", e);
      treeError = normalizeError(e);
    } finally {
      treeRequests = Math.max(0, treeRequests - 1);
      treePending.end();
    }
  }

  $effect(() => {
    if (!repoId) return;
    // Depend on the revision too, so switching branches reloads the tree.
    const revision = rev || "HEAD";
    // `loadTree` bumps its own in-flight counter, which is reactive. Running it
    // inside `untrack` keeps that bookkeeping out of this effect's dependency
    // set — otherwise every completed request retriggers the effect and the
    // tree reloads forever.
    untrack(() => {
      fileList = [];
      treeError = null;
      loadTree(revision);
    });
    return () => {
      // Only invalidate the in-flight request; the pending toggle is left to
      // its own begin/end balance so overlapping loads stay tracked.
      treeGen++;
    };
  });

  $effect(() => {
    if (targetFile) openFileTab(repoId, targetFile);
    if (initialTargetLine) activeTargetLine = initialTargetLine;
  });

  // Shared with the commits column: the leading edge must not move when the
  // pane switches between Commits and Files.
  const startTreeResize = createHorizontalResizer({
    getWidth: () => $leftColumnWidth,
    setWidth: (w) => setLeftColumnWidth(w),
    minWidth: 180,
    maxWidth: 700,
  });

  const nudgeTreeResize = createHorizontalResizerKeyboard({
    getWidth: () => $leftColumnWidth,
    setWidth: (w) => setLeftColumnWidth(w),
    minWidth: 160,
    maxWidth: 600,
  });

  $effect(() => {
    if ($navigationTarget) {
      if (!$navigationTarget.repoId || $navigationTarget.repoId === repoId) {
        if ($navigationTarget.path) {
          openFileTab(repoId, $navigationTarget.path);
        }
        if ($navigationTarget.line) {
          activeTargetLine = $navigationTarget.line;
        }
      }
    }
  });

  async function openInEditor() {
    const target = activeFile;
    if (!target) return;
    try {
      await api.openExternalEditor(repoId, target, activeTargetLine || undefined);
      addToast({ message: `Opened '${target}' in external editor`, kind: "info" });
    } catch (e: any) {
      console.error("Failed to open external editor:", e);
      addToast({
        message: e?.message || "Failed to open external editor. Ensure 'code' CLI is installed in PATH or configure in Settings.",
        kind: "error",
      });
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    const isInput =
      e.target instanceof HTMLInputElement ||
      e.target instanceof HTMLTextAreaElement ||
      (e.target as HTMLElement)?.isContentEditable;

    if (!isInput && e.key === "/") {
      e.preventDefault();
      fileTreeRef?.focusTree();
    } else if (matchShortcut(e, "Mod+P")) {
      e.preventDefault();
      fileTreeRef?.focusTree();
    }
  }

  function selectFileAt(index: number) {
    const file = fileList[index];
    if (!file) return;
    activeTargetLine = null;
    openFileTab(repoId, file);
  }

  function moveFile(delta: 1 | -1): boolean {
    if (!ownsPaneKeyboard(paneIndex) || fileList.length === 0) return false;
    const current = activeFile ? fileList.indexOf(activeFile) : -1;
    const next = Math.max(0, Math.min(fileList.length - 1, (current < 0 ? 0 : current) + delta));
    selectFileAt(next);
    return true;
  }

  $effect(() => {
    const unregister = [
      registerHandler(
        "focus_files",
        () => {
          if (!ownsPaneKeyboard(paneIndex)) return false;
          fileTreeRef?.focusTree?.();
          return true;
        },
        "files"
      ),
      registerHandler("list_down", () => moveFile(1), "files"),
      registerHandler("list_up", () => moveFile(-1), "files"),
    ];
    return () => unregister.forEach((off) => off());
  });
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="files-tab-container">
  <!-- Left Tree Pane -->
  <div class="tree-pane" style="width: {$leftColumnWidth}px;">
    <!-- View tabs fill the tree column; branch lives in the app header -->
    <div class="tree-header">
      <div class="pane-tabs" role="tablist" aria-label="Pane view">
        <button
          type="button"
          role="tab"
          aria-selected="false"
          class="pane-tab"
          title="Switch to Commits & History"
          onclick={() => setPaneTab(paneIndex, "history")}
        >
          <Icon name="commit" size={14} />
          <span>Commits</span>
        </button>
        <button
          type="button"
          role="tab"
          aria-selected="true"
          class="pane-tab active"
          title="Project Files Explorer"
        >
          <Icon name="folder" size={14} />
          <span>Files</span>
        </button>
      </div>
    </div>
    <!-- Tree Body -->
    <div class="tree-body">
      {#if treeLoading && fileList.length === 0}
        <div style="padding: 12px;">
          <Skeleton rows={8} height={26} shape="file" label="Loading files" />
        </div>
      {:else if treeError}
        <div style="padding: 12px;">
          <Banner kind="error" message={treeError.message} error={treeError} retryLabel="Retry" onretry={loadTree} />
        </div>
      {:else}
        <FileTree
          bind:this={fileTreeRef}
          files={fileList}
          selectedFile={activeFile}
          {repoId}
          onselectfile={(path) => {
            activeTargetLine = null;
            openFileTab(repoId, path);
          }}
        />
      {/if}
    </div>
  </div>

  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="split-handle"
    role="separator"
    aria-orientation="vertical"
    aria-label="Resize file tree"
    aria-valuemin={160}
    aria-valuemax={600}
    aria-valuenow={Math.round($leftColumnWidth)}
    tabindex="0"
    title="Drag to resize file tree (Double-click to reset, Arrow keys to nudge)"
    onpointerdown={startTreeResize}
    onkeydown={nudgeTreeResize}
    ondblclick={() => {
      setLeftColumnWidth(viewportPaneWidths().commits);
    }}
  ></div>

  <!-- Right Code Viewer -->
  <div class="viewer-pane">
    {#if activeFile}
      <FileTabs
        tabs={openTabs}
        active={activeFile}
        onactivate={(path) => {
          activeTargetLine = null;
          openFileTab(repoId, path);
        }}
        onclose={(path) => closeFileTab(repoId, path)}
      />
      <div class="viewer-body">
        <CodeViewer
          {repoId}
          path={activeFile}
          rev={rev || "HEAD"}
          targetLine={activeTargetLine || undefined}
          {paneIndex}
          onopenineditor={openInEditor}
          onnavigatetofile={(filePath, line) => {
            activeTargetLine = line || null;
            openFileTab(repoId, filePath);
          }}
        />
      </div>
    {:else}
      <div class="empty-viewer">
        <div class="empty-viewer-card">
          <Icon name="code" size={36} class="empty-icon" />
          <div class="empty-title">No file open</div>
          <div class="empty-desc">
            Select a file from the explorer on the left.
          </div>
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .files-tab-container {
    display: flex;
    width: 100%;
    flex: 1;
    min-height: 0;
    min-width: 0;
    overflow: hidden;
    background: var(--bg);
  }

  .tree-pane {
    height: 100%;
    background-color: var(--bg);
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    min-width: 0;
    border-right: 1px solid var(--border);
  }

  .tree-header {
    flex-shrink: 0;
    height: 32px;
    display: flex;
    align-items: stretch;
    padding: 0;
    background-color: var(--bg-subtle);
    border-bottom: 1px solid var(--border);
  }

  .tree-body {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .viewer-body {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
  }

  .viewer-pane {
    flex: 1;
    min-width: 0;
    height: 100%;
    display: flex;
    flex-direction: column;
    background-color: var(--bg);
    overflow: hidden;
  }

  .empty-viewer {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    width: 100%;
    padding: 24px;
    color: var(--text-muted);
  }

  .empty-viewer-card {
    text-align: center;
    max-width: 320px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
  }

  :global(.empty-icon) {
    opacity: 0.35;
    margin-bottom: 4px;
  }

  .empty-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--text);
  }

  .empty-desc {
    font-size: 12.5px;
    line-height: 1.5;
  }


  .split-handle {
    flex: 0 0 auto;
    width: 1px;
    height: 100%;
    position: relative;
    cursor: col-resize;
    background-color: var(--border);
    z-index: 2;
  }
  .split-handle::after {
    content: "";
    position: absolute;
    top: 0;
    bottom: 0;
    left: -3px;
    width: 7px;
    background-color: transparent;
    transition: background-color 120ms ease;
  }
  .split-handle:hover::after,
  .split-handle:focus-visible::after {
    background-color: #3b82f6;
    opacity: 0.6;
  }
</style>
