<script lang="ts">
  import { onMount, onDestroy, untrack } from "svelte";
  import type { RepoEntry, RepoStatus, CommitSummary, FileChange, DiffFile, DiffScope } from "../../types";
  import * as api from "../../api";
  import { settings } from "../../stores/settings";
  import { panes, focusedPaneIndex, focusPane } from "../../stores/panes";
  import { addToast, openDialog } from "../../stores/ui";
  import { navigationTarget, navigateTo } from "../../stores/navigation";
  import type { MenuItem } from "../../menu";
  import ContextMenu from "../common/ContextMenu.svelte";
  import UncommittedRow from "./UncommittedRow.svelte";
  import CommitRow from "./CommitRow.svelte";
  import CommitHeader from "./CommitHeader.svelte";
  import FileList from "./FileList.svelte";
  import CommitBox from "./CommitBox.svelte";
  import DiffView from "../Diff/DiffView.svelte";
  import FilesTab from "./FilesTab.svelte";
  import TerminalDrawer from "../Terminal/TerminalDrawer.svelte";
  import Icon from "../common/Icon.svelte";
  import { viewportPaneWidths, savedPaneWidth } from "../../layout";
  import { createHorizontalResizer, createHorizontalResizerKeyboard } from "../../resizer";

  interface Props {
    paneIndex: number;
    repo: RepoEntry | null;
    status: RepoStatus | null;
  }

  let { paneIndex, repo, status }: Props = $props();

  let currentPane = $derived($panes[paneIndex]);
  let isFocused = $derived($focusedPaneIndex === paneIndex);

  // Column Resizing & Focus Diff
  let focusDiff = $state(false);
  // Commit controls are revealed on demand so they don't add permanent UI the
  // reference does not show (RELAY-UI · UI-06).
  let commitMode = $state(false);
  let showCommitDetails = $state(false);
  let commitsColWidth = $state(savedPaneWidth("vantage_commits_col_width", viewportPaneWidths().commits));
  let filesColWidth = $state(savedPaneWidth("vantage_files_col_width", viewportPaneWidths().files));

  const startCommitsResize = createHorizontalResizer({
    getWidth: () => commitsColWidth,
    setWidth: (w) => (commitsColWidth = w),
    minWidth: 180,
    maxWidth: 700,
    storageKey: "vantage_commits_col_width",
  });

  const startFilesResize = createHorizontalResizer({
    getWidth: () => filesColWidth,
    setWidth: (w) => (filesColWidth = w),
    minWidth: 160,
    maxWidth: 600,
    storageKey: "vantage_files_col_width",
  });

  const nudgeCommitsResize = createHorizontalResizerKeyboard({
    getWidth: () => commitsColWidth,
    setWidth: (w) => (commitsColWidth = w),
    minWidth: 180,
    maxWidth: 700,
    storageKey: "vantage_commits_col_width",
  });

  const nudgeFilesResize = createHorizontalResizerKeyboard({
    getWidth: () => filesColWidth,
    setWidth: (w) => (filesColWidth = w),
    minWidth: 160,
    maxWidth: 600,
    storageKey: "vantage_files_col_width",
  });

  // Commits & Selection
  let commits = $state<CommitSummary[]>([]);
  let selectedCommitSha = $state<string | null>(null);
  let isUncommittedSelected = $state(false);
  let isRangeSelected = $state(false);
  let currentScope = $state<DiffScope>({ type: "working" });

  let selectedCommit = $derived(
    selectedCommitSha && !isUncommittedSelected
      ? commits.find((c) => c.sha === selectedCommitSha) || null
      : null
  );

  // Prototype parity: the commits column ends with a quiet branch summary
  // (approved-prototype.html · .history-footer). The history is paged at 200,
  // so the count is reported as "200+" when the page is full.
  let branchCommitLabel = $derived(
    commits.length === 0
      ? ""
      : `${commits.length >= 200 ? "200+" : commits.length} ${
          commits.length === 1 ? "commit" : "commits"
        } on this branch`
  );

  let unpushedShas = $derived.by(() => {
    const ahead = status?.ahead || 0;
    if (ahead <= 0) return new Set<string>();
    return new Set(commits.slice(0, ahead).map((c) => c.sha));
  });

  let lastAhead = $state<number | undefined>(undefined);
  let lastBehind = $state<number | undefined>(undefined);
  $effect(() => {
    const a = status?.ahead;
    const b = status?.behind;
    if (lastAhead !== undefined && lastBehind !== undefined) {
      if (a !== lastAhead || b !== lastBehind) {
        untrack(() => {
          loadHistory(true);
        });
      }
    }
    lastAhead = a;
    lastBehind = b;
  });

  // Files & Diff
  let scopeFilesList = $state<FileChange[]>([]);
  let selectedFilePath = $state<string | null>(null);
  let currentDiffFile = $state<DiffFile | null>(null);
  let checkedPaths = $state<string[]>([]);

  // Generation IDs to avoid stale async overwrites (R11)
  let historyGen = 0;
  let scopeGen = 0;
  let diffGen = 0;

  let lastLoadedRepoId = "";

  // Watch repo changes
  $effect(() => {
    const currentId = repo?.id || "";
    if (currentId !== lastLoadedRepoId) {
      lastLoadedRepoId = currentId;
      untrack(() => {
        if (currentId) {
          loadHistory();
        } else {
          commits = [];
          scopeFilesList = [];
          currentDiffFile = null;
          selectedCommitSha = null;
          selectedFilePath = null;
          isUncommittedSelected = false;
          isRangeSelected = false;
        }
      });
    }
  });

  // Listen to navigation target (R17)
  $effect(() => {
    if ($navigationTarget && repo) {
      if (!$navigationTarget.repoId || $navigationTarget.repoId === repo.id) {
        if ($navigationTarget.tab && currentPane) {
          $panes[paneIndex].tab = $navigationTarget.tab;
        }
        if ($navigationTarget.commitSha) {
          selectCommit($navigationTarget.commitSha);
        }
        if ($navigationTarget.path) {
          selectedFilePath = $navigationTarget.path;
          selectFile($navigationTarget.path);
        }
      }
    }
  });

  // Re-fetch diff when whitespace or context line settings change (R19)
  let lastWhitespace = $settings.ignoreWhitespace;
  let lastContext = $settings.contextLines;
  $effect(() => {
    const ws = $settings.ignoreWhitespace;
    const ctx = $settings.contextLines;
    if (ws !== lastWhitespace || ctx !== lastContext) {
      lastWhitespace = ws;
      lastContext = ctx;
      if (selectedFilePath && repo) {
        selectFile(selectedFilePath);
      }
    }
  });

  // Invalidate workspace on backend events while preserving selection (R12)
  let unlistenHead: (() => void) | null = null;
  let unlistenStatus: (() => void) | null = null;

  onMount(() => {
    let active = true;
    (async () => {
      try {
        const h = await api.onHeadChanged((payload: { repoId: string; sha: string }) => {
          if (!active || !repo || payload.repoId !== repo.id) return;
          loadHistory(true);
        });
        unlistenHead = h;

        const s = await api.onRepoStatus((st: RepoStatus) => {
          if (!active || !repo || st.repoId !== repo.id) return;
          if (isUncommittedSelected) {
            loadScopeFiles({ type: "working" }, true);
          }
        });
        unlistenStatus = s;
      } catch (e) {
        console.warn("Event listener registration failed:", e);
      }
    })();

    return () => {
      active = false;
      unlistenHead?.();
      unlistenStatus?.();
    };
  });

  async function loadHistory(preserveSelection = false) {
    if (!repo) return;
    const gen = ++historyGen;
    try {
      const log = await api.logPage(repo.id, 0, 200);
      if (gen !== historyGen) return;
      commits = log;

      if (!preserveSelection) {
        // Select default view
        if (status?.dirtyCount && status.dirtyCount > 0) {
          selectUncommitted();
        } else if (status?.newCommitCount && status.newCommitCount > 0 && repo.marker) {
          selectRange();
        } else if (log.length > 0) {
          selectCommit(log[0].sha);
        }
      } else {
        // If current selection is commit, reload files; if uncommitted, reload files
        if (isUncommittedSelected) {
          loadScopeFiles({ type: "working" }, true);
        } else if (isRangeSelected && repo.marker) {
          loadScopeFiles({ type: "range", from: repo.marker.sha, to: "HEAD" }, true);
        } else if (selectedCommitSha) {
          loadScopeFiles({ type: "commit", sha: selectedCommitSha }, true);
        }
      }
    } catch (e) {
      console.error("Failed to load history:", e);
    }
  }

  function selectUncommitted() {
    isUncommittedSelected = true;
    isRangeSelected = false;
    selectedCommitSha = null;
    currentScope = { type: "working" };
    commitMode = false;
    showCommitDetails = false;
    loadScopeFiles(currentScope);
  }

  function selectCommit(sha: string) {
    selectedCommitSha = sha;
    isUncommittedSelected = false;
    isRangeSelected = false;
    currentScope = { type: "commit", sha };
    commitMode = false;
    showCommitDetails = false;
    loadScopeFiles(currentScope);
  }

  function selectRange() {
    if (!repo?.marker) return;
    isRangeSelected = true;
    isUncommittedSelected = false;
    selectedCommitSha = null;
    currentScope = { type: "range", from: repo.marker.sha, to: "HEAD" };
    commitMode = false;
    showCommitDetails = false;
    loadScopeFiles(currentScope);
  }

  async function loadScopeFiles(scope: DiffScope, preserveFile = false) {
    if (!repo) return;
    const gen = ++scopeGen;
    try {
      const files = await api.scopeFiles(repo.id, scope, $settings.ignoreWhitespace);
      if (gen !== scopeGen) return;

      scopeFilesList = files;
      checkedPaths = files.map((f) => f.path);

      if (files.length > 0) {
        const fileToSelect =
          preserveFile && selectedFilePath && files.some((f) => f.path === selectedFilePath)
            ? selectedFilePath
            : files[0].path;
        selectFile(fileToSelect, scope);
      } else {
        selectedFilePath = null;
        currentDiffFile = null;
      }
    } catch (e: any) {
      if (gen !== scopeGen) return;
      console.error("Failed to load scope files:", e);
      addToast({ message: e.message || "Failed to load files for commit", kind: "error" });
    }
  }

  async function selectFile(path: string, scope?: DiffScope) {
    if (!repo) return;
    const gen = ++diffGen;
    selectedFilePath = path;
    const activeScope = scope || currentScope;

    try {
      const diff = await api.scopeFileDiff(
        repo.id,
        activeScope,
        path,
        $settings.contextLines,
        $settings.ignoreWhitespace,
        false
      );
      if (gen !== diffGen) return;
      currentDiffFile = diff;
    } catch (e: any) {
      if (gen !== diffGen) return;
      console.error("Failed to load file diff:", e);
      addToast({ message: e.message || "Failed to load diff", kind: "error" });
    }
  }

  function toggleCheckedPath(path: string) {
    if (checkedPaths.includes(path)) {
      checkedPaths = checkedPaths.filter((p) => p !== path);
    } else {
      checkedPaths = [...checkedPaths, path];
    }
  }

  function toggleAllChecked(selectAll: boolean) {
    if (selectAll) {
      checkedPaths = scopeFilesList.map((f) => f.path);
    } else {
      checkedPaths = [];
    }
  }

  // ---- Row context menus (commit + changed file) ----
  type MenuTarget =
    | { kind: "commit"; commit: CommitSummary }
    | { kind: "file"; file: FileChange };

  let contextMenu = $state<{ x: number; y: number; target: MenuTarget } | null>(null);

  function openCommitMenu(commit: CommitSummary, pos: { x: number; y: number }) {
    contextMenu = { x: pos.x, y: pos.y, target: { kind: "commit", commit } };
  }

  function openFileMenu(file: FileChange, pos: { x: number; y: number }) {
    contextMenu = { x: pos.x, y: pos.y, target: { kind: "file", file } };
  }

  function closeContextMenu() {
    contextMenu = null;
  }

  async function copyCommitSha(sha: string) {
    try {
      await navigator.clipboard.writeText(sha);
      addToast({ message: "Commit SHA copied to clipboard", kind: "success" });
    } catch {
      addToast({ message: "Failed to copy SHA", kind: "error" });
    }
  }

  async function copyFilePath(path: string) {
    try {
      await navigator.clipboard.writeText(path);
      addToast({ message: "File path copied to clipboard", kind: "success" });
    } catch {
      addToast({ message: "Failed to copy path", kind: "error" });
    }
  }

  async function revealFilePath(path: string) {
    if (!repo) return;
    try {
      await api.revealInFileManager(`${repo.path}/${path}`);
    } catch (e: any) {
      addToast({ message: e.message || "Failed to reveal in file manager", kind: "error" });
    }
  }

  function openRevertDialog(commit: CommitSummary) {
    if (!repo) return;
    openDialog({ type: "revert", repoId: repo.id, data: { sha: commit.sha, commit } });
  }

  function openResetDialog(commit: CommitSummary) {
    if (!repo) return;
    const index = commits.findIndex((c) => c.sha === commit.sha);
    const dropped = index > 0 ? commits.slice(0, index) : [];
    openDialog({
      type: "reset",
      repoId: repo.id,
      data: {
        targetSha: commit.sha,
        targetLabel: "this commit",
        commits: dropped,
      },
    });
  }

  function openDiscardDialog(paths: string[]) {
    if (!repo || paths.length === 0) return;
    openDialog({ type: "discard", repoId: repo.id, data: { paths: [...paths] } });
  }

  function discardSelected() {
    openDiscardDialog(checkedPaths);
  }

  let menuItems = $derived.by<MenuItem[]>(() => {
    const menu = contextMenu;
    if (!menu || !repo) return [];

    if (menu.target.kind === "commit") {
      const commit = menu.target.commit;
      const index = commits.findIndex((c) => c.sha === commit.sha);
      // "Reset to here" needs at least one commit above the target to drop.
      const canReset = index > 0;
      return [
        {
          id: "revert",
          label: "Revert commit…",
          onselect: () => openRevertDialog(commit),
        },
        {
          id: "reset",
          label: "Reset to here…",
          danger: true,
          disabled: !canReset,
          onselect: () => openResetDialog(commit),
        },
        {
          id: "copy-sha",
          label: "Copy SHA",
          shortcut: "c",
          onselect: () => void copyCommitSha(commit.sha),
        },
        {
          id: "view-diff",
          label: "View diff",
          onselect: () => selectCommit(commit.sha),
        },
      ];
    }

    const file = menu.target.file;
    const items: MenuItem[] = [];
    if (isUncommittedSelected) {
      items.push({
        id: "discard-file",
        label: "Discard changes…",
        danger: true,
        onselect: () => openDiscardDialog([file.path]),
      });
    }
    items.push({
      id: "copy-path",
      label: "Copy path",
      onselect: () => void copyFilePath(file.path),
    });
    items.push({
      id: "reveal",
      label: "Show in file manager",
      onselect: () => void revealFilePath(file.path),
    });
    return items;
  });

  function isCommitMenuOpen(sha: string): boolean {
    return contextMenu?.target.kind === "commit" && contextMenu.target.commit.sha === sha;
  }

  let openMenuFilePath = $derived(
    contextMenu?.target.kind === "file" ? contextMenu.target.file.path : null
  );
</script>

<div
  class="pane-container {isFocused ? 'focused' : ''}"
  onpointerdown={() => focusPane(paneIndex)}
  onfocusin={() => focusPane(paneIndex)}
  role="region"
  aria-label="Repository Pane {paneIndex + 1}"
>
  <!-- Pane Body -->
  <div class="pane-content">
    {#if !repo}
      <div class="empty-pane">
        <span class="text-strong">No repository selected</span>
        <span class="text-sm text-muted">Select a repository from the sidebar or press Mod+O to add one.</span>
      </div>
    {:else if currentPane.tab === "files"}
      <FilesTab repoId={repo.id} rev={repo.branch || "HEAD"} />
    {:else}
      <!-- History Tab 3-Column Layout -->
      <div class="history-layout">
        <!-- Commits Column -->
        {#if !focusDiff}
          <div class="commits-col" style="width: {commitsColWidth}px;">
            <!-- Commits List -->
            <div class="commits-scroll">
              <UncommittedRow
                dirtyCount={status?.dirtyCount || 0}
                isSelected={isUncommittedSelected}
                onselect={selectUncommitted}
              />

              {#if commits.length === 0}
                <div class="empty-commits text-sm text-muted">No commits yet</div>
              {/if}

              {#each commits as commit (commit.sha)}
                <CommitRow
                  {commit}
                  isSelected={selectedCommitSha === commit.sha}
                  isLocal={unpushedShas.has(commit.sha)}
                  isMenuOpen={isCommitMenuOpen(commit.sha)}
                  onselect={() => selectCommit(commit.sha)}
                  onmenu={(pos) => openCommitMenu(commit, pos)}
                />
              {/each}
            </div>

            {#if branchCommitLabel}
              <div class="history-footer">{branchCommitLabel}</div>
            {/if}
          </div>

          <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
          <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
          <div
            class="split-handle"
            role="separator"
            aria-orientation="vertical"
            aria-label="Resize commit list"
            tabindex="0"
            title="Drag to resize commits column (Double-click to reset, Arrow keys to nudge)"
            onpointerdown={startCommitsResize}
            onkeydown={nudgeCommitsResize}
            ondblclick={() => {
              commitsColWidth = viewportPaneWidths().commits;
              try { localStorage.setItem("vantage_commits_col_width", String(commitsColWidth)); } catch {}
            }}
          ></div>
        {/if}

        <!-- Detail Layout: Files and Diff columns -->
        <div class="detail-layout">
          {#if selectedCommit && !isUncommittedSelected && showCommitDetails}
            <CommitHeader commit={selectedCommit} />
          {/if}

          <div class="detail-split">
            <!-- Changed Files Column -->
            <div class="files-col" style="width: {focusDiff ? '300px' : filesColWidth + 'px'};">
              <FileList
                files={scopeFilesList}
                selectedFile={selectedFilePath}
                isUncommitted={isUncommittedSelected}
                {checkedPaths}
                showCommitControls={isUncommittedSelected && commitMode}
                {focusDiff}
                hasCommitDetails={!!selectedCommit && !isUncommittedSelected}
                {showCommitDetails}
                menuOpenPath={openMenuFilePath}
                ontogglecommitdetails={() => (showCommitDetails = !showCommitDetails)}
                ontogglefocusdiff={() => (focusDiff = !focusDiff)}
                onselectfile={(path) => selectFile(path)}
                ontogglepath={toggleCheckedPath}
                ontoggleall={toggleAllChecked}
                oncommitrequest={() => (commitMode = true)}
                ondiscardrequest={discardSelected}
                onfilemenu={openFileMenu}
                onbrowseall={() => ($panes[paneIndex].tab = "files")}
              />

              <!-- Commit Box revealed on demand when uncommitted changes are selected -->
              {#if isUncommittedSelected && commitMode}
                <CommitBox
                  repoId={repo.id}
                  branch={status?.branch || "main"}
                  selectedPaths={checkedPaths}
                  oncommit={() => {
                    api.repoStatus(repo.id).then((s) => {
                      loadHistory();
                    });
                  }}
                />
              {/if}
            </div>

            <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
            <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
            <div
              class="split-handle"
              role="separator"
              aria-orientation="vertical"
              aria-label="Resize file list"
              tabindex="0"
              title="Drag to resize files column (Double-click to reset, Arrow keys to nudge)"
              onpointerdown={startFilesResize}
              onkeydown={nudgeFilesResize}
              ondblclick={() => {
                filesColWidth = viewportPaneWidths().files;
                try { localStorage.setItem("vantage_files_col_width", String(filesColWidth)); } catch {}
              }}
            ></div>

            <div class="diff-col">
              <DiffView
                diffFile={currentDiffFile}
                diffMode={$settings.diffMode}
                ignoreWhitespace={$settings.ignoreWhitespace}
                repoId={repo.id}
                rev={selectedCommitSha || repo.branch || "HEAD"}
                onviewfile={(path) => {
                  navigateTo({
                    repoId: repo.id,
                    tab: "files",
                    path,
                  });
                }}
                onloadforce={() => {
                  if (selectedFilePath) selectFile(selectedFilePath);
                }}
              />
            </div>
          </div>
        </div>
      </div>
    {/if}
  </div>

  <!-- Terminal Drawer: keyed by repo.id so switching repositories recreates clean session (R13) -->
  {#if currentPane.terminalOpen && repo}
    {#key repo.id}
      <TerminalDrawer
        repoId={repo.id}
        repoName={repo.name}
        repoPath={repo.path}
        height={currentPane.terminalHeight}
        onclose={() => {
          $panes[paneIndex].terminalOpen = false;
        }}
      />
    {/key}
  {/if}

  <!-- Row context menu (commit / changed file). Recreated per open so it
       re-measures and re-positions itself. -->
  {#if contextMenu}
    {#key contextMenu}
      <ContextMenu
        x={contextMenu.x}
        y={contextMenu.y}
        items={menuItems}
        label={contextMenu.target.kind === "commit" ? "Commit actions" : "File actions"}
        onclose={closeContextMenu}
      />
    {/key}
  {/if}
</div>

<style>
  .pane-container {
    flex: 1;
    min-width: 0;
    height: 100%;
    display: flex;
    flex-direction: column;
    background-color: var(--bg);
    position: relative;
    overflow: hidden;
  }

  .pane-content {
    flex: 1;
    min-width: 0;
    position: relative;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .empty-pane {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 8px;
    color: var(--text);
  }

  .history-layout {
    display: flex;
    width: 100%;
    height: 100%;
    min-width: 0;
    overflow: hidden;
  }

  .commits-col {
    display: flex;
    flex-direction: column;
    height: 100%;
    background-color: var(--bg);
    flex-shrink: 1;
    min-width: 180px;
    border-right: 1px solid var(--border);
  }

  .commits-scroll {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
  }

  .empty-commits {
    padding: 24px 16px;
    text-align: center;
  }

  /* Quiet branch summary pinned to the foot of the commits column
     (approved-prototype.html · .history-footer). */
  .history-footer {
    flex-shrink: 0;
    padding: 16px 19px;
    color: #7c847a;
    font-size: 11px;
    user-select: none;
  }

  .detail-layout {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    height: 100%;
    overflow: hidden;
  }

  .detail-split {
    flex: 1;
    display: flex;
    min-width: 0;
    min-height: 0;
    height: 100%;
    overflow: hidden;
  }

  .files-col {
    display: flex;
    flex-direction: column;
    height: 100%;
    background-color: var(--bg);
    flex-shrink: 1;
    min-width: 160px;
    border-right: 1px solid var(--border);
  }

  .diff-col {
    flex: 1;
    min-width: 260px;
    height: 100%;
    background-color: var(--bg);
    overflow: hidden;
  }

  /* Thin separator with a wider invisible grab target (RELAY-UI · UI-08). */
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
