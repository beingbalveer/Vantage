<script lang="ts">
  import { onMount, onDestroy, tick, untrack } from "svelte";
  import type { RepoEntry, RepoStatus, CommitSummary, FileChange, DiffFile, DiffScope, TriageFlag } from "../../types";
  import * as api from "../../api";
  import { settings } from "../../stores/settings";
  import { panes, focusedPaneIndex, focusPane, setPaneTab } from "../../stores/panes";
  import { addToast, openDialog, activeDialog, paletteOpen, settingsOpen, shortcutsHelpOpen, addRepoDialogOpen, scanFolderDialogOpen, composerRequest } from "../../stores/ui";
  import { refreshSingleStatus } from "../../stores/repos";
  import { tagsByRepo, loadTags, groupTagsByTarget } from "../../stores/tags";
  import { navigationTarget, navigateTo } from "../../stores/navigation";
  import {
    HISTORY_PAGE_SIZE,
    formatHistoryFooter,
    hasMoreHistory,
    isEditableTarget,
  } from "../../history";
  import { findFlaggedCommitIndex, matchShortcut } from "../../keys";
  import type { MenuItem } from "../../menu";
  import ContextMenu from "../common/ContextMenu.svelte";
  import UncommittedRow from "./UncommittedRow.svelte";
  import CommitRow from "./CommitRow.svelte";
  import CommitHeader from "./CommitHeader.svelte";
  import BranchDropdown from "./BranchDropdown.svelte";
  import FileList from "./FileList.svelte";
  import CommitBox from "./CommitBox.svelte";
  import ConflictBanner from "./ConflictBanner.svelte";
  import { loadRepos } from "../../stores/repos";
  import DiffView from "../Diff/DiffView.svelte";
  import FilesTab from "./FilesTab.svelte";
  import TerminalDrawer from "../Terminal/TerminalDrawer.svelte";
  import Icon from "../common/Icon.svelte";
  import { viewportPaneWidths, savedPaneWidth } from "../../layout";
  import {
    createHorizontalResizer,
    createHorizontalResizerKeyboard,
    createVerticalBottomResizer,
    createVerticalBottomResizerKeyboard,
  } from "../../resizer";

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
  // Bumped when the palette asks to amend the last commit, so the composer
  // opens already in amend mode (ALPH-154).
  let amendRequestToken = $state(0);
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

  let terminalHeight = $state(savedPaneWidth("vantage_terminal_height", 240));

  const startTerminalResize = createVerticalBottomResizer({
    getHeight: () => terminalHeight,
    setHeight: (h) => {
      terminalHeight = h;
      panes.update((list) => {
        if (list[paneIndex]) {
          list[paneIndex] = { ...list[paneIndex], terminalHeight: h };
        }
        return list;
      });
    },
    minHeight: 80,
    maxHeight: 1200,
    storageKey: "vantage_terminal_height",
  });

  const nudgeTerminalResize = createVerticalBottomResizerKeyboard({
    getHeight: () => terminalHeight,
    setHeight: (h) => {
      terminalHeight = h;
      panes.update((list) => {
        if (list[paneIndex]) {
          list[paneIndex] = { ...list[paneIndex], terminalHeight: h };
        }
        return list;
      });
    },
    minHeight: 80,
    maxHeight: 1200,
    storageKey: "vantage_terminal_height",
  });

  // Commits & Selection
  let commits = $state<CommitSummary[]>([]);
  let selectedCommitSha = $state<string | null>(null);
  let isUncommittedSelected = $state(false);
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
  // True when the currently shown diff was fetched with whole-file context.
  // Reset on repo/scope/file change so the expanded state never leaks.
  let diffIsFullContext = $state(false);

  /** File-list record for the selection (authoritative binary/status/rename). */
  let selectedFileChange = $derived(
    selectedFilePath
      ? (scopeFilesList.find((f) => f.path === selectedFilePath) ?? null)
      : null
  );

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

  // Amend request from the command palette (ALPH-154): open the composer on the
  // uncommitted row in amend mode for the targeted repo.
  $effect(() => {
    const req = $composerRequest;
    if (!req) return;
    if (repo && req.repoId === repo.id) {
      isUncommittedSelected = true;
      selectedCommitSha = null;
      currentScope = { type: "working" };
      commitMode = true;
      showCommitDetails = false;
      amendRequestToken += 1;
      loadScopeFiles({ type: "working" });
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
      clearTimeout(filterDebounce);
      window.removeEventListener("keydown", handleFlagKey);
      unlistenHead?.();
      unlistenStatus?.();
    };
  });

  async function loadHistory(preserveSelection = false) {
    if (!repo) return;
    const gen = ++historyGen;
    historyLoading = true;
    historyLoadingMore = false;
    const query = historyFilterApplied;
    try {
      const page = await api.historyPage(repo.id, 0, HISTORY_PAGE_SIZE, query || undefined);
      if (gen !== historyGen) return;
      commits = page.commits;
      historyTotal = page.total;
      historyCapped = page.capped;
      historyLoading = false;

      // Batch-request triage flags for the loaded page. Failures are ignored
      // per-SHA (the backend omits SHAs it could not evaluate) so the list
      // never breaks because flagging failed (ALPH-147).
      void loadFlags(repo.id, page.commits.map((c) => c.sha), gen);

      if (!preserveSelection) {
        if (query) {
          // Filtering: land on the newest match, or clear when nothing matches.
          if (page.commits.length > 0) {
            selectCommit(page.commits[0].sha);
          } else {
            clearSelection();
          }
        } else if (status?.dirtyCount && status.dirtyCount > 0) {
          selectUncommitted();
        } else if (page.commits.length > 0) {
          selectCommit(page.commits[0].sha);
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
      if (gen !== historyGen) return;
      historyLoading = false;
      console.error("Failed to load history:", e);
    }
  }

  // Append the next page without disturbing selection, scroll or the open diff.
  async function loadMoreHistory() {
    if (!repo || historyLoading || historyLoadingMore) return;
    if (!hasMoreHistory(historyMeta)) return;
    const gen = historyGen;
    const skip = commits.length;
    historyLoadingMore = true;
    try {
      const page = await api.historyPage(
        repo.id,
        skip,
        HISTORY_PAGE_SIZE,
        historyFilterApplied || undefined
      );
      if (gen !== historyGen) return;
      const seen = new Set(commits.map((c) => c.sha));
      const newCommits = page.commits.filter((c) => !seen.has(c.sha));
      commits = [...commits, ...newCommits];
      historyTotal = page.total;
      historyCapped = page.capped;
      void loadFlags(repo.id, newCommits.map((c) => c.sha), gen);
    } catch (e: any) {
      if (gen !== historyGen) return;
      console.error("Failed to load more history:", e);
      addToast({ message: e.message || "Failed to load more commits", kind: "error" });
    } finally {
      if (gen === historyGen) historyLoadingMore = false;
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
  async function loadFlags(repoId: string, shas: string[], gen: number) {
    if (shas.length === 0) return;
    try {
      const result = await api.commitFlags(repoId, shas);
      if (gen !== historyGen) return;
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
  function selectUncommitted() {
    isUncommittedSelected = true;
    selectedCommitSha = null;
    currentScope = { type: "working" };
    commitMode = false;
    showCommitDetails = false;
    loadScopeFiles(currentScope);
  }

  function selectCommit(sha: string) {
    selectedCommitSha = sha;
    isUncommittedSelected = false;
    currentScope = { type: "commit", sha };
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

  async function selectFile(
    path: string,
    scope?: DiffScope,
    opts: { force?: boolean; fullContext?: boolean } = {}
  ) {
    if (!repo) return;
    const force = opts.force ?? false;
    const fullContext = opts.fullContext ?? false;
    const gen = ++diffGen;
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
      if (gen !== diffGen) return;
      currentDiffFile = diff;
    } catch (e: any) {
      if (gen !== diffGen) return;
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

</script>

<svelte:window onkeydown={handlePaneKeydown} />

<div
  class="pane-container {isFocused ? 'focused' : ''}"
  onpointerdown={() => focusPane(paneIndex)}
  onfocusin={() => focusPane(paneIndex)}
  role="region"
  aria-label="Repository Pane {paneIndex + 1}"
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
      />
    {:else}
      <!-- History Tab 3-Column Layout -->
      <div class="history-layout">
        <!-- Commits Column -->
        {#if !focusDiff}
          <div class="commits-col" style="width: {commitsColWidth}px;">
            <!-- Branch Selector Header with View Tabs -->
            <div class="commits-header">
              <div class="branch-wrap">
                <BranchDropdown {repo} {status} />
              </div>
              <div class="pane-view-tabs">
                <button
                  type="button"
                  class="view-tab active"
                  title="Commits & History"
                  aria-label="Commits view"
                >
                  <Icon name="commit" size={13} />
                  <span>Commits</span>
                </button>
                <button
                  type="button"
                  class="view-tab"
                  onclick={() => setPaneTab(paneIndex, "files")}
                  title="Project Files Explorer"
                  aria-label="Files explorer"
                >
                  <Icon name="folder" size={13} />
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
                  isSelected={selectedCommitSha === commit.sha}
                  isLocal={unpushedShas.has(commit.sha)}
                  isMenuOpen={isCommitMenuOpen(commit.sha)}
                  flags={flagsBySha[commit.sha] ?? []}
                  repoId={repo ? repo.id : ""}
                  tags={tagsByTarget.get(commit.sha) || []}
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

              <!-- Commit Box revealed on demand when uncommitted changes are selected -->
              {#if isUncommittedSelected && commitMode}
                <CommitBox
                  repoId={repo.id}
                  branch={status?.branch || "main"}
                  selectedPaths={checkedPaths}
                  conflictedPaths={status?.conflictedPaths ?? []}
                  amendToken={amendRequestToken}
                  oncommit={(preserve = false) => {
                    refreshSingleStatus(repo.id).then(() => {
                      loadHistory(preserve);
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
                scope={currentScope}
                fileChange={selectedFileChange}
                fullContext={diffIsFullContext}
                onexpandwholefile={() => expandWholeFile()}
                oncollapsecontext={() => collapseContext()}
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
        terminalHeight = 240;
        panes.update((list) => {
          if (list[paneIndex]) list[paneIndex] = { ...list[paneIndex], terminalHeight: 240 };
          return list;
        });
        try { localStorage.setItem("vantage_terminal_height", "240"); } catch {}
      }}
    ></div>

    {#key repo.id}
      <TerminalDrawer
        repoId={repo.id}
        repoName={repo.name}
        repoPath={repo.path}
        height={terminalHeight}
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

  .commits-header {
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
    position: relative;
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
    border-radius: 4px;
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
    color: #7c847a;
    font-size: 11px;
    user-select: none;
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
</style>
