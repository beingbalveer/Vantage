<script lang="ts">
  import * as api from "../../api";
  import { normalizeError, createDelayedToggle, type NormalizedError } from "../../asyncState";
  import CodeViewer from "../Viewer/CodeViewer.svelte";
  import { createHorizontalResizer } from "../../resizer";
  import Icon from "../common/Icon.svelte";
  import Skeleton from "../common/Skeleton.svelte";
  import Banner from "../common/Banner.svelte";
  import { navigationTarget } from "../../stores/navigation";

  interface Props {
    repoId: string;
    rev?: string;
    targetFile?: string | null;
    targetLine?: number | null;
  }

  let { repoId, rev = "HEAD", targetFile = null, targetLine: initialTargetLine = null }: Props = $props();

  let fileList = $state<string[]>([]);
  let filterText = $state("");
  let selectedFile = $state<string | null>(null);
  let activeTargetLine = $state<number | null>(null);

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
      ? parseInt(localStorage.getItem("vantage_files_tree_width") || localStorage.getItem("relay_files_tree_width") || "260", 10) || 260
      : 260
  );

  const startTreeResize = createHorizontalResizer({
    getWidth: () => treeWidth,
    setWidth: (w) => (treeWidth = w),
    minWidth: 160,
    maxWidth: 600,
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

  let filteredFiles = $derived.by(() => {
    if (!filterText.trim()) return fileList;
    const q = filterText.toLowerCase();
    return fileList.filter((f) => f.toLowerCase().includes(q));
  });

  async function openInEditor() {
    if (!selectedFile) return;
    try {
      await api.openExternalEditor(repoId, selectedFile, activeTargetLine || undefined);
    } catch (e) {
      console.error("Failed to open external editor:", e);
    }
  }
</script>

<div class="files-tab-container">
  <!-- Left Tree / List -->
  <div class="tree-pane" style="width: {treeWidth}px;">
    <div class="filter-header">
      <input
        type="text"
        placeholder="Filter files (/)"
        bind:value={filterText}
        class="tree-filter-input"
      />
    </div>

    <div class="file-tree-scroll" class:pending={treeLoading && fileList.length > 0} aria-busy={treeLoading || treeInFlight}>
      {#if treeLoading && fileList.length === 0}
        <Skeleton rows={8} height={26} shape="file" label="Loading files" />
      {:else if treeError}
        <Banner kind="error" message={treeError.message} error={treeError} retryLabel="Retry" onretry={loadTree} />
      {:else if filteredFiles.length === 0 && !treeInFlight}
        <div class="empty-tree text-sm text-muted">
          {filterText.trim() ? `No files match "${filterText}".` : "No files yet."}
        </div>
      {/if}

      {#each filteredFiles as f (f)}
        <button
          type="button"
          class="file-row {selectedFile === f ? 'selected' : ''}"
          onclick={() => {
            selectedFile = f;
            activeTargetLine = null;
          }}
          title={f}
        >
          <Icon name="file-text" size={13} class="file-icon" />
          <span class="file-path truncate">{f}</span>
        </button>
      {/each}
    </div>
  </div>

  <div
    class="split-handle"
    role="separator"
    aria-orientation="vertical"
    title="Drag to resize file tree (Double-click to reset)"
    onpointerdown={startTreeResize}
    ondblclick={() => {
      treeWidth = 260;
      try { localStorage.setItem("vantage_files_tree_width", "260"); } catch {}
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
        onopenineditor={openInEditor}
        onnavigatetofile={(filePath, line) => {
          selectedFile = filePath;
          activeTargetLine = line || null;
        }}
      />
    {:else}
      <div class="empty-viewer text-muted">Select a file to view</div>
    {/if}
  </div>
</div>

<style>
  .files-tab-container {
    display: flex;
    width: 100%;
    height: 100%;
    min-width: 0;
    overflow: hidden;
  }

  .tree-pane {
    height: 100%;
    background-color: var(--bg-subtle);
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    min-width: 0;
  }

  .filter-header {
    padding: 8px 12px;
    border-bottom: 1px solid var(--border-subtle);
  }

  .tree-filter-input {
    width: 100%;
    padding: 4px 8px;
    font-size: 12px;
    background-color: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    color: var(--text);
    outline: none;
  }
  .tree-filter-input:focus {
    border-color: var(--accent);
  }

  .file-tree-scroll {
    flex: 1;
    overflow-y: auto;
    padding: 4px 0;
    transition: opacity 120ms ease;
  }
  .file-tree-scroll.pending {
    opacity: 0.6;
  }

  .empty-tree {
    padding: 16px 12px;
  }

  .file-row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 12px;
    border: none;
    background: transparent;
    color: var(--text);
    font-size: 12px;
    font-family: var(--font-mono);
    text-align: left;
    cursor: pointer;
    user-select: none;
  }
  .file-row:hover {
    background-color: var(--bg-muted);
  }
  .file-row.selected {
    background-color: var(--accent-bg);
    color: var(--accent);
    font-weight: 500;
  }

  :global(.file-icon) {
    color: var(--text-muted);
    flex-shrink: 0;
  }
  .file-row.selected :global(.file-icon) {
    color: var(--accent);
  }

  .file-path {
    flex: 1;
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
    font-size: 13px;
  }

  .split-handle {
    flex: 0 0 auto;
    width: 0;
    height: 100%;
    position: relative;
    cursor: col-resize;
    background-color: transparent;
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
    background-color: var(--accent);
    opacity: 0.35;
  }
</style>
