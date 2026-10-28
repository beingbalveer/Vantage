<script lang="ts">
  import { onMount, onDestroy, tick, untrack } from "svelte";
  import { get } from "svelte/store";
  import { isFeatureEnabled } from "../../config/featureFlags.svelte";
  import type {
    RepoEntry,
    RepoStatus,
    CommitSummary,
    FileChange,
    DiffFile,
    DiffScope,
    TriageFlag,
  } from "../../platform/types";
  import * as api from "../../platform/api";
  import { RequestTracker } from "../../platform/request";
  import { settings } from "../../domain/stores/settings";
  import { panes, paneCount, focusedPaneIndex, focusPane, setPaneTab } from "../../domain/stores/panes";
  import { addToast, openDialog, activeDialog, paletteOpen, settingsOpen, shortcutsHelpOpen, addRepoDialogOpen, scanFolderDialogOpen } from "../../app/ui";
  import { composerRequest } from "../../domain/stores/composer";
  import { refreshSingleStatus, loadRepos, setRepoSelection, getRepoSelection, statuses } from "../../domain/stores/repos";
  import { tagsByRepo, loadTags, groupTagsByTarget } from "../../domain/stores/tags";
  import { navigationTarget, navigateTo } from "../../domain/stores/navigation";
  import {
    HISTORY_PAGE_SIZE,
    formatHistoryFooter,
    hasMoreHistory,
    isEditableTarget,
  } from "../../domain/history";
  import { findFlaggedCommitIndex, matchShortcut } from "../../platform/keys";
  import { registerHandler, markRepoReviewed, ownsPaneKeyboard } from "../../app/commands";
  import type { MenuItem } from "../../app/menu";
  import ContextMenu from "../common/ContextMenu.svelte";
  import UncommittedRow from "./UncommittedRow.svelte";
  import CommitRow from "./CommitRow.svelte";
  import CommitHeader from "./CommitHeader.svelte";
  import FileList from "./FileList.svelte";
  import CommitDialog from "./CommitDialog.svelte";
  import ConflictBanner from "./ConflictBanner.svelte";
  import DiffView from "../Diff/DiffView.svelte";
  import FilesTab from "./FilesTab.svelte";
  import Icon from "../common/Icon.svelte";
  import {
    viewportPaneWidths,
    savedPaneWidth,
    savedLeftColumnWidth,
    defaultLeftColumnWidth,
    persistLeftColumnWidth,
    LEFT_COLUMN_KEY,
  } from "../../app/layout";
  import {
    createHorizontalResizer,
    createHorizontalResizerKeyboard,
    createVerticalBottomResizer,
    createVerticalBottomResizerKeyboard,
  } from "../../app/resizer";

  interface Props {
    paneIndex: number;
    repo: RepoEntry | null;
    status: RepoStatus | null;
  }

  let { paneIndex, repo, status }: Props = $props();

  let currentPane = $derived($panes[paneIndex]);
  let isFocused = $derived($focusedPaneIndex === paneIndex);
  // The accent rail is only meaningful when more than one pane is on screen.
  let showFocusRail = $derived(isFocused && isFeatureEnabled("ALPH_144_MULTI_PANE") && $paneCount > 1);

  // Column Resizing & Focus Diff
  let focusDiff = $state(false);
  // Commit controls are revealed on demand so they don't add permanent UI the
  // reference does not show (RELAY-UI · UI-06).
  let commitMode = $state(false);
  let showCommitDetails = $state(false);
  // Bumped when the palette asks to amend the last commit, so the composer
  // opens already in amend mode (ALPH-154).
  let amendRequestToken = $state(0);
  // Per-pane: the commits column and the file tree share this inside the pane,
  // but other panes keep their own width.
  let leftColWidth = $state(savedLeftColumnWidth());
  let filesColWidth = $state(savedPaneWidth("vantage_files_col_width", viewportPaneWidths().files));

  const startCommitsResize = createHorizontalResizer({
    getWidth: () => leftColWidth,
    setWidth: (w) => (leftColWidth = w),
    minWidth: 180,
    maxWidth: 700,
    storageKey: LEFT_COLUMN_KEY,
  });

  const startFilesResize = createHorizontalResizer({
    getWidth: () => filesColWidth,
    setWidth: (w) => (filesColWidth = w),
    minWidth: 160,
    maxWidth: 600,
    storageKey: "vantage_files_col_width",
  });

  const nudgeCommitsResize = createHorizontalResizerKeyboard({
    getWidth: () => leftColWidth,
    setWidth: (w) => (leftColWidth = w),
    minWidth: 180,
    maxWidth: 700,
    storageKey: LEFT_COLUMN_KEY,
  });

  const nudgeFilesResize = createHorizontalResizerKeyboard({
    getWidth: () => filesColWidth,
    setWidth: (w) => (filesColWidth = w),
    minWidth: 160,
    maxWidth: 600,
    storageKey: "vantage_files_col_width",
  });

  let terminalHeight = $state(savedPaneWidth("vantage_terminal_height", 240));
  let terminalMaximized = $state(false);
  let restoreTerminalHeight = 240;

  function applyTerminalHeight(h: number) {
    terminalHeight = h;
    panes.update((list) => {
      if (list[paneIndex]) {
        list[paneIndex] = { ...list[paneIndex], terminalHeight: h };
      }
      return list;
    });
    try {
      localStorage.setItem("vantage_terminal_height", String(h));
    } catch {}
  }

  function toggleTerminalMaximize() {
    if (terminalMaximized) {
      applyTerminalHeight(restoreTerminalHeight);
      terminalMaximized = false;
      return;
    }
    // Leave a slice of the diff visible so the terminal never swallows the pane.
    const paneHeight = paneEl?.clientHeight ?? window.innerHeight;
    const max = Math.max(160, Math.floor(paneHeight * 0.7));
    if (terminalHeight >= max - 8) return;
    restoreTerminalHeight = terminalHeight;
    applyTerminalHeight(max);
    terminalMaximized = true;
  }

  const startTerminalResize = createVerticalBottomResizer({
    getHeight: () => terminalHeight,
    setHeight: (h) => {
      terminalMaximized = false;
      applyTerminalHeight(h);
    },
    minHeight: 80,
    maxHeight: 1200,
    storageKey: "vantage_terminal_height",
  });

  const nudgeTerminalResize = createVerticalBottomResizerKeyboard({
    getHeight: () => terminalHeight,
    setHeight: (h) => {
      terminalMaximized = false;
      applyTerminalHeight(h);
    },
    minHeight: 80,
    maxHeight: 1200,
    storageKey: "vantage_terminal_height",
  });

  // Commits & Selection
  let commits = $state<CommitSummary[]>([]);
  let selectedCommitSha = $state<string | null>(null);
  let isUncommittedSelected = $state(false);
  let isRangeSelected = $state(false);
  let currentScope = $state<DiffScope>({ type: "working" });

  // Triage flags, cached per commit SHA for the loaded history page (ALPH-147).
  let flagsBySha = $state<Record<string, TriageFlag[]>>({});
  let commitsScrollEl = $state<HTMLDivElement | null>(null);

  let flaggedCount = $derived.by(() => {
    let n = 0;
    for (const c of commits) {
      if ((flagsBySha[c.sha]?.length ?? 0) > 0) n++;
    }
    return n;
  });

  let selectedCommit = $derived(
    selectedCommitSha && !isUncommittedSelected
      ? commits.find((c) => c.sha === selectedCommitSha) || null
      : null
  );

  // History paging + filter state (ALPH-149). `historyTotal` is the true commit
  // count (or match count) from the backend, so the footer never claims a bogus
  // hard "200+" cap.
  let historyTotal = $state(0);
  let historyCapped = $state(false);
  let historyLoading = $state(false);
  let historyLoadingMore = $state(false);
  let historyFilter = $state("");
  let historyFilterApplied = $state("");
  let historyFilterInput = $state<HTMLInputElement | null>(null);
  let historySentinel = $state<HTMLElement | null>(null);
  let filterDebounce: ReturnType<typeof setTimeout> | undefined;

  let historyFilterActive = $derived(historyFilterApplied.trim().length > 0);
  let historyMeta = $derived({
    total: historyTotal,
    capped: historyCapped,
    loaded: commits.length,
  });
  let historyFooterLabel = $derived(formatHistoryFooter(historyMeta, historyFilterActive));

  // Tags for the current repo, indexed by the commit they point at (ALPH-157).
  let repoTags = $derived(repo ? $tagsByRepo[repo.id] || [] : []);
  let tagsByTarget = $derived(groupTagsByTarget(repoTags));

  $effect(() => {
    const currentId = repo?.id || "";
    if (currentId) {
      untrack(() => loadTags(currentId));
    }
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
  // Scope identity of the rows currently on screen; a change blanks the list so
  // the previous commit's files can never linger under a new selection.
  let lastLoadedScope = $state<DiffScope | null>(null);
  let scopeLoading = $state(false);
  // True when the currently shown diff was fetched with whole-file context.
  // Reset on repo/scope/file change so the expanded state never leaks.
  let diffIsFullContext = $state(false);

  /** File-list record for the selection (authoritative binary/status/rename). */
  let selectedFileChange = $derived(
    selectedFilePath
      ? (scopeFilesList.find((f) => f.path === selectedFilePath) ?? null)
      : null
  );

  // Registry navigation state (ALPH-161)
  let paneEl = $state<HTMLElement | null>(null);
  let selectionAnchorSha: string | null = null;
  let extendedShas = $state<Set<string>>(new Set());
  let flagsScanInFlight = false;

  // Latest-wins guards for async loads (R11). Tauri commands cannot be aborted
  // once sent, so each load checks that it is still the newest before applying.
  const historyRequests = new RequestTracker();
  const scopeRequests = new RequestTracker();
  const diffRequests = new RequestTracker();

  onDestroy(() => {
    historyRequests.dispose();
    scopeRequests.dispose();
    diffRequests.dispose();
  });

  let lastLoadedRepoId = "";

  // Xterm is only needed once a terminal is opened. Keeping it in a dynamic
  // chunk removes ~300 kB of JavaScript from the startup path.
  let terminalModule = $state<typeof import("../Terminal/TerminalDrawer.svelte") | null>(null);
  let terminalLoading = $state(false);
  let terminalLoadError = $state<string | null>(null);

  function loadTerminal() {
    if (terminalModule || terminalLoading) return;
    terminalLoading = true;
    terminalLoadError = null;
    import("../Terminal/TerminalDrawer.svelte")
      .then((mod) => {
        terminalModule = mod;
      })
      .catch((e: any) => {
        console.error("Failed to load terminal:", e);
        terminalLoadError = e?.message || "Failed to load terminal";
      })
      .finally(() => {
        terminalLoading = false;
      });
  }

  $effect(() => {
    if (currentPane?.terminalOpen && repo) {
      // untrack the loader state so the effect only re-runs when a terminal is
      // opened (and not again after a failed load, which would retry forever).
      untrack(() => loadTerminal());
    }
  });

  // Watch repo changes
  $effect(() => {
    const currentId = repo?.id || "";
    if (currentId !== lastLoadedRepoId) {
      lastLoadedRepoId = currentId;
      // A repo switch must not let the previous repo's in-flight loads write
      // into the new repo's panes.
      historyRequests.cancel();
      scopeRequests.cancel();
      diffRequests.cancel();
      historyLoading = false;
      historyLoadingMore = false;
      flagsBySha = {};
      untrack(() => {
        diffIsFullContext = false;
        if (currentId) {
          historyFilter = "";
          historyFilterApplied = "";
          historyTotal = 0;
          historyCapped = false;
          loadHistory();
        } else {
          commits = [];
          scopeFilesList = [];
          currentDiffFile = null;
          selectedCommitSha = null;
          selectedFilePath = null;
          isUncommittedSelected = false;
          historyTotal = 0;
          historyCapped = false;
          isRangeSelected = false;
        }
        lastLoadedScope = null;
        scopeLoading = false;
      });
    }
  });

  // Listen to navigation target (R17)
  $effect(() => {
    if ($navigationTarget && repo) {
      if (!$navigationTarget.repoId || $navigationTarget.repoId === repo.id) {
        // Selections kick off async loads that touch reactive state; keep those
        // writes from becoming dependencies of this effect.
        untrack(() => {
          if ($navigationTarget?.tab && currentPane) {
            $panes[paneIndex].tab = $navigationTarget.tab;
          }
          if ($navigationTarget?.commitSha) {
            selectCommit($navigationTarget.commitSha);
          }
          if ($navigationTarget?.path) {
            selectedFilePath = $navigationTarget.path;
            selectFile($navigationTarget.path);
          }
        });
      }
    }
  });

  // Amend request from the command palette (ALPH-154): open the composer on the
  // uncommitted row in amend mode for the targeted repo.
  $effect(() => {
    const req = $composerRequest;
    if (!req) return;
    if (repo && req.repoId === repo.id) {
      untrack(() => {
        isUncommittedSelected = true;
        selectedCommitSha = null;
        currentScope = { type: "working" };
        commitMode = true;
        showCommitDetails = false;
        amendRequestToken += 1;
        loadScopeFiles({ type: "working" });
      });
    }
    composerRequest.set(null);
  });

  // Drop a consumed amend request when the composer closes so reopening it does
  // not unexpectedly start amending the last commit.
  $effect(() => {
    if (!(isUncommittedSelected && commitMode)) {
      amendRequestToken = 0;
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
    window.addEventListener("keydown", handleFlagKey);
    (async () => {
      try {
        const h = await api.onHeadChanged((payload: { repoId: string; sha: string }) => {
          if (!active || !repo || payload.repoId !== repo.id) return;
          loadHistory(true);
        });
        unlistenHead = h;

        const s = await api.onRepoStatus((st: RepoStatus) => {
          if (!active || !repo || st.repoId !== repo.id) return;
          const lastSel = getRepoSelection(repo.id);
          if (st.dirtyCount > 0 && (!lastSel || lastSel.type === "uncommitted") && !isUncommittedSelected) {
            selectUncommitted(false);
          } else if (isUncommittedSelected) {
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
      clearTimeout(filterDebounce);
      window.removeEventListener("keydown", handleFlagKey);
      unlistenHead?.();
      unlistenStatus?.();
    };
  });

  async function loadHistory(preserveSelection = false) {
    if (!repo) return;
    const request = historyRequests.begin();
    historyLoading = true;
    historyLoadingMore = false;
    const query = historyFilterApplied;
    try {
      const page = await api.historyPage(repo.id, 0, HISTORY_PAGE_SIZE, query || undefined);
      if (!request.isCurrent()) return;
      commits = page.commits;
      historyTotal = page.total;
      historyCapped = page.capped;
      historyLoading = false;

      // Batch-request triage flags for the loaded page. Failures are ignored
      // per-SHA (the backend omits SHAs it could not evaluate) so the list
      // never breaks because flagging failed (ALPH-147).
      void loadFlags(repo.id, page.commits.map((c) => c.sha));

      if (!preserveSelection) {
        if (query) {
          // Filtering: land on the newest match, or clear when nothing matches.
          if (page.commits.length > 0) {
            selectCommit(page.commits[0].sha, false);
          } else {
            clearSelection();
          }
        } else {
          const lastSel = repo ? getRepoSelection(repo.id) : undefined;
          const statusDirty =
            Boolean(status?.dirtyCount && status.dirtyCount > 0) ||
            Boolean(repo && get(statuses)[repo.id]?.dirtyCount && get(statuses)[repo.id].dirtyCount > 0);

          if (statusDirty) {
            selectUncommitted(false);
          } else if (lastSel) {
            if (lastSel.type === "uncommitted") {
              selectUncommitted(false);
            } else if (
              lastSel.type === "commit" &&
              page.commits.some((c) => c.sha === lastSel.sha)
            ) {
              selectCommit(lastSel.sha, false);
            } else {
              selectUncommitted(false);
            }
          } else {
            // Default when switching repo: uncommitted changes (never auto-select first commit)
            selectUncommitted(false);
          }
        }
      } else {
        // If current selection is commit, reload files; if uncommitted, reload files
        if (isUncommittedSelected) {
          loadScopeFiles({ type: "working" }, true);
        } else if (selectedCommitSha) {
          loadScopeFiles({ type: "commit", sha: selectedCommitSha }, true);
        }
      }
    } catch (e) {
      if (!request.isCurrent()) return;
      historyLoading = false;
      console.error("Failed to load history:", e);
    }
  }

  // Append the next page without disturbing selection, scroll or the open diff.
  async function loadMoreHistory() {
    if (!repo || historyLoading || historyLoadingMore) return;
    if (!hasMoreHistory(historyMeta)) return;
    const request = historyRequests.begin();
    const skip = commits.length;
    historyLoadingMore = true;
    try {
      const page = await api.historyPage(
        repo.id,
        skip,
        HISTORY_PAGE_SIZE,
        historyFilterApplied || undefined
      );
      if (!request.isCurrent()) return;
      const seen = new Set(commits.map((c) => c.sha));
      const newCommits = page.commits.filter((c) => !seen.has(c.sha));
      commits = [...commits, ...newCommits];
      historyTotal = page.total;
      historyCapped = page.capped;
      void loadFlags(repo.id, newCommits.map((c) => c.sha));
    } catch (e: any) {
      if (!request.isCurrent()) return;
      console.error("Failed to load more history:", e);
      addToast({ message: e.message || "Failed to load more commits", kind: "error" });
    } finally {
      if (request.isCurrent()) historyLoadingMore = false;
    }
  }

  function clearSelection() {
    selectedCommitSha = null;
    isUncommittedSelected = false;
    scopeFilesList = [];
    currentDiffFile = null;
    selectedFilePath = null;
    checkedPaths = [];
  }

  function applyFilter(value: string) {
    const next = value.trim();
    if (next === historyFilterApplied) return;
    historyFilterApplied = next;
    loadHistory(false);
  }

  function onFilterInput(e: Event) {
    historyFilter = (e.target as HTMLInputElement).value;
    clearTimeout(filterDebounce);
    filterDebounce = setTimeout(() => applyFilter(historyFilter), 220);
  }

  function onFilterKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      clearFilter();
      (e.target as HTMLInputElement)?.blur();
    } else if (e.key === "Enter") {
      e.preventDefault();
      clearTimeout(filterDebounce);
      applyFilter(historyFilter);
    }
  }

  function clearFilter() {
    clearTimeout(filterDebounce);
    historyFilter = "";
    if (historyFilterApplied !== "") {
      historyFilterApplied = "";
      loadHistory(false);
    }
    historyFilterInput?.focus();
  }

  // `/` focuses the history filter for the focused pane (F3) without hijacking
  // typing in inputs, textareas or the embedded terminal.
  function handlePaneKeydown(e: KeyboardEvent) {
    if (!isFocused || !repo || currentPane?.tab !== "history") return;
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    if (e.key !== "/") return;
    if (isEditableTarget(e.target)) return;
    e.preventDefault();
    historyFilterInput?.focus();
    historyFilterInput?.select();
  }

  // Auto-load the next page as the sentinel nears the viewport (F3 infinite scroll).
  $effect(() => {
    const el = historySentinel;
    if (!el || !hasMoreHistory(historyMeta)) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          loadMoreHistory();
        }
      },
      { rootMargin: "160px" }
    );
    observer.observe(el);
    return () => observer.disconnect();
  });

  // Fetch triage flags for a set of SHAs and merge them into the cache.
  // Per-SHA failures are silent by design: a flagged SHA simply stays blank.
  async function loadFlags(repoId: string, shas: string[]) {
    if (shas.length === 0) return;
    try {
      const result = await api.commitFlags(repoId, shas);
      // Flags are per-SHA, so a late response is still valid for this repo even
      // if the history filter changed meanwhile. Only a repo switch invalidates
      // it (the cache was cleared and the keys belong to another repo).
      if (repo?.id !== repoId) return;
      flagsBySha = { ...flagsBySha, ...result };
    } catch (e) {
      console.warn("Failed to load triage flags:", e);
    }
  }

  // `{` / `}` jump to the previous/next flagged commit in the loaded history.
  async function jumpToFlag(direction: "next" | "prev") {
    if (commits.length === 0) return;

    const tagged = commits.map((c) => ({ flags: flagsBySha[c.sha] ?? [] }));
    if (!tagged.some((c) => c.flags.length > 0)) {
      addToast({ message: "No flagged commits in this history", kind: "info" });
      return;
    }

    const currentIndex = selectedCommitSha
      ? commits.findIndex((c) => c.sha === selectedCommitSha)
      : -1;
    const targetIndex = findFlaggedCommitIndex(tagged, currentIndex, direction);

    if (targetIndex < 0 || targetIndex === currentIndex) {
      addToast({
        message:
          direction === "next"
            ? "No later flagged commits"
            : "No earlier flagged commits",
        kind: "info",
      });
      return;
    }

    const target = commits[targetIndex];
    if (!target) return;
    selectCommit(target.sha);
    await tick();
    const row = commitsScrollEl?.querySelector(`[data-sha="${target.sha}"]`);
    if (row instanceof HTMLElement) row.scrollIntoView({ block: "nearest" });
  }

  function handleFlagKey(e: KeyboardEvent) {
    if (!isFocused || !repo || currentPane?.tab !== "history") return;
    if (
      $activeDialog ||
      $paletteOpen ||
      $settingsOpen ||
      $shortcutsHelpOpen ||
      $addRepoDialogOpen ||
      $scanFolderDialogOpen
    ) {
      return;
    }
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    const target = e.target as HTMLElement | null;
    if (
      target &&
      (target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        target.isContentEditable)
    ) {
      return;
    }
    if (matchShortcut(e, "{")) {
      e.preventDefault();
      void jumpToFlag("prev");
    } else if (matchShortcut(e, "}")) {
      e.preventDefault();
      void jumpToFlag("next");
    }
  }

  function clearExtendedSelection() {
    if (extendedShas.size > 0) extendedShas = new Set();
    selectionAnchorSha = null;
  }
  function selectUncommitted(record = true) {
    if (record && repo) {
      setRepoSelection(repo.id, { type: "uncommitted" });
    }
    clearExtendedSelection();
    isUncommittedSelected = true;
    selectedCommitSha = null;
    currentScope = { type: "working" };
    commitMode = false;
    showCommitDetails = false;
    loadScopeFiles(currentScope);
  }

  function selectCommit(sha: string, record = true) {
    if (record && repo) {
      setRepoSelection(repo.id, { type: "commit", sha });
    }
    clearExtendedSelection();
    selectedCommitSha = sha;
    isUncommittedSelected = false;
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

  /** Two scopes describe the same file list when their kind and range match. */
  function sameScope(a: DiffScope, b: DiffScope | null): boolean {
    if (!b || a.type !== b.type) return false;
    if (a.type === "commit" && b.type === "commit") return a.sha === b.sha;
    if (a.type === "range" && b.type === "range") {
      return a.from === b.from && a.to === b.to;
    }
    return true;
  }

  /** Stable identity for the current scope, used to reset list scroll. */
  function scopeKeyOf(scope: DiffScope): string {
    if (scope.type === "commit") return `commit:${scope.sha}`;
    if (scope.type === "range") return `range:${scope.from}..${scope.to}`;
    return "working";
  }

  async function loadScopeFiles(scope: DiffScope, preserveFile = false) {    if (!repo) return;
    const request = scopeRequests.begin();
    // The old scope's diff is no longer relevant while the file list loads.
    diffRequests.cancel();

    // Switching commits must never leave the previous scope's files on screen
    // under the new commit's header. Same-scope reloads (refresh, staging) keep
    // the rows in place so the list does not flicker.
    const scopeChanged = !sameScope(scope, lastLoadedScope);
    if (scopeChanged) {
      lastLoadedScope = scope;
      scopeFilesList = [];
      checkedPaths = [];
      selectedFilePath = null;
      currentDiffFile = null;
      scopeLoading = true;
    }

    try {
      const files = await api.scopeFiles(repo.id, scope, $settings.ignoreWhitespace);
      if (!request.isCurrent()) return;

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
      if (!request.isCurrent()) return;
      console.error("Failed to load scope files:", e);
      addToast({ message: e.message || "Failed to load files for commit", kind: "error" });
    } finally {
      if (request.isCurrent()) scopeLoading = false;
    }
  }

  async function selectFile(
    path: string,
    scope?: DiffScope,
    opts: { force?: boolean; fullContext?: boolean } = {}
  ) {
    if (!repo) return;
    const force = opts.force ?? false;
    const fullContext = opts.fullContext ?? false;
    const request = diffRequests.begin();
    selectedFilePath = path;
    // Reflect the intended state immediately so a scope/file switch never shows
    // a stale "Whole file" indicator while the new request is in flight.
    diffIsFullContext = fullContext;
    const activeScope = scope || currentScope;

    try {
      const diff = await api.scopeFileDiff(
        repo.id,
        activeScope,
        path,
        $settings.contextLines,
        $settings.ignoreWhitespace,
        force,
        fullContext
      );
      if (!request.isCurrent()) return;
      currentDiffFile = diff;
    } catch (e: any) {
      if (!request.isCurrent()) return;
      console.error("Failed to load file diff:", e);
      addToast({ message: e.message || "Failed to load diff", kind: "error" });
    }
  }

  /** Re-request the current file's diff with all context (whole file). */
  function expandWholeFile(path?: string) {
    const target = path || selectedFilePath;
    if (target) selectFile(target, undefined, { fullContext: true });
  }

  /** Re-request the current file's diff with the configured context. */
  function collapseContext(path?: string) {
    const target = path || selectedFilePath;
    if (target) selectFile(target, undefined, { fullContext: false });
  }

  async function handlePartialRefresh() {
    if (!repo) return;
    await loadScopeFiles(currentScope, true);
    await refreshSingleStatus(repo.id);
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

  // After a merge/rebase/cherry-pick/revert is continued or aborted the working
  // tree and history both change, so refresh the pane, this repo's status, and
  // the sidebar list (ALPH-156).
  async function handleOperationResolved() {
    if (!repo) return;
    // Refresh the repo status first so the banner disappears (or re-renders with
    // the remaining conflicts) and the sidebar reflects the new HEAD.
    await refreshSingleStatus(repo.id);
    loadRepos();
    // Preserve the current selection while reloading history and the selected
    // scope's files/diff, which the continue/abort just changed.
    loadHistory(true);
  }

  // ---- Registry-driven navigation (ALPH-161) -------------------------------

  function scrollCommitIntoView(sha: string) {
    queueMicrotask(() => {
      paneEl
        ?.querySelector<HTMLElement>(`[data-sha="${sha}"]`)
        ?.scrollIntoView({ block: "nearest" });
    });
  }

  function moveList(delta: 1 | -1, extend = false): boolean {
    if (!ownsPaneKeyboard(paneIndex) || focusDiff) return false;

    // j/k follow the focused list: the commit list or the changed-file list.
    const active = document.activeElement as HTMLElement | null;
    if (!extend && active?.closest(".files-col")) {
      return moveFile(delta);
    }

    if (commits.length === 0) return false;

    const current = isUncommittedSelected
      ? -1
      : selectedCommitSha
        ? commits.findIndex((c) => c.sha === selectedCommitSha)
        : -1;

    let next = current + delta;
    next = Math.max(-1, Math.min(commits.length - 1, next));

    if (!extend) {
      if (next === -1) {
        selectUncommitted();
      } else {
        selectCommit(commits[next].sha);
        scrollCommitIntoView(commits[next].sha);
      }
      return true;
    }

    // Extending cannot include the uncommitted row.
    if (next === -1) next = 0;
    const anchorSha =
      selectionAnchorSha ||
      selectedCommitSha ||
      commits[0]?.sha;
    if (!anchorSha) return false;

    const anchorIdx = commits.findIndex((c) => c.sha === anchorSha);
    if (anchorIdx < 0) return false;

    selectionAnchorSha = anchorSha;
    const lo = Math.min(anchorIdx, next);
    const hi = Math.max(anchorIdx, next);
    extendedShas = new Set(commits.slice(lo, hi + 1).map((c) => c.sha));

    const sha = commits[next].sha;
    selectedCommitSha = sha;
    isUncommittedSelected = false;
    isRangeSelected = false;
    currentScope = { type: "commit", sha };
    commitMode = false;
    showCommitDetails = false;
    loadScopeFiles(currentScope);
    scrollCommitIntoView(sha);
    return true;
  }

  async function jumpToFlagged(direction: "next" | "prev"): Promise<boolean> {
    if (!ownsPaneKeyboard(paneIndex) || !repo || commits.length === 0) return false;

    // `log_page` returns empty flag arrays; compute triage flags on demand the
    // first time the user asks to jump. (Rendering flags is owned elsewhere.)
    if (!commits.some((c) => c.flags && c.flags.length > 0) && !flagsScanInFlight) {
      flagsScanInFlight = true;
      addToast({ message: "Checking triage flags…", kind: "info" });
      try {
        const map = await api.commitFlags(
          repo.id,
          commits.map((c) => c.sha)
        );
        commits = commits.map((c) => ({ ...c, flags: map[c.sha] ?? c.flags ?? [] }));
      } catch (e: any) {
        addToast({ message: e.message || "Failed to load triage flags", kind: "error" });
        return true;
      } finally {
        flagsScanInFlight = false;
      }
    }

    const current = commits.findIndex((c) => c.sha === selectedCommitSha);
    const target = findFlaggedCommitIndex(commits, current, direction);
    if (target < 0 || target === current) {
      addToast({
        message: direction === "next" ? "No next flagged commit" : "No previous flagged commit",
        kind: "info",
      });
      return true;
    }

    selectCommit(commits[target].sha);
    scrollCommitIntoView(commits[target].sha);
    return true;
  }

  async function markReviewedHere(): Promise<boolean> {
    if (!ownsPaneKeyboard(paneIndex) || !repo) return false;
    if (extendedShas.size > 1) {
      // Newest commit in the selected range (history is newest-first).
      const newest = commits.find((c) => extendedShas.has(c.sha));
      if (!newest) return false;
      await markRepoReviewed(repo.id, newest.sha);
      clearExtendedSelection();
      return true;
    }
    await markRepoReviewed(repo.id);
    return true;
  }

  async function copySelectedSha(): Promise<boolean> {
    if (!ownsPaneKeyboard(paneIndex) || !repo) return false;
    const active = document.activeElement as HTMLElement | null;
    const focusedSha = active?.closest<HTMLElement>("[data-sha]")?.dataset.sha;
    const commit = commits.find((c) => c.sha === (focusedSha || selectedCommitSha));
    if (!commit) {
      addToast({ message: "Select a commit to copy its SHA", kind: "info" });
      return true;
    }
    try {
      await navigator.clipboard.writeText(commit.sha);
      addToast({ message: `Copied ${commit.short}`, kind: "info" });
    } catch (e: any) {
      addToast({ message: e.message || "Failed to copy SHA", kind: "error" });
    }
    return true;
  }

  function moveFile(delta: 1 | -1): boolean {
    if (!ownsPaneKeyboard(paneIndex) || scopeFilesList.length === 0) return false;
    const current = selectedFilePath
      ? scopeFilesList.findIndex((f) => f.path === selectedFilePath)
      : -1;
    const next = Math.max(
      0,
      Math.min(scopeFilesList.length - 1, (current < 0 ? 0 : current) + delta)
    );
    const path = scopeFilesList[next]?.path;
    if (!path) return false;
    selectFile(path);
    queueMicrotask(() => {
      paneEl
        ?.querySelector<HTMLElement>(`[data-file-path="${CSS.escape(path)}"]`)
        ?.scrollIntoView({ block: "nearest" });
    });
    return true;
  }

  function focusNextRegion(): boolean {
    if (!ownsPaneKeyboard(paneIndex) || !paneEl) return false;
    const active = document.activeElement as HTMLElement | null;

    const commitRow = active?.closest<HTMLElement>(".commit-item, .change-item");
    if (commitRow && paneEl.contains(commitRow)) {
      const sha = commitRow.dataset.sha;
      if (sha && sha !== selectedCommitSha) {
        selectCommit(sha);
      }
      const row =
        paneEl.querySelector<HTMLElement>(".file-row.selected") ||
        paneEl.querySelector<HTMLElement>(".file-row");
      if (row) {
        row.focus();
        return true;
      }
      return false;
    }

    const fileRow = active?.closest<HTMLElement>(".file-row");
    if (fileRow && paneEl.contains(fileRow)) {
      const diff =
        paneEl.querySelector<HTMLElement>(".diff-body") ||
        paneEl.querySelector<HTMLElement>(".diff-col");
      if (diff) {
        diff.focus();
        return true;
      }
      return false;
    }

    // Enter on an unrelated control (toolbar button, menu trigger) must keep
    // its native activation. Only an unfocused surface advances focus.
    const isSurface =
      active === document.body ||
      active === document.documentElement ||
      active?.classList.contains("diff-body") ||
      active?.classList.contains("diff-col") ||
      active?.classList.contains("pane-container");
    if (!isSurface) return false;

    const first =
      paneEl.querySelector<HTMLElement>(".commit-item.active") ||
      paneEl.querySelector<HTMLElement>(".change-item.active") ||
      paneEl.querySelector<HTMLElement>("[data-sha]") ||
      paneEl.querySelector<HTMLElement>("[data-uncommitted]");
    if (first) {
      first.focus();
      return true;
    }
    return false;
  }

  $effect(() => {
    const unregister = [
      registerHandler("list_down", () => moveList(1), "history"),
      registerHandler("list_up", () => moveList(-1), "history"),
      registerHandler("extend_down", () => moveList(1, true), "history"),
      registerHandler("extend_up", () => moveList(-1, true), "history"),
      registerHandler("next_flag", () => jumpToFlagged("next"), "history"),
      registerHandler("prev_flag", () => jumpToFlagged("prev"), "history"),
      registerHandler("mark_reviewed", markReviewedHere, "history"),
      registerHandler("copy_sha", copySelectedSha, "history"),
      registerHandler("enter_focus", focusNextRegion, "history"),
      registerHandler("next_file", () => moveFile(1), "diff"),
      registerHandler("prev_file", () => moveFile(-1), "diff"),
    ];
    return () => unregister.forEach((off) => off());
  });
</script>

<svelte:window onkeydown={handlePaneKeydown} />

<div
  class="pane-container"
  class:focused={showFocusRail}
  bind:this={paneEl}
  onpointerdown={() => focusPane(paneIndex)}
  onfocusin={() => focusPane(paneIndex)}
  role="region"
  aria-label="Repository Pane {paneIndex + 1}"
  aria-current={isFocused ? "true" : undefined}
>
  <!-- Pane Body -->
  <div class="pane-content">
    {#if repo}
      <ConflictBanner
        repoId={repo.id}
        {status}
        onresolved={handleOperationResolved}
      />
    {/if}

    {#if !repo}
      <div class="empty-pane">
        <span class="text-strong">No repository selected</span>
        <span class="text-sm text-muted">Select a repository from the sidebar or press Mod+O to add one.</span>
      </div>
    {:else if currentPane.tab === "files"}
      <FilesTab
        repoId={repo.id}
        rev={repo.branch || "HEAD"}
        {repo}
        {status}
        {paneIndex}
        leftWidth={leftColWidth}
        onleftwidthchange={(w) => (leftColWidth = w)}
      />
    {:else}
      <!-- History Tab 3-Column Layout -->
      <div class="history-layout">
        <!-- Commits Column -->
        {#if !focusDiff}
          <div class="commits-col" style="width: {leftColWidth}px;">
            <!-- View tabs fill the commits column; branch lives in the app header -->
            <div class="commits-header">
              <div class="pane-tabs" role="tablist" aria-label="Pane view">
                <button
                  type="button"
                  role="tab"
                  aria-selected="true"
                  class="pane-tab active"
                  title="Commits & History"
                >
                  <Icon name="commit" size={14} />
                  <span>Commits</span>
                </button>
                <button
                  type="button"
                  role="tab"
                  aria-selected="false"
                  class="pane-tab"
                  title="Project Files Explorer"
                  onclick={() => setPaneTab(paneIndex, "files")}
                >
                  <Icon name="folder" size={14} />
                  <span>Files</span>
                </button>
              </div>
            </div>

            <!-- History filter (F3): matches message, author or touched path -->
            <div class="history-filter">
              <span class="filter-icon" aria-hidden="true">
                <Icon name="search" size={14} />
              </span>
              <input
                class="filter-input"
                type="text"
                placeholder="Filter commits"
                aria-label="Filter commits by message, author, or path"
                bind:this={historyFilterInput}
                value={historyFilter}
                oninput={onFilterInput}
                onkeydown={onFilterKeydown}
              />
              {#if historyFilter}
                <button
                  class="filter-clear"
                  type="button"
                  aria-label="Clear history filter"
                  onclick={clearFilter}
                >
                  <Icon name="close" size={13} />
                </button>
              {/if}
            </div>

            <!-- Commits List -->
            <div class="commits-scroll" bind:this={commitsScrollEl}>
              {#if !historyFilterActive}
                <UncommittedRow
                  dirtyCount={status?.dirtyCount || 0}
                  isSelected={isUncommittedSelected}
                  onselect={selectUncommitted}
                />
              {/if}

              {#if historyLoading && commits.length === 0}
                <div class="history-status text-sm text-muted">Loading history…</div>
              {:else if commits.length === 0 && historyFilterActive}
                <div class="history-empty">
                  <span class="text-sm text-muted">
                    No commits match “{historyFilterApplied}”.
                  </span>
                  <button class="empty-clear" type="button" onclick={clearFilter}>
                    Clear filter
                  </button>
                </div>
              {:else if commits.length === 0}
                <div class="empty-commits text-sm text-muted">No commits yet</div>
              {/if}

              {#each commits as commit (commit.sha)}
                <CommitRow
                  {commit}
                  isSelected={selectedCommitSha === commit.sha || extendedShas.has(commit.sha)}
                  isMenuOpen={isCommitMenuOpen(commit.sha)}
                  onselect={() => selectCommit(commit.sha)}
                  onmenu={(pos: { x: number; y: number }) => openCommitMenu(commit, pos)}
                />
              {/each}

              {#if hasMoreHistory(historyMeta)}
                <div class="load-more-row" bind:this={historySentinel}>
                  <button
                    class="load-more"
                    type="button"
                    disabled={historyLoadingMore}
                    onclick={loadMoreHistory}
                  >
                    {historyLoadingMore ? "Loading…" : "Load more"}
                  </button>
                </div>
              {/if}
            </div>

            {#if historyFooterLabel}
              <div class="history-footer">
                {historyFooterLabel}{#if flaggedCount > 0}<span class="history-flagged"> · {flaggedCount} flagged</span>{/if}
              </div>
            {/if}
          </div>

          <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
          <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
          <div
            class="split-handle"
            role="separator"
            aria-orientation="vertical"
            aria-label="Resize commit list"
            aria-valuemin={180}
            aria-valuemax={700}
            aria-valuenow={Math.round(leftColWidth)}
            tabindex="0"
            title="Drag to resize commits column (Double-click to reset, Arrow keys to nudge)"
            onpointerdown={startCommitsResize}
            onkeydown={nudgeCommitsResize}
            ondblclick={() => {
              leftColWidth = defaultLeftColumnWidth();
              persistLeftColumnWidth(leftColWidth);
            }}
          ></div>
        {/if}

        <!-- Detail Layout: Files and Diff columns -->
        <div class="detail-layout">
          {#if selectedCommit && !isUncommittedSelected && showCommitDetails}
            <CommitHeader
              commit={selectedCommit}
              repoId={repo.id}
              tags={tagsByTarget.get(selectedCommit.sha) || []}
            />
          {/if}

          <div class="detail-split">
            <!-- Changed Files Column -->
            <div class="files-col" style="width: {focusDiff ? '300px' : filesColWidth + 'px'};">
              <FileList
                files={scopeFilesList}
                selectedFile={selectedFilePath}
                isUncommitted={isUncommittedSelected}
                loading={scopeLoading}
                scopeKey={scopeKeyOf(currentScope)}
                {checkedPaths}
                showCommitControls={isUncommittedSelected && commitMode}
                {focusDiff}
                hasCommitDetails={!!selectedCommit && !isUncommittedSelected}
                {showCommitDetails}
                menuOpenPath={openMenuFilePath}
                expandedFile={diffIsFullContext ? selectedFilePath : null}
                onexpandwholefile={expandWholeFile}
                oncollapsecontext={collapseContext}
                ontogglecommitdetails={() => (showCommitDetails = !showCommitDetails)}
                ontogglefocusdiff={() => (focusDiff = !focusDiff)}
                onselectfile={(path: string) => selectFile(path)}
                ontogglepath={toggleCheckedPath}
                ontoggleall={toggleAllChecked}
                oncommitrequest={() => (commitMode = true)}
                ondiscardrequest={discardSelected}
                onfilemenu={openFileMenu}
                onbrowseall={() => ($panes[paneIndex].tab = "files")}
              />
            </div>

            <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
            <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
            <div
              class="split-handle"
              role="separator"
              aria-orientation="vertical"
              aria-label="Resize file list"
              aria-valuemin={160}
              aria-valuemax={600}
              aria-valuenow={Math.round(filesColWidth)}
              tabindex="0"
              title="Drag to resize files column (Double-click to reset, Arrow keys to nudge)"
              onpointerdown={startFilesResize}
              onkeydown={nudgeFilesResize}
              ondblclick={() => {
                filesColWidth = viewportPaneWidths().files;
                try { localStorage.setItem("vantage_files_col_width", String(filesColWidth)); } catch {}
              }}
            ></div>

            <div class="diff-col" tabindex="-1">
              <DiffView
                diffFile={currentDiffFile}
                ignoreWhitespace={$settings.ignoreWhitespace}
                repoId={repo.id}
                rev={selectedCommitSha || repo.branch || "HEAD"}
                scope={currentScope}
                fileChange={selectedFileChange}
                fullContext={diffIsFullContext}
                onexpandwholefile={() => expandWholeFile()}
                oncollapsecontext={() => collapseContext()}
                scopeType={currentScope.type}
                onrefresh={handlePartialRefresh}
                {paneIndex}
                onviewfile={(path: string) => {
                  navigateTo({
                    repoId: repo.id,
                    tab: "files",
                    path,
                  });
                }}
                onloadforce={() => {
                  if (selectedFilePath) {
                    selectFile(selectedFilePath, undefined, {
                      force: true,
                      fullContext: diffIsFullContext,
                    });
                  }
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
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      class="terminal-sash"
      role="separator"
      aria-orientation="horizontal"
      aria-label="Resize terminal"
      tabindex="0"
      title="Drag to resize terminal (Double-click to reset)"
      onpointerdown={startTerminalResize}
      onkeydown={nudgeTerminalResize}
      ondblclick={() => {
        terminalMaximized = false;
        applyTerminalHeight(240);
      }}
    ></div>

    {#if terminalModule}
      {#key repo.id}
        {@const TerminalDrawer = terminalModule.default}
        <TerminalDrawer
          repoId={repo.id}
          repoName={repo.name}
          repoPath={repo.path}
          height={terminalHeight}
          maximized={terminalMaximized}
          onmaximize={toggleTerminalMaximize}
          onclose={() => {
            panes.update((list) => {
              if (list[paneIndex]) {
                list[paneIndex] = { ...list[paneIndex], terminalOpen: false };
              }
              return list;
            });
          }}
        />
      {/key}
    {:else}
      <div class="terminal-loading" style="height: {terminalHeight}px">
        <span class="text-xs text-muted">
          {terminalLoadError ?? "Starting terminal…"}
        </span>
        {#if terminalLoadError}
          <button
            type="button"
            class="terminal-retry"
            onclick={loadTerminal}
            title="Retry loading terminal"
            aria-label="Retry loading terminal"
          >
            <Icon name="refresh" size={13} />
          </button>
        {/if}
      </div>
    {/if}
  {/if}

  <!-- Centered commit dialog. Mounted only while commit mode is active so a
       closed dialog never keeps a stale draft or GitHub snapshot around. -->
  {#if isUncommittedSelected && commitMode && repo}
    <CommitDialog
      repoId={repo.id}
      repoName={repo.name}
      branch={status?.branch || repo.branch || "main"}
      selectedPaths={checkedPaths}
      conflictedPaths={status?.conflictedPaths ?? []}
      amendToken={amendRequestToken}
      onclose={() => (commitMode = false)}
      oncommit={(preserve = false) => {
        refreshSingleStatus(repo.id).then(() => {
          loadHistory(preserve);
        });
      }}
    />
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

  /* Accent rail marks the focused pane while 2-3 panes are visible. */
  .pane-container.focused::before {
    content: "";
    position: absolute;
    top: 0;
    bottom: 0;
    left: 0;
    width: 3px;
    background-color: var(--accent);
    z-index: 10;
    pointer-events: none;
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
    flex: 1 1 auto;
    min-height: 0;
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

  /* Commits column header: the view tabs fill the column width. */
  .commits-header {
    flex-shrink: 0;
    height: 32px;
    display: flex;
    align-items: stretch;
    padding: 0;
    background-color: var(--bg-subtle);
    border-bottom: 1px solid var(--border);
  }

  .commits-scroll {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
  }

  /* History filter (F3). 28px control, quiet until focused. */
  .history-filter {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    margin: 8px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: var(--bg-subtle);
    color: var(--text-faint);
  }
  .history-filter:focus-within {
    border-color: var(--accent);
    background: var(--bg);
    color: var(--text-muted);
  }
  .filter-icon {
    display: inline-flex;
    align-items: center;
    flex-shrink: 0;
  }
  .filter-input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--text);
    font-family: inherit;
    font-size: 12.5px;
  }
  .filter-input::placeholder {
    color: var(--text-faint);
  }
  .filter-clear {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    width: 18px;
    height: 18px;
    padding: 0;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
  }
  .filter-clear:hover {
    background: var(--bg-muted);
    color: var(--text);
  }

  .empty-commits {
    padding: 24px 16px;
    text-align: center;
  }

  .history-status {
    padding: 16px;
  }

  .history-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 28px 16px;
    text-align: center;
    overflow-wrap: anywhere;
  }
  .empty-clear {
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: var(--bg-subtle);
    color: var(--text);
    font-size: 12px;
    padding: 4px 10px;
    cursor: pointer;
  }
  .empty-clear:hover {
    border-color: var(--accent);
    color: var(--accent);
  }

  /* Paging sentinel (F3). Sits at the list foot and self-loads when near. */
  .load-more-row {
    display: flex;
    justify-content: center;
    padding: 12px;
  }
  .load-more {
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: var(--bg-subtle);
    color: var(--text-muted);
    font-size: 12px;
    padding: 4px 14px;
    cursor: pointer;
  }
  .load-more:hover:not(:disabled) {
    border-color: var(--accent);
    color: var(--accent);
  }
  .load-more:disabled {
    cursor: default;
    opacity: 0.7;
  }

  /* Quiet branch summary pinned to the foot of the commits column
     (approved-prototype.html · .history-footer). */
  .history-footer {
    flex-shrink: 0;
    padding: 16px 19px;
    color: var(--text-faint);
    font-size: 11px;
  }

  .history-flagged {
    color: var(--warning);
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
    outline: none;
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

  /* Horizontal sash for the bottom terminal drawer */
  .terminal-sash {
    flex: 0 0 auto;
    height: 0;
    width: 100%;
    position: relative;
    cursor: row-resize;
    cursor: ns-resize;
    background-color: transparent;
    z-index: 25;
  }
  .terminal-sash::after {
    content: "";
    position: absolute;
    left: 0;
    right: 0;
    top: -5px;
    height: 10px;
    background-color: transparent;
    transition: background-color 120ms ease;
    z-index: 25;
  }
  .terminal-sash:hover::after,
  .terminal-sash:focus-visible::after,
  :global(body.resizing-row) .terminal-sash::after {
    background-color: var(--accent);
    opacity: 0.6;
  }

  /* Placeholder shown while the xterm chunk downloads on first open. */
  .terminal-loading {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: center;
    gap: 10px;
    border-top: 1px solid var(--border);
    background-color: var(--bg-subtle);
  }
  .terminal-retry {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: var(--bg);
    color: var(--text);
    cursor: pointer;
  }
  .terminal-retry:hover {
    border-color: var(--accent);
    color: var(--accent);
  }
</style>
