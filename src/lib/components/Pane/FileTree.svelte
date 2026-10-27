<script lang="ts">
  import { tick } from "svelte";
  import {
    buildFileTree,
    filterFileTree,
    flattenVisibleTree,
    collectAllFolderIds,
    getAncestorFolderIds,
    type TreeNode,
    type VisibleNode,
  } from "../../domain/tree";
  import FileIcon from "../common/FileIcon.svelte";
  import Icon from "../common/Icon.svelte";
  import * as api from "../../platform/api";
  import { repos } from "../../domain/stores/repos";
  import { addToast } from "../../app/ui";

  interface Props {
    files: string[];
    selectedFile: string | null;
    repoId: string;
    onselectfile: (path: string) => void;
  }

  let { files, selectedFile, repoId, onselectfile }: Props = $props();

  let repoEntry = $derived($repos.find((r) => r.id === repoId));

  let filterText = $state("");
  let compactFolders = $state(true);
  let expandedDirs = $state<Set<string>>(new Set());
  let focusedRowIndex = $state<number>(-1);
  let searchInputEl = $state<HTMLInputElement | null>(null);
  let treeContainerEl = $state<HTMLDivElement | null>(null);

  // Initialize or expand root folders when files first load
  let initialized = $state(false);
  $effect(() => {
    if (!initialized && files.length > 0) {
      initialized = true;
      // Auto-expand first level directories by default
      const initialTree = buildFileTree(files, { compactSingleChildDirs: compactFolders });
      const initialExp = new Set<string>();
      for (const node of initialTree) {
        if (node.isDir) {
          initialExp.add(node.id);
        }
      }
      expandedDirs = initialExp;

      // Also reveal selectedFile if present
      if (selectedFile) {
        const ancestors = getAncestorFolderIds(selectedFile);
        for (const id of ancestors) initialExp.add(id);
        expandedDirs = new Set(initialExp);
      }
    }
  });

  // Rebuild tree
  let rawTree = $derived(buildFileTree(files, { compactSingleChildDirs: compactFolders }));

  // Filter tree
  let filterResult = $derived(filterFileTree(rawTree, filterText));
  let currentTree = $derived(filterResult.filtered);
  let matchCount = $derived(filterResult.matchCount);

  // Active expanded directories set (union of user-expanded + filter-auto-expanded)
  let effectiveExpandedDirs = $derived.by(() => {
    if (filterText.trim()) {
      const merged = new Set(expandedDirs);
      for (const id of filterResult.autoExpanded) merged.add(id);
      return merged;
    }
    return expandedDirs;
  });

  // Visible flattened rows
  let visibleRows = $derived(flattenVisibleTree(currentTree, effectiveExpandedDirs));

  function toggleFolder(node: TreeNode) {
    if (!node.isDir) return;
    const next = new Set(expandedDirs);
    if (next.has(node.id)) {
      next.delete(node.id);
    } else {
      next.add(node.id);
    }
    expandedDirs = next;
  }

  export function collapseAll() {
    expandedDirs = new Set();
  }

  export function expandAll() {
    expandedDirs = collectAllFolderIds(rawTree);
  }

  export function revealSelected() {
    if (!selectedFile) return;
    const ancestors = getAncestorFolderIds(selectedFile);
    const next = new Set(expandedDirs);
    for (const id of ancestors) next.add(id);
    expandedDirs = next;

    tick().then(() => {
      const el = treeContainerEl?.querySelector(`[data-path="${CSS.escape(selectedFile!)}"]`);
      el?.scrollIntoView({ block: "nearest", behavior: "smooth" });
    });
  }

  export function focusSearch() {
    searchInputEl?.focus();
    searchInputEl?.select();
  }

  function handleRowClick(visible: VisibleNode) {
    if (visible.node.isDir) {
      toggleFolder(visible.node);
    } else {
      onselectfile(visible.node.fullPath);
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      if (visibleRows.length === 0) return;
      focusedRowIndex = Math.min(focusedRowIndex + 1, visibleRows.length - 1);
      scrollToFocused();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (visibleRows.length === 0) return;
      focusedRowIndex = Math.max(focusedRowIndex - 1, 0);
      scrollToFocused();
    } else if (e.key === "Enter") {
      if (focusedRowIndex >= 0 && focusedRowIndex < visibleRows.length) {
        e.preventDefault();
        handleRowClick(visibleRows[focusedRowIndex]);
      }
    } else if (e.key === "ArrowRight") {
      if (focusedRowIndex >= 0 && focusedRowIndex < visibleRows.length) {
        const item = visibleRows[focusedRowIndex];
        if (item.node.isDir && !item.isOpen) {
          e.preventDefault();
          toggleFolder(item.node);
        }
      }
    } else if (e.key === "ArrowLeft") {
      if (focusedRowIndex >= 0 && focusedRowIndex < visibleRows.length) {
        const item = visibleRows[focusedRowIndex];
        if (item.node.isDir && item.isOpen) {
          e.preventDefault();
          toggleFolder(item.node);
        }
      }
    } else if (e.key === "Escape") {
      if (filterText) {
        e.preventDefault();
        filterText = "";
      }
    }
  }

  function scrollToFocused() {
    tick().then(() => {
      const el = treeContainerEl?.querySelector(`[data-index="${focusedRowIndex}"]`);
      el?.scrollIntoView({ block: "nearest" });
    });
  }

  async function copyPath(path: string, e: MouseEvent) {
    e.stopPropagation();
    try {
      await navigator.clipboard.writeText(path);
      addToast({ message: `Copied '${path}' to clipboard`, kind: "info" });
    } catch {
      addToast({ message: "Failed to copy path", kind: "error" });
    }
  }

  async function revealInOs(path: string, e: MouseEvent) {
    e.stopPropagation();
    try {
      const fullOsPath = repoEntry?.path ? `${repoEntry.path}/${path}` : path;
      await api.revealInFileManager(fullOsPath);
    } catch (err: any) {
      addToast({ message: err?.message || "Failed to reveal file", kind: "error" });
    }
  }
</script>

<div class="file-tree-root" role="tree" tabindex="0" onkeydown={handleKeydown}>
  <!-- Explorer Title & Toolbar -->
  <div class="explorer-toolbar">
    <div class="explorer-heading">
      <span class="explorer-label">EXPLORER</span>
      <span class="file-count">({files.length})</span>
    </div>

    <div class="explorer-actions">
      <button
        class="action-icon-btn"
        onclick={collapseAll}
        title="Collapse all folders"
        aria-label="Collapse all folders"
      >
        <Icon name="fold" size={13} />
      </button>

      <button
        class="action-icon-btn"
        onclick={expandAll}
        title="Expand all folders"
        aria-label="Expand all folders"
      >
        <Icon name="unfold" size={13} />
      </button>

      {#if selectedFile}
        <button
          class="action-icon-btn"
          onclick={revealSelected}
          title="Reveal active file in tree"
          aria-label="Reveal active file in tree"
        >
          <Icon name="search" size={12} />
        </button>
      {/if}

      <button
        class="action-icon-btn"
        class:active={compactFolders}
        onclick={() => (compactFolders = !compactFolders)}
        title={compactFolders ? "Disable compact folders" : "Enable compact folders"}
        aria-label="Toggle compact folders"
      >
        <span class="compact-badge">/</span>
      </button>
    </div>
  </div>

  <!-- Search & Filter Bar -->
  <div class="search-box">
    <div class="search-input-wrap">
      <span class="search-icon">
        <Icon name="search" size={13} />
      </span>
      <input
        bind:this={searchInputEl}
        type="text"
        placeholder="Search files (Press /)"
        bind:value={filterText}
        class="search-input"
        spellcheck="false"
        autocomplete="off"
        onkeydown={(e) => {
          if (e.key === "ArrowDown") {
            e.preventDefault();
            focusedRowIndex = 0;
            scrollToFocused();
          }
        }}
      />
      {#if filterText}
        <span class="filter-count">{matchCount}</span>
        <button
          class="clear-filter-btn"
          onclick={() => {
            filterText = "";
            searchInputEl?.focus();
          }}
          title="Clear search"
          aria-label="Clear search"
        >
          <Icon name="close" size={11} />
        </button>
      {/if}
    </div>
  </div>

  <!-- Hierarchical File Tree -->
  <div bind:this={treeContainerEl} class="tree-rows-container">
    {#if visibleRows.length === 0}
      <div class="empty-tree text-muted text-xs">
        {#if filterText}
          No files matching "{filterText}"
        {:else if files.length === 0}
          No files in this repository
        {:else}
          Tree collapsed
        {/if}
      </div>
    {/if}

    {#each visibleRows as visible, idx (visible.node.id)}
      {@const isSelected = !visible.node.isDir && selectedFile === visible.node.fullPath}
      {@const isFocused = idx === focusedRowIndex}

      <div
        class="tree-row"
        class:selected={isSelected}
        class:focused={isFocused}
        class:is-dir={visible.node.isDir}
        style="--depth: {visible.depth};"
        data-index={idx}
        data-path={visible.node.fullPath}
        role="treeitem"
        tabindex={isFocused ? 0 : -1}
        aria-expanded={visible.node.isDir ? visible.isOpen : undefined}
        aria-selected={isSelected}
        onclick={() => {
          focusedRowIndex = idx;
          handleRowClick(visible);
        }}
        onkeydown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            handleRowClick(visible);
          }
        }}
        title={visible.node.fullPath}
      >
        <!-- Indent Guidelines -->
        {#if visible.depth > 0}
          <div class="indent-spacers">
            {#each Array(visible.depth) as _, d}
              <span class="indent-guide" style="left: {d * 14 + 10}px;"></span>
            {/each}
          </div>
        {/if}

        <div class="row-content" style="padding-left: {visible.depth * 14 + 6}px;">
          <!-- Folder Chevron or File Spacer -->
          {#if visible.node.isDir}
            <span class="chevron-wrap" class:open={visible.isOpen}>
              <Icon name="chevron-right" size={11} strokeWidth={2.5} />
            </span>
          {:else}
            <span class="file-spacer"></span>
          {/if}

          <!-- Icon -->
          <FileIcon
            filename={visible.node.name}
            isDir={visible.node.isDir}
            isOpen={visible.isOpen}
            size={14}
          />

          <!-- Label -->
          <span class="node-label truncate">
            {visible.node.name}
          </span>

          <!-- Hover Actions for Files -->
          {#if !visible.node.isDir}
            <div class="row-hover-actions">
              <button
                class="row-action-btn"
                onclick={(e) => copyPath(visible.node.fullPath, e)}
                title="Copy relative path"
                aria-label="Copy relative path"
              >
                <Icon name="copy" size={12} />
              </button>
              <button
                class="row-action-btn"
                onclick={(e) => revealInOs(visible.node.fullPath, e)}
                title="Reveal in Finder / Explorer"
                aria-label="Reveal in Finder"
              >
                <Icon name="external-link" size={12} />
              </button>
            </div>
          {/if}
        </div>
      </div>
    {/each}
  </div>
</div>

<style>
  .file-tree-root {
    display: flex;
    flex-direction: column;
    height: 100%;
    width: 100%;
    user-select: none;
    outline: none;
    background: var(--bg);
    color: var(--text);
  }

  .explorer-toolbar {
    height: 32px;
    padding: 0 10px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-shrink: 0;
    border-bottom: 1px solid var(--border);
    background: var(--chrome, var(--bg));
  }

  .explorer-heading {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .explorer-label {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.5px;
    color: var(--text-muted);
    text-transform: uppercase;
  }

  .file-count {
    font-size: 11px;
    color: var(--text-muted);
    opacity: 0.8;
  }

  .explorer-actions {
    display: flex;
    align-items: center;
    gap: 3px;
  }

  .action-icon-btn {
    width: 22px;
    height: 22px;
    border: 0;
    background: transparent;
    border-radius: var(--r-sm);
    color: var(--text-muted);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    padding: 0;
    transition: background 120ms ease, color 120ms ease;
  }
  .action-icon-btn:hover {
    background: rgba(125, 125, 125, 0.15);
    color: var(--text);
  }
  .action-icon-btn.active {
    background: rgba(125, 125, 125, 0.2);
    color: var(--text);
  }

  .compact-badge {
    font-size: 12px;
    font-weight: 700;
    font-family: ui-monospace, SFMono-Regular, monospace;
    line-height: 1;
  }

  .search-box {
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .search-input-wrap {
    position: relative;
    display: flex;
    align-items: center;
    width: 100%;
    background: var(--bg-hover, rgba(0, 0, 0, 0.04));
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 0 6px;
    height: 26px;
    transition: border-color 140ms ease, box-shadow 140ms ease;
  }
  :global([data-theme="dark"]) .search-input-wrap {
    background: rgba(255, 255, 255, 0.05);
  }
  .search-input-wrap:focus-within {
    border-color: #3b82f6;
    box-shadow: 0 0 0 1px rgba(59, 130, 246, 0.35);
  }

  .search-icon {
    display: inline-flex;
    align-items: center;
    color: var(--text-muted);
    margin-right: 5px;
    flex-shrink: 0;
  }

  .search-input {
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    color: var(--text);
    font-size: 12px;
    outline: none;
    padding: 0;
  }
  .search-input::placeholder {
    color: var(--text-muted);
    opacity: 0.7;
  }

  .filter-count {
    font-size: 10px;
    font-variant-numeric: tabular-nums;
    font-weight: 600;
    color: var(--text-muted);
    background: rgba(125, 125, 125, 0.15);
    padding: 1px 5px;
    border-radius: var(--r-sm);
    margin-right: 4px;
    flex-shrink: 0;
  }

  .clear-filter-btn {
    border: 0;
    background: none;
    color: var(--text-muted);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 2px;
    border-radius: var(--r-sm);
  }
  .clear-filter-btn:hover {
    color: var(--text);
    background: rgba(125, 125, 125, 0.2);
  }

  .tree-rows-container {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    position: relative;
    padding: 4px 0 12px 0;
  }

  .empty-tree {
    padding: 16px 12px;
    text-align: center;
  }

  .tree-row {
    position: relative;
    display: flex;
    align-items: center;
    height: 24px;
    cursor: pointer;
    font-size: 12.5px;
    transition: background 80ms ease;
  }
  .tree-row:hover {
    background: var(--bg-hover, rgba(0, 0, 0, 0.04));
  }
  :global([data-theme="dark"]) .tree-row:hover {
    background: rgba(255, 255, 255, 0.05);
  }
  .tree-row.selected {
    background: rgba(59, 130, 246, 0.15) !important;
    color: #2563eb;
    font-weight: 500;
  }
  :global([data-theme="dark"]) .tree-row.selected {
    background: rgba(59, 130, 246, 0.25) !important;
    color: #60a5fa;
  }
  .tree-row.focused:not(.selected) {
    outline: 1px dotted var(--text-muted);
    outline-offset: -1px;
  }

  .indent-spacers {
    position: absolute;
    top: 0;
    bottom: 0;
    left: 0;
    right: 0;
    pointer-events: none;
  }

  .indent-guide {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1px;
    background: var(--border);
    opacity: 0.55;
  }

  .row-content {
    display: flex;
    align-items: center;
    width: 100%;
    min-width: 0;
    height: 100%;
    padding-right: 6px;
    gap: 5px;
  }

  .chevron-wrap {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 14px;
    height: 14px;
    flex-shrink: 0;
    color: var(--text-muted);
    transition: transform 140ms ease;
  }
  .chevron-wrap.open {
    transform: rotate(90deg);
  }

  .file-spacer {
    width: 14px;
    height: 14px;
    flex-shrink: 0;
  }

  .node-label {
    flex: 1;
    min-width: 0;
    line-height: 1;
  }

  .is-dir .node-label {
    font-weight: 500;
  }

  .row-hover-actions {
    display: none;
    align-items: center;
    gap: 2px;
    margin-left: auto;
    flex-shrink: 0;
  }
  .tree-row:hover .row-hover-actions {
    display: flex;
  }

  .row-action-btn {
    width: 18px;
    height: 18px;
    border: 0;
    background: transparent;
    border-radius: var(--r-sm);
    color: var(--text-muted);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    padding: 0;
  }
  .row-action-btn:hover {
    background: rgba(125, 125, 125, 0.2);
    color: var(--text);
  }
</style>
