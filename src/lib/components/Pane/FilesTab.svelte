<script lang="ts">
  import * as api from "../../api";
  import { normalizeError, createDelayedToggle, type NormalizedError } from "../../asyncState";
  import type { RepoEntry, RepoStatus } from "../../types";
  import { setPaneTab } from "../../stores/panes";
  import CodeViewer from "../Viewer/CodeViewer.svelte";
  import FileTree from "./FileTree.svelte";
  import BranchDropdown from "./BranchDropdown.svelte";
  import { createHorizontalResizer } from "../../resizer";
  import { registerHandler, ownsPaneKeyboard } from "../../commands";
  import Icon from "../common/Icon.svelte";
  import Skeleton from "../common/Skeleton.svelte";
  import Banner from "../common/Banner.svelte";
  import { navigationTarget } from "../../stores/navigation";
  import { addToast } from "../../stores/ui";
  import { matchShortcut } from "../../keys";

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
  let selectedFile = $state<string | null>(null);
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

  async function loadTree() {
    if (!repoId) return;
    const gen = ++treeGen;
    treeRequests += 1;
    treePending.begin();
    treeError = null;
    try {
      const files = await api.treeList(repoId, rev || "HEAD");
      if (gen !== treeGen) return;
      fileList = files;
      if (files.length > 0 && (!selectedFile || !files.includes(selectedFile))) {
        selectedFile = files[0];
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
    // Track identity so a repo/rev switch clears the previous tree.
    if (!repoId) return;
    fileList = [];
    selectedFile = null;
    treeError = null;
    loadTree();
    return () => {
      // Only invalidate the in-flight request; the pending toggle is left to
      // its own begin/end balance so overlapping loads stay tracked.
      treeGen++;
    };
  });

  $effect(() => {
    if (targetFile) selectedFile = targetFile;
    if (initialTargetLine) activeTargetLine = initialTargetLine;
  });

  let treeWidth = $state(
    typeof localStorage !== "undefined"
      ? parseInt(localStorage.getItem("vantage_files_tree_width") || localStorage.getItem("relay_files_tree_width") || "280", 10) || 280
      : 280
  );

  const startTreeResize = createHorizontalResizer({
    getWidth: () => treeWidth,
    setWidth: (w) => (treeWidth = w),
    minWidth: 180,
    maxWidth: 700,
    storageKey: "vantage_files_tree_width",
  });

  $effect(() => {
    if ($navigationTarget) {
      if (!$navigationTarget.repoId || $navigationTarget.repoId === repoId) {
        if ($navigationTarget.path) {
          selectedFile = $navigationTarget.path;
        }
        if ($navigationTarget.line) {
          activeTargetLine = $navigationTarget.line;
        }
      }
    }
  });

  async function openInEditor() {
    if (!selectedFile) return;
    try {
      await api.openExternalEditor(repoId, selectedFile, activeTargetLine || undefined);
      addToast({ message: `Opened '${selectedFile}' in external editor`, kind: "info" });
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
      fileTreeRef?.focusSearch();
    } else if (matchShortcut(e, "Mod+P")) {
      e.preventDefault();
      fileTreeRef?.focusSearch();
    }
  }

  function selectFileAt(index: number) {
    const file = fileList[index];
    if (!file) return;
    selectedFile = file;
    activeTargetLine = null;
  }

  function moveFile(delta: 1 | -1): boolean {
    if (!ownsPaneKeyboard(paneIndex) || fileList.length === 0) return false;
    const current = selectedFile ? fileList.indexOf(selectedFile) : -1;
    const next = Math.max(0, Math.min(fileList.length - 1, (current < 0 ? 0 : current) + delta));
    selectFileAt(next);
    return true;
  }

  $effect(() => {
    const unregister = [
      registerHandler(
        "filter_files",
        () => {
          if (!ownsPaneKeyboard(paneIndex)) return false;
          fileTreeRef?.focusSearch?.();
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
  <div class="tree-pane" style="width: {treeWidth}px;">
    <!-- Coordinated 40px Header matching commits-header in Pane.svelte -->
    <div class="tree-header">
      <div class="branch-wrap">
        <BranchDropdown {repo} {status} />
      </div>

      <div class="pane-view-tabs">
        <button
          type="button"
          class="view-tab"
          onclick={() => setPaneTab(paneIndex, "history")}
          title="Switch to Commits & History"
          aria-label="Commits view"
        >
          <Icon name="commit" size={13} />
          <span>Commits</span>
        </button>
        <button
          type="button"
          class="view-tab active"
          title="Project Files Explorer"
          aria-label="Files explorer"
        >
          <Icon name="folder" size={13} />
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
          {selectedFile}
          {repoId}
          onselectfile={(path) => {
            selectedFile = path;
            activeTargetLine = null;
          }}
        />
      {/if}
    </div>
  </div>

  <div
    class="split-handle"
    role="separator"
    aria-orientation="vertical"
    title="Drag to resize file tree (Double-click to reset)"
    onpointerdown={startTreeResize}
    ondblclick={() => {
      treeWidth = 280;
      try { localStorage.setItem("vantage_files_tree_width", "280"); } catch {}
    }}
  ></div>

  <!-- Right Code Viewer -->
  <div class="viewer-pane">
    {#if selectedFile}
      <CodeViewer
        {repoId}
        path={selectedFile}
        rev={rev || "HEAD"}
        targetLine={activeTargetLine || undefined}
        {paneIndex}
        onopenineditor={openInEditor}
        onnavigatetofile={(filePath, line) => {
          selectedFile = filePath;
          activeTargetLine = line || null;
        }}
      />
    {:else}
      <div class="empty-viewer">
        <div class="empty-viewer-card">
          <Icon name="code" size={36} class="empty-icon" />
          <div class="empty-title">No file open</div>
          <div class="empty-desc">
            Select a file from the explorer on the left or press <kbd>/</kbd> to search files.
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
    height: 40px;
    border-bottom: 1px solid var(--border);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 8px;
    background-color: var(--bg);
    gap: 8px;
  }

  .branch-wrap {
    min-width: 0;
    flex: 1;
    overflow: hidden;
  }

  .pane-view-tabs {
    display: inline-flex;
    align-items: center;
    background: var(--bg-hover, rgba(0, 0, 0, 0.05));
    padding: 2px;
    border-radius: 6px;
    border: 1px solid var(--border);
    flex-shrink: 0;
  }
  :global([data-theme="dark"]) .pane-view-tabs {
    background: rgba(255, 255, 255, 0.06);
  }

  .view-tab {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 8px;
    font-size: 11.5px;
    font-weight: 500;
    border: 0;
    background: transparent;
    border-radius: 4px;
    color: var(--text-muted);
    cursor: pointer;
    transition: background 120ms ease, color 120ms ease;
    white-space: nowrap;
  }
  .view-tab:hover:not(.active) {
    color: var(--text);
    background: rgba(125, 125, 125, 0.12);
  }
  .view-tab.active {
    background: var(--bg);
    color: var(--text);
    font-weight: 600;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.08);
  }
  :global([data-theme="dark"]) .view-tab.active {
    background: #21262d;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.35);
  }

  .tree-body {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
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

  kbd {
    display: inline-block;
    padding: 1px 5px;
    font-size: 11px;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    background: var(--bg-hover, rgba(0, 0, 0, 0.08));
    border: 1px solid var(--border);
    border-radius: 4px;
    box-shadow: 0 1px 0 rgba(0, 0, 0, 0.1);
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
