<script lang="ts">
  import { onMount, onDestroy, untrack } from "svelte";
  import type { RepoEntry, RepoStatus, CommitSummary, FileChange, DiffFile, DiffScope } from "../../types";
  import * as api from "../../api";
  import { settings } from "../../stores/settings";
  import { panes, focusedPaneIndex, focusPane } from "../../stores/panes";
  import { addToast } from "../../stores/ui";
  import { refreshSingleStatus } from "../../stores/repos";
  import { navigationTarget, navigateTo } from "../../stores/navigation";
  import {
    normalizeError,
    createDelayedToggle,
    emptyFileListMessage,
    type NormalizedError,
  } from "../../asyncState";
  import UncommittedRow from "./UncommittedRow.svelte";
  import CommitRow from "./CommitRow.svelte";
  import CommitHeader from "./CommitHeader.svelte";
  import FileList from "./FileList.svelte";
  import CommitBox from "./CommitBox.svelte";
  import DiffView from "../Diff/DiffView.svelte";
  import FilesTab from "./FilesTab.svelte";
  import TerminalDrawer from "../Terminal/TerminalDrawer.svelte";
  import Icon from "../common/Icon.svelte";
  import Skeleton from "../common/Skeleton.svelte";
  import EmptyState from "../common/EmptyState.svelte";
  import Banner from "../common/Banner.svelte";
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

  // ---- Loading / empty / error state (ALPH-150) -------------------------
  // Each async read keeps its own pending flag and normalized error so the UI
  // can show a skeleton, a calm empty state, or a persistent error with retry
  // instead of leaving stale content on screen.
  let historyLoading = $state(false);
  let historyError = $state<NormalizedError | null>(null);
  let historyFailureCount = 0;

  let scopeLoading = $state(false);
  let scopeError = $state<NormalizedError | null>(null);
  let scopeFailureCount = 0;

  let diffLoading = $state(false);
  let diffError = $state<NormalizedError | null>(null);
  let diffFailureCount = 0;

  const historyPending = createDelayedToggle(150, (v) => (historyLoading = v));
  const scopePending = createDelayedToggle(150, (v) => (scopeLoading = v));
  const diffPending = createDelayedToggle(150, (v) => (diffLoading = v));

  // Immediate request counters so the columns can stay quiet (rather than flash
  // an empty state) during the first 150ms of a fetch, when the delayed
  // skeleton has not been revealed yet.
  let historyRequests = $state(0);
  let scopeRequests = $state(0);
  let diffRequests = $state(0);
  let historyInFlight = $derived(historyRequests > 0);
  let scopeInFlight = $derived(scopeRequests > 0);
  let diffInFlight = $derived(diffRequests > 0);

  function beginHistoryRequest() {
    historyRequests += 1;
    historyPending.begin();
  }
  function endHistoryRequest() {
    historyRequests = Math.max(0, historyRequests - 1);
    historyPending.end();
  }
  function beginScopeRequest() {
    scopeRequests += 1;
    scopePending.begin();
  }
  function endScopeRequest() {
    scopeRequests = Math.max(0, scopeRequests - 1);
    scopePending.end();
  }
  function beginDiffRequest() {
    diffRequests += 1;
    diffPending.begin();
  }
  function endDiffRequest() {
    diffRequests = Math.max(0, diffRequests - 1);
    diffPending.end();
  }

  // A repo whose path vanished or isn't a repo reports the failure through the
  // status store; surface it as a first-class pane state.
  let repoError = $derived(status?.error ? normalizeError(status.error) : null);

  let fileEmptyCopy = $derived(
    emptyFileListMessage(currentScope.type, status?.dirtyCount || 0)
  );

  function resetWorkspaceContent() {
    // Invalidate any in-flight request for the previous repository. The
    // requests still call `end()` on their pending toggle, so the counters stay
    // accurate; the generation guards stop their results from landing (R11).
    historyGen++;
    scopeGen++;
    diffGen++;

    commits = [];
    scopeFilesList = [];
    currentDiffFile = null;
    selectedCommitSha = null;
    selectedFilePath = null;
    isUncommittedSelected = false;
    isRangeSelected = false;
    historyError = null;
    scopeError = null;
    diffError = null;
    historyFailureCount = 0;
    scopeFailureCount = 0;
    diffFailureCount = 0;
  }

  /** First failure of a surface gets a toast; every failure keeps the inline
   *  banner so a dismissed toast never hides the problem. */
  function reportHistoryError(e: unknown) {
    historyError = normalizeError(e);
    historyFailureCount += 1;
    if (historyFailureCount === 1) {
      addToast({
        message: historyError.message,
        kind: "error",
        details: historyError.stderr || undefined,
      });
    }
  }

  function reportScopeError(e: unknown) {
    scopeError = normalizeError(e);
    scopeFailureCount += 1;
    if (scopeFailureCount === 1) {
      addToast({
        message: scopeError.message,
        kind: "error",
        details: scopeError.stderr || undefined,
      });
    }
  }

  function reportDiffError(e: unknown) {
    diffError = normalizeError(e);
    diffFailureCount += 1;
    if (diffFailureCount === 1) {
      addToast({
        message: diffError.message,
        kind: "error",
        details: diffError.stderr || undefined,
      });
    }
  }

  function retryHistory() {
    historyError = null;
    historyFailureCount = 0;
    loadHistory(commits.length > 0);
  }

  function retryScope() {
    scopeError = null;
    scopeFailureCount = 0;
    loadScopeFiles(currentScope, true);
  }

  function retryDiff() {
    diffError = null;
    diffFailureCount = 0;
    if (selectedFilePath) selectFile(selectedFilePath);
  }

  function retryRepo() {
    if (repo) {
      refreshSingleStatus(repo.id).then(() => loadHistory());
    }
  }

  // Watch repo changes
  $effect(() => {
    const currentId = repo?.id || "";
    if (currentId !== lastLoadedRepoId) {
      lastLoadedRepoId = currentId;
      untrack(() => {
        // Drop the previous repository's content immediately so nothing stale
        // is shown while the new repo's history loads (ALPH-150 / R11).
        resetWorkspaceContent();
        if (currentId) {
          loadHistory();
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
      historyPending.cancel();
      scopePending.cancel();
      diffPending.cancel();
    };
  });

  async function loadHistory(preserveSelection = false) {
    if (!repo) return;
    const gen = ++historyGen;
    beginHistoryRequest();
    historyError = null;
    try {
      const log = await api.logPage(repo.id, 0, 200);
      if (gen !== historyGen) return;
      commits = log;
      historyFailureCount = 0;

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
      if (gen !== historyGen) return;
      console.error("Failed to load history:", e);
      reportHistoryError(e);
    } finally {
      endHistoryRequest();
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
    beginScopeRequest();
    scopeError = null;

    // A scope/selection change must never leave the previous scope's files or
    // diff on screen. Clear them up front; a preserved refresh keeps content
    // and is shown dimmed instead (ALPH-150 / R11).
    if (!preserveFile) {
      scopeFilesList = [];
      selectedFilePath = null;
      currentDiffFile = null;
      // Invalidate any diff request for the previous scope so its late result
      // cannot repopulate the cleared diff column.
      diffGen++;
      diffError = null;
    }

    try {
      const files = await api.scopeFiles(repo.id, scope, $settings.ignoreWhitespace);
      if (gen !== scopeGen) return;

      scopeFilesList = files;
      checkedPaths = files.map((f) => f.path);
      scopeFailureCount = 0;

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
      reportScopeError(e);
    } finally {
      endScopeRequest();
    }
  }

  async function selectFile(path: string, scope?: DiffScope) {
    if (!repo) return;
    const gen = ++diffGen;
    selectedFilePath = path;
    const activeScope = scope || currentScope;

    // Clear the previous file's diff so a new selection never shows stale
    // hunks; the skeleton covers the fetch (ALPH-150 / R11).
    currentDiffFile = null;
    diffError = null;
    beginDiffRequest();

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
      diffFailureCount = 0;
    } catch (e: any) {
      if (gen !== diffGen) return;
      console.error("Failed to load file diff:", e);
      reportDiffError(e);
    } finally {
      endDiffRequest();
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
    {:else if repoError}
      <div class="pane-error-state">
        <div class="pane-error-card">
          <Banner
            kind={repoError.kind === "git-missing" ||
            repoError.kind === "not-a-repo" ||
            repoError.kind === "path-missing"
              ? "error"
              : "warning"}
            message={repoError.message}
            error={repoError}
            retryLabel="Retry"
            onretry={retryRepo}
          />
        </div>
      </div>
    {:else if currentPane.tab === "files"}
      <FilesTab repoId={repo.id} rev={repo.branch || "HEAD"} />
    {:else}
      <!-- History Tab 3-Column Layout -->
      <div class="history-layout">
        <!-- Commits Column -->
        {#if !focusDiff}
          <div class="commits-col" style="width: {commitsColWidth}px;">
            {#if historyError}
              <div class="col-banner">
                <Banner
                  kind="error"
                  message={historyError.message}
                  error={historyError}
                  retryLabel="Retry"
                  onretry={retryHistory}
                />
              </div>
            {/if}

            <!-- Commits List -->
            <div
              class="commits-scroll"
              class:pending={historyLoading && commits.length > 0}
              aria-busy={historyLoading || historyInFlight}
            >
              {#if commits.length > 0}
                <UncommittedRow
                  dirtyCount={status?.dirtyCount || 0}
                  isSelected={isUncommittedSelected}
                  onselect={selectUncommitted}
                />

                {#each commits as commit (commit.sha)}
                  <CommitRow
                    {commit}
                    isSelected={selectedCommitSha === commit.sha}
                    isLocal={unpushedShas.has(commit.sha)}
                    onselect={() => selectCommit(commit.sha)}
                  />
                {/each}
              {:else if historyLoading}
                <Skeleton rows={6} height={60} shape="commit" label="Loading commits" />
              {:else if !historyInFlight && !historyError}
                <UncommittedRow
                  dirtyCount={status?.dirtyCount || 0}
                  isSelected={isUncommittedSelected}
                  onselect={selectUncommitted}
                />
                <EmptyState
                  icon="commit"
                  title="No commits yet"
                  description="Make your first commit from the terminal or the Uncommitted changes row."
                />
              {/if}
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
                loading={scopeLoading}
                pending={scopeInFlight}
                error={scopeError}
                emptyTitle={fileEmptyCopy.title}
                emptyDescription={fileEmptyCopy.description}
                onretry={retryScope}
                ontogglecommitdetails={() => (showCommitDetails = !showCommitDetails)}
                ontogglefocusdiff={() => (focusDiff = !focusDiff)}
                onselectfile={(path) => selectFile(path)}
                ontogglepath={toggleCheckedPath}
                ontoggleall={toggleAllChecked}
                oncommitrequest={() => (commitMode = true)}
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
                loading={diffLoading}
                pending={diffInFlight}
                error={diffError}
                onretry={retryDiff}
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
    transition: opacity 120ms ease;
  }
  .commits-scroll.pending {
    opacity: 0.6;
  }

  .col-banner {
    flex-shrink: 0;
  }

  .pane-error-state {
    display: flex;
    align-items: flex-start;
    justify-content: center;
    height: 100%;
    padding: 40px 24px;
  }
  .pane-error-card {
    width: 100%;
    max-width: 460px;
    background-color: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    overflow: hidden;
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
