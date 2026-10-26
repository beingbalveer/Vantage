<script lang="ts">
  import { tick } from "svelte";
  import { isFeatureEnabled } from "../../featureFlags";
  import type { NormalizedError } from "../../asyncState";
  import { emptyDiffMessage } from "../../asyncState";
  import Skeleton from "../common/Skeleton.svelte";
  import EmptyState from "../common/EmptyState.svelte";
  import Banner from "../common/Banner.svelte";
  import type {
    DiffFile,
    DiffMode,
    DiffScope,
    FileChange,
    FileDiffMeta,
    Hunk,
    DiffLine,
    NoteSide,
    LineSelection,
    WorkingPart,
    WorkingPartCounts,
  } from "../../types";
  import { getLanguageFromPath, highlightCodeLines } from "../../highlight";
  import * as api from "../../api";
  import { repos, activeRepoId } from "../../stores/repos";
  import { panes, focusedPaneIndex } from "../../stores/panes";
  import { addToast } from "../../stores/ui";
  import { settings, updateSettings } from "../../stores/settings";
  import { registerHandler, registerDismiss, ownsPaneKeyboard } from "../../commands";
  import Icon from "../common/Icon.svelte";
  import { Modal, Button } from "../common";
  import BinaryDiff from "./BinaryDiff.svelte";
  import ImageDiff from "./ImageDiff.svelte";
  import { classifyDiffContent } from "../../diff-content";
  import { buildSplitRows, type SplitRow, type SplitCell } from "../../diff-split";

  interface Props {
    diffFile: DiffFile | null;
    diffMode?: DiffMode;
    ignoreWhitespace?: boolean;
    repoId?: string;
    rev?: string;
    loading?: boolean;
    pending?: boolean;
    error?: NormalizedError | null;
    onretry?: () => void;
    scope?: DiffScope | null;
    fileChange?: FileChange | null;
    fullContext?: boolean;
    scopeType?: "working" | "commit" | "range";
    workingPart?: WorkingPart;
    workingCounts?: WorkingPartCounts | null;
    onchangepart?: (part: WorkingPart) => void;
    onrefresh?: () => void;
    paneIndex?: number;
    onexpandcontext?: (oldStart: number, count: number) => void;
    onexpandwholefile?: () => void;
    oncollapsecontext?: () => void;
    onviewfile?: (path: string) => void;
    onloadforce?: () => void;
  }

  let {
    diffFile,
    diffMode = "unified",
    ignoreWhitespace = false,
    repoId,
    rev = "HEAD",
    loading = false,
    pending = false,
    error = null,
    onretry,
    scope = null,
    fileChange = null,
    fullContext = false,
    scopeType = "commit",
    workingPart = "unstaged",
    workingCounts = null,
    onchangepart,
    onrefresh,
    paneIndex = 0,
    onexpandcontext,
    onexpandwholefile,
    oncollapsecontext,
    onviewfile,
    onloadforce,
  }: Props = $props();

  /** File-list metadata wins when it describes the same path (numstat is the
   *  source of truth for `binary`); the standalone diff fills in the rest. */
  let effectiveChange = $derived(
    fileChange && diffFile && fileChange.path === diffFile.change.path
      ? fileChange
      : (diffFile?.change ?? null)
  );

  /** text | binary | image — images win by extension, else git's binary flag. */
  let contentKind = $derived(
    effectiveChange
      ? classifyDiffContent(
          effectiveChange.path,
          effectiveChange.binary || Boolean(diffFile?.change.binary)
        )
      : "text"
  );

  // Active repo ID resolution
  let effectiveRepoId = $derived(
    repoId || $panes[$focusedPaneIndex]?.repoId || $activeRepoId || ""
  );

  let isTruncated = $derived(
    Boolean(diffFile && (diffFile.truncated || (diffFile as any).isTruncated))
  );

  // Binary / image metadata (sizes + bounded previews). Fetched lazily only for
  // non-text content, guarded so a stale response never clobbers a newer file.
  let fileMeta = $state<FileDiffMeta | null>(null);
  let fileMetaLoading = $state(false);
  let fileMetaError = $state(false);
  let fileMetaGen = 0;

  $effect(() => {
    const df = diffFile;
    const change = effectiveChange;
    const sc = scope;
    const kind = contentKind;
    const repo = effectiveRepoId;
    if (!df || !repo || (kind !== "binary" && kind !== "image")) {
      fileMeta = null;
      fileMetaLoading = false;
      fileMetaError = false;
      return;
    }
    const gen = ++fileMetaGen;
    const path = change?.path ?? df.change.path;
    const oldPath = change?.oldPath ?? null;
    const activeScope: DiffScope = sc ?? { type: "commit", sha: rev };
    fileMeta = null;
    fileMetaLoading = true;
    fileMetaError = false;
    (async () => {
      try {
        const meta = await api.fileDiffMeta(repo, activeScope, path, oldPath);
        if (gen !== fileMetaGen) return;
        fileMeta = meta;
      } catch (e) {
        if (gen !== fileMetaGen) return;
        console.error("Failed to load binary/image metadata:", e);
        fileMeta = null;
        fileMetaError = true;
      } finally {
        if (gen === fileMetaGen) fileMetaLoading = false;
      }
    })();
  });

  // Preserve the reader's place when switching unified ↔ split; the two modes
  // have different row heights, so keep the relative scroll position.
  let textBodyEl = $state<HTMLDivElement | null>(null);

  async function setDiffMode(mode: DiffMode) {
    if (mode === diffMode) return;
    const el = textBodyEl;
    const ratio =
      el && el.scrollHeight > el.clientHeight
        ? el.scrollTop / (el.scrollHeight - el.clientHeight)
        : null;

    const saving = updateSettings({ ...$settings, diffMode: mode });
    await tick();
    if (el && textBodyEl && ratio != null) {
      textBodyEl.scrollTop = ratio * (textBodyEl.scrollHeight - textBodyEl.clientHeight);
    }
    await saving;
  }

  // ---- Split (side-by-side) helpers ----

  /** Pair a row's left/right cells with their side so markup can be shared. */
  function cellPairs(row: SplitRow): [("old" | "new"), SplitCell][] {
    if (row.type !== "line") return [];
    return [
      ["old", row.left],
      ["new", row.right],
    ];
  }

  function isCellSelected(cell: SplitCell, side: "old" | "new"): boolean {
    if (!selectedRange || cell.kind === "empty") return false;
    if (selectedRange.side !== side) return false;
    const num = side === "old" ? cell.oldNo : cell.newNo;
    if (num == null) return false;
    return num >= selectedRange.start && num <= selectedRange.end;
  }

  function splitGutterNumber(cell: SplitCell, side: "old" | "new"): number | null {
    return side === "old" ? cell.oldNo : cell.newNo;
  }

  function splitSign(cell: SplitCell, side: "old" | "new"): string {
    if (cell.kind === "context") return "\u00a0";
    if (side === "old" && cell.kind === "del") return "−";
    if (side === "new" && cell.kind === "add") return "+";
    return "\u00a0";
  }

  function statusLabel(kind: string): string {
    switch (kind) {
      case "added":
        return "added";
      case "deleted":
        return "deleted";
      case "renamed":
        return "renamed";
      case "untracked":
        return "added";
      default:
        return "modified";
    }
  }

  function statusHasOld(kind: string): boolean {
    return kind !== "added" && kind !== "untracked";
  }

  function statusHasNew(kind: string): boolean {
    return kind !== "deleted";
  }

  // Local reactive copy of hunks for context expansion
  let localHunks = $state<Hunk[]>([]);
  let emptyCopy = $derived(emptyDiffMessage(Boolean(diffFile?.change.binary)));

  $effect(() => {
    if (diffFile && diffFile.hunks) {
      localHunks = diffFile.hunks.map((h) => ({
        oldStart: h.oldStart,
        oldLines: h.oldLines,
        newStart: h.newStart,
        newLines: h.newLines,
        header: h.header,
        lines: [...h.lines],
      }));
    } else {
      localHunks = [];
    }
    selectedRange = null;
  });

  // Range selection & Floating action bar state
  let selectedRange = $state<{
    start: number;
    end: number;
    side: NoteSide;
  } | null>(null);

  function handleLineClick(lineNo: number | null, side: NoteSide, e: MouseEvent) {
    if (lineNo == null) return;
    if (e.shiftKey && selectedRange) {
      const start = Math.min(selectedRange.start, lineNo);
      const end = Math.max(selectedRange.end, lineNo);
      selectedRange = { start, end, side: selectedRange.side };
    } else {
      selectedRange = { start: lineNo, end: lineNo, side };
    }
  }

  function isLineSelected(line: DiffLine): boolean {
    if (!selectedRange) return false;
    const num = selectedRange.side === "old" ? line.oldNo : line.newNo;
    if (num == null) return false;
    return num >= selectedRange.start && num <= selectedRange.end;
  }

  let stagingEnabled = $derived(
    isFeatureEnabled("ALPH_160_LINE_PARTIAL_COMMIT") && scopeType === "working"
  );
  let emptyMessage = $derived(
    scopeType === "working"
      ? workingPart === "staged"
        ? "No staged changes in this file"
        : "No unstaged changes in this file"
      : "No changes in this file"
  );

  let isApplying = $state(false);
  let discardTarget = $state<{ path: string; lines: LineSelection[] } | null>(null);

  /** Changed (add/del) lines covered by the current gutter selection. */
  function getSelectedChangeLines(): LineSelection[] {
    if (!selectedRange) return [];
    const out: LineSelection[] = [];
    for (const hunk of localHunks) {
      for (const line of hunk.lines) {
        if (line.kind !== "add" && line.kind !== "del") continue;
        const num = selectedRange.side === "old" ? line.oldNo : line.newNo;
        if (num == null || num < selectedRange.start || num > selectedRange.end) continue;
        out.push({
          kind: line.kind,
          oldNo: line.oldNo,
          newNo: line.newNo,
          text: line.text,
        });
      }
    }
    return out;
  }

  function getHunkChangeLines(hunk: Hunk): LineSelection[] {
    return hunk.lines
      .filter((l) => l.kind === "add" || l.kind === "del")
      .map((l) => ({
        kind: l.kind,
        oldNo: l.oldNo,
        newNo: l.newNo,
        text: l.text,
      }));
  }

  let selectedChangeLines = $derived(getSelectedChangeLines());
  let canApplySelection = $derived(
    stagingEnabled && selectedChangeLines.length > 0 && !isApplying
  );

  function lineCountLabel(count: number): string {
    return `${count} line${count === 1 ? "" : "s"}`;
  }

  async function runPartial(
    action: "stage" | "unstage",
    lines: LineSelection[]
  ): Promise<void> {
    if (!diffFile || !effectiveRepoId || lines.length === 0 || isApplying) return;
    isApplying = true;
    try {
      if (action === "stage") {
        await api.stageLines(effectiveRepoId, diffFile.change.path, lines, ignoreWhitespace);
        addToast({
          message: `Staged ${lineCountLabel(lines.length)} in ${diffFile.change.path}`,
          kind: "success",
        });
      } else {
        await api.unstageLines(effectiveRepoId, diffFile.change.path, lines, ignoreWhitespace);
        addToast({
          message: `Unstaged ${lineCountLabel(lines.length)} in ${diffFile.change.path}`,
          kind: "success",
        });
      }
      selectedRange = null;
      onrefresh?.();
    } catch (e: any) {
      addToast({
        message: e?.message || e?.toString?.() || "Could not apply the selected lines",
        kind: "error",
      });
    } finally {
      isApplying = false;
    }
  }

  function stageSelection() {
    runPartial("stage", selectedChangeLines);
  }

  function unstageSelection() {
    runPartial("unstage", selectedChangeLines);
  }

  function stageHunk(hunk: Hunk) {
    runPartial(workingPart === "staged" ? "unstage" : "stage", getHunkChangeLines(hunk));
  }

  function requestDiscardSelection() {
    if (!diffFile || selectedChangeLines.length === 0) return;
    discardTarget = { path: diffFile.change.path, lines: [...selectedChangeLines] };
  }

  async function confirmDiscard() {
    if (!discardTarget || !effectiveRepoId || isApplying) return;
    const target = discardTarget;
    isApplying = true;
    try {
      await api.discardLines(effectiveRepoId, target.path, target.lines, ignoreWhitespace);
      addToast({
        message: `Discarded ${lineCountLabel(target.lines.length)} in ${target.path}`,
        kind: "info",
      });
      discardTarget = null;
      selectedRange = null;
      onrefresh?.();
    } catch (e: any) {
      // Keep the selection and the confirmation open so the user can retry
      // after refreshing; the backend changed nothing.
      addToast({
        message: e?.message || e?.toString?.() || "Could not discard the selected lines",
        kind: "error",
      });
    } finally {
      isApplying = false;
    }
  }

  function getSelectedSnippet(): string {
    if (!diffFile || !selectedRange) return "";
    const linesToCopy: string[] = [];
    for (const hunk of localHunks) {
      for (const line of hunk.lines) {
        const num = selectedRange.side === "old" ? line.oldNo : line.newNo;
        if (num != null && num >= selectedRange.start && num <= selectedRange.end) {
          linesToCopy.push(line.text);
        }
      }
    }
    return linesToCopy.join("\n");
  }

  async function handleCopyReference() {
    if (!diffFile || !selectedRange) return;
    const ref =
      selectedRange.start === selectedRange.end
        ? `${diffFile.change.path}:L${selectedRange.start}`
        : `${diffFile.change.path}:L${selectedRange.start}-L${selectedRange.end}`;
    await navigator.clipboard.writeText(ref);
    addToast({ message: `Copied ${ref}`, kind: "info" });
  }

  async function handleCopySnippet() {
    const text = getSelectedSnippet();
    if (!text) return;
    await navigator.clipboard.writeText(text);
    addToast({ message: "Copied snippet to clipboard", kind: "info" });
  }


  // Hunk boundaries
  function getHunkNewEnd(hunk: Hunk): number {
    let max = hunk.newStart - 1;
    for (const l of hunk.lines) {
      if (l.newNo != null && l.newNo > max) max = l.newNo;
    }
    return max > hunk.newStart - 1 ? max : hunk.newStart + hunk.newLines - 1;
  }

  function getHunkOldEnd(hunk: Hunk): number {
    let max = hunk.oldStart - 1;
    for (const l of hunk.lines) {
      if (l.oldNo != null && l.oldNo > max) max = l.oldNo;
    }
    return max > hunk.oldStart - 1 ? max : hunk.oldStart + hunk.oldLines - 1;
  }

  // Context expansion handlers
  async function expandAboveHunk(hunkIndex: number, count?: number) {
    if (!diffFile || !effectiveRepoId || hunkIndex < 0 || hunkIndex >= localHunks.length) return;
    const hunk = localHunks[hunkIndex];
    const prevEnd = hunkIndex > 0 ? getHunkNewEnd(localHunks[hunkIndex - 1]) : 0;
    const gap = hunk.newStart - 1 - prevEnd;
    if (gap <= 0) return;

    const linesToFetch = count ? Math.min(count, gap) : gap;
    const start = hunk.newStart - linesToFetch;
    const end = hunk.newStart - 1;

    try {
      const fetched = await api.blobLines(effectiveRepoId, rev, diffFile.change.path, start, end);
      if (!fetched || fetched.length === 0) return;

      const newDiffLines: DiffLine[] = fetched.map((text, idx) => {
        const newNo = start + idx;
        const delta = hunk.newStart - hunk.oldStart;
        const oldNo = newNo - delta;
        return {
          kind: "context",
          oldNo: oldNo > 0 ? oldNo : newNo,
          newNo,
          text,
          wordRanges: null,
        };
      });

      hunk.lines = [...newDiffLines, ...hunk.lines];
      hunk.newStart = start;
      hunk.newLines += newDiffLines.length;
      hunk.oldStart = newDiffLines[0].oldNo ?? hunk.oldStart;
      hunk.oldLines += newDiffLines.length;

      mergeAdjacent();
      onexpandcontext?.(hunk.oldStart, newDiffLines.length);
    } catch (e) {
      console.error("Failed to expand context above:", e);
    }
  }

  async function expandBelowHunk(hunkIndex: number, count: number = 20) {
    if (!diffFile || !effectiveRepoId || hunkIndex < 0 || hunkIndex >= localHunks.length) return;
    const hunk = localHunks[hunkIndex];
    const curEnd = getHunkNewEnd(hunk);
    const nextStart = hunkIndex < localHunks.length - 1 ? localHunks[hunkIndex + 1].newStart : Infinity;
    const gap = nextStart === Infinity ? count : nextStart - 1 - curEnd;
    if (gap <= 0) return;

    const linesToFetch = Math.min(count, gap);
    const start = curEnd + 1;
    const end = curEnd + linesToFetch;

    try {
      const fetched = await api.blobLines(effectiveRepoId, rev, diffFile.change.path, start, end);
      if (!fetched || fetched.length === 0) return;

      const delta = hunk.newStart - hunk.oldStart;
      const newDiffLines: DiffLine[] = fetched.map((text, idx) => {
        const newNo = start + idx;
        const oldNo = newNo - delta;
        return {
          kind: "context",
          oldNo: oldNo > 0 ? oldNo : newNo,
          newNo,
          text,
          wordRanges: null,
        };
      });

      hunk.lines = [...hunk.lines, ...newDiffLines];
      hunk.newLines += newDiffLines.length;
      hunk.oldLines += newDiffLines.length;

      mergeAdjacent();
      onexpandcontext?.(hunk.oldStart, newDiffLines.length);
    } catch (e) {
      console.error("Failed to expand context below:", e);
    }
  }

  function mergeAdjacent() {
    if (localHunks.length <= 1) return;
    const merged: Hunk[] = [localHunks[0]];

    for (let i = 1; i < localHunks.length; i++) {
      const prev = merged[merged.length - 1];
      const curr = localHunks[i];
      const prevEnd = getHunkNewEnd(prev);

      if (prevEnd >= curr.newStart - 1) {
        prev.lines = [...prev.lines, ...curr.lines];
        prev.newLines = getHunkNewEnd(prev) - prev.newStart + 1;
        prev.oldLines = getHunkOldEnd(prev) - prev.oldStart + 1;
      } else {
        merged.push(curr);
      }
    }
    localHunks = merged;
  }

  /**
   * Toggle whole-file context. Expansion is fetched from the backend (a single
   * diff command with large context) rather than merged client-side, so the
   * expanded hunks are always exact and the state is owned by the pane's
   * diff-generation guard.
   */
  function toggleWholeFile() {
    if (fullContext) {
      oncollapsecontext?.();
    } else {
      onexpandwholefile?.();
    }
  }

  let activeHunkIndex = $state(-1);

  $effect(() => {
    // Reset hunk cursor whenever the displayed file changes.
    void diffFile;
    activeHunkIndex = -1;
  });

  function moveHunk(direction: 1 | -1): boolean {
    if (!textBodyEl || localHunks.length === 0) return false;
    const headers = Array.from(
      textBodyEl.querySelectorAll<HTMLElement>("[data-hunk-index]")
    );
    if (headers.length === 0) return false;

    if (activeHunkIndex < 0) {
      activeHunkIndex = direction > 0 ? 0 : headers.length - 1;
    } else {
      activeHunkIndex = Math.max(
        0,
        Math.min(headers.length - 1, activeHunkIndex + direction)
      );
    }
    headers[activeHunkIndex]?.scrollIntoView({ block: "start", behavior: "smooth" });
    return true;
  }

  $effect(() => {
    const unregister = [
      registerHandler(
        "expand_context",
        () => {
          if (!ownsPaneKeyboard(paneIndex) || !diffFile) return false;
          toggleWholeFile();
          return true;
        },
        "diff"
      ),
      registerHandler(
        "next_hunk",
        () => (ownsPaneKeyboard(paneIndex) ? moveHunk(1) : false),
        "diff"
      ),
      registerHandler(
        "prev_hunk",
        () => (ownsPaneKeyboard(paneIndex) ? moveHunk(-1) : false),
        "diff"
      ),
      registerHandler(
        "toggle_diff_mode",
        () => {
          if (!ownsPaneKeyboard(paneIndex)) return false;
          void setDiffMode(diffMode === "split" ? "unified" : "split");
          return true;
        },
        "diff"
      ),
      registerDismiss(() => {
        if (!ownsPaneKeyboard(paneIndex)) return false;
        if (selectedRange) {
          selectedRange = null;
          return true;
        }
        return false;
      }),
    ];
    return () => unregister.forEach((off) => off());
  });

  function handleKeydown(e: KeyboardEvent) {
    const target = e.target as HTMLElement | null;
    if (
      target &&
      (target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.isContentEditable)
    ) {
      return;
    }

    if (e.key === "e" || e.key === "E") {
      if (!e.metaKey && !e.ctrlKey && !e.altKey) {
        e.preventDefault();
        toggleWholeFile();
      }
    } else if (e.key === "s" || e.key === "S") {
      if (!e.metaKey && !e.ctrlKey && !e.altKey) {
        e.preventDefault();
        setDiffMode(diffMode === "split" ? "unified" : "split");
      }
    } else if (e.key === "Escape") {
      selectedRange = null;
    }
  }
  /** Normalized hunk label. The backend already stores context (text after the
   *  second `@@`), so we only strip a leading range if one slipped through,
   *  which prevents printing `@@ … @@` twice (RELAY-UI · UI-06). */
  function hunkLabel(hunk: Hunk): string {
    const range = `@@ -${hunk.oldStart},${hunk.oldLines} +${hunk.newStart},${hunk.newLines} @@`;
    const context = (hunk.header || "").replace(/^@@.*?@@\s*/, "").trim();
    return context ? `${range} ${context}` : range;
  }

  /** Syntax-highlight a single diff line. Falls back to escaped plain text for
   *  unknown languages and always returns escaped HTML — never raw source. */
  function renderCode(text: string): string {
    const language = getLanguageFromPath(diffFile?.change.path || "");
    if (!language) return escapeHtml(text);
    return highlightCodeLines(text, language)[0] ?? escapeHtml(text);
  }

  function renderWordDiff(
    text: string,
    ranges: [number, number][] | null,
    kind: string
  ): string {
    // Word-level marks take precedence and are escaped manually; no change is
    // ever injected unescaped into the DOM.
    if (!ranges || ranges.length === 0) {
      return renderCode(text);
    }

    let out = "";
    let lastIdx = 0;
    const wordClass = kind === "add" ? "diff-add-word" : "diff-del-word";

    for (const [start, end] of ranges) {
      if (start > lastIdx) {
        out += escapeHtml(text.slice(lastIdx, start));
      }
      out += `<span class="${wordClass}">${escapeHtml(text.slice(start, end))}</span>`;
      lastIdx = end;
    }

    if (lastIdx < text.length) {
      out += escapeHtml(text.slice(lastIdx));
    }

    return out;
  }

  function escapeHtml(str: string): string {
    return str
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;");
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#snippet expandAboveRow(hIdx: number, gapAbove: number)}
  <div class="expand-row font-mono text-xs">
    <span class="expand-label flex items-center gap-1">
      <Icon name="arrow-up-down" size={11} strokeWidth={2.2} />
      <span>{gapAbove} hidden lines</span>
    </span>
    {#if gapAbove <= 20}
      <button class="expand-btn" onclick={() => expandAboveHunk(hIdx, gapAbove)}>
        Show all {gapAbove} hidden lines
      </button>
    {:else}
      <button
        class="expand-btn inline-flex items-center gap-1"
        onclick={() => expandAboveHunk(hIdx, 20)}
      >
        <span>Show 20 lines above</span>
        <Icon name="arrow-up" size={11} strokeWidth={2.2} />
      </button>
      <button class="expand-btn" onclick={() => expandAboveHunk(hIdx, gapAbove)}>
        Show all {gapAbove} lines
      </button>
    {/if}
  </div>
{/snippet}

{#snippet expandBelowRow(hIdx: number)}
  <div class="expand-row font-mono text-xs">
    <span class="expand-label flex items-center gap-1">
      <Icon name="arrow-up-down" size={11} strokeWidth={2.2} />
      <span>Next hidden lines</span>
    </span>
    <button
      class="expand-btn inline-flex items-center gap-1"
      onclick={() => expandBelowHunk(hIdx, 20)}
    >
      <span>Show 20 lines below</span>
      <Icon name="arrow-down" size={11} strokeWidth={2.2} />
    </button>
  </div>
{/snippet}

<div class="diff-view-container">
  {#if loading}
    <div class="diff-loading">
      <Skeleton rows={10} height={25} shape="line" label="Loading diff" />
    </div>
  {:else if error}
    <Banner kind="error" message={error.message} {error} retryLabel="Retry" {onretry} />
    <div class="diff-state-fill">
      <span class="text-sm text-muted">Couldn't load this diff.</span>
    </div>
  {:else if !diffFile && pending}
    <!-- A file is selected but its diff has not arrived yet -->
  {:else if !diffFile}
    <div class="empty-diff">
      <span class="text-muted">Select a file to view changes</span>
    </div>
  {:else}
    <!-- File Header: always rendered so whole-file state stays reachable even
         when a diff is capped. -->
    <div class="file-header">
      <div class="file-path-group">
        <span class="file-path" title={diffFile.change.path}>{diffFile.change.path}</span>
        {#if (effectiveChange?.additions ?? 0) > 0 || (effectiveChange?.deletions ?? 0) > 0}
          <span class="file-stats text-xs font-mono">
            {#if (effectiveChange?.additions ?? 0) > 0}
              <span class="stat-add">+{effectiveChange?.additions}</span>
            {/if}
            {#if (effectiveChange?.deletions ?? 0) > 0}
              <span class="stat-del">−{effectiveChange?.deletions}</span>
            {/if}
          </span>
        {/if}
      </div>

      <div class="header-actions">
        {#if stagingEnabled}
          <div class="part-switch" role="group" aria-label="Working diff view">
            <button
              type="button"
              class:active={workingPart === "unstaged"}
              aria-pressed={workingPart === "unstaged"}
              onclick={() => onchangepart?.("unstaged")}
            >
              Unstaged{workingCounts ? ` ${workingCounts.unstaged}` : ""}
            </button>
            <button
              type="button"
              class:active={workingPart === "staged"}
              aria-pressed={workingPart === "staged"}
              onclick={() => onchangepart?.("staged")}
            >
              Staged{workingCounts ? ` ${workingCounts.staged}` : ""}
            </button>
          </div>
        {/if}

        {#if isFeatureEnabled("ALPH_159_EXPAND_WHOLE_FILE")}
          {#if fullContext}
            <span class="whole-file-chip text-xs" title="Showing the whole file">Whole file</span>
            <button
              class="header-icon-btn"
              type="button"
              title="Collapse to default context (e)"
              aria-label="Collapse to default context"
              onclick={() => oncollapsecontext?.()}
            >
              <Icon name="fold" size={14} />
            </button>
          {:else}
            <button
              class="expand-whole-btn"
              type="button"
              title="Expand whole file (e)"
              aria-label="Expand whole file"
              onclick={() => onexpandwholefile?.()}
            >
              <Icon name="unfold" size={13} />
              <span>Expand whole file</span>
            </button>
          {/if}
        {/if}

        {#if contentKind === "text" && localHunks.length > 0 && isFeatureEnabled("ALPH_158_SPLIT_IMAGE_DIFF")}
          <div class="mode-toggle" role="group" aria-label="Diff layout">
            <button
              class="mode-btn"
              class:active={diffMode === "unified"}
              aria-pressed={diffMode === "unified"}
              title="Unified diff (S to toggle)"
              onclick={() => setDiffMode("unified")}
            >Unified</button>
            <button
              class="mode-btn"
              class:active={diffMode === "split"}
              aria-pressed={diffMode === "split"}
              title="Side-by-side diff (S to toggle)"
              onclick={() => setDiffMode("split")}
            >Split</button>
          </div>
        {/if}
      </div>
    </div>

    {#if isTruncated}
      <div class="truncated-card">
        <span class="text-strong">Large diff</span>
        <span class="text-sm text-muted">
          {fullContext
            ? "This file is too large to expand fully. Load it anyway to see every change."
            : "This file has a very large diff that was hidden to keep performance instant."}
        </span>
        <button class="load-btn" onclick={onloadforce}>Load diff anyway</button>
      </div>
    {:else if isFeatureEnabled("ALPH_158_SPLIT_IMAGE_DIFF") && contentKind === "image"}
      <div class="diff-body">
        <ImageDiff
          path={diffFile.change.path}
          oldImage={fileMeta?.oldImage ?? null}
          newImage={fileMeta?.newImage ?? null}
          oldSize={fileMeta?.oldSize ?? null}
          newSize={fileMeta?.newSize ?? null}
          oldExists={fileMeta?.oldExists ??
            statusHasOld(effectiveChange?.status ?? diffFile.change.status)}
          newExists={fileMeta?.newExists ??
            statusHasNew(effectiveChange?.status ?? diffFile.change.status)}
          oldTooLarge={fileMeta?.oldTooLarge ?? false}
          newTooLarge={fileMeta?.newTooLarge ?? false}
          capBytes={fileMeta?.capBytes ?? 4 * 1024 * 1024}
          status={statusLabel(effectiveChange?.status ?? diffFile.change.status)}
          loading={fileMetaLoading || (!fileMeta && !fileMetaError)}
          error={fileMetaError}
        />
      </div>
    {:else if isFeatureEnabled("ALPH_158_SPLIT_IMAGE_DIFF") && contentKind === "binary"}
      <div class="diff-body">
        <BinaryDiff
          path={diffFile.change.path}
          oldSize={fileMeta?.oldSize ?? null}
          newSize={fileMeta?.newSize ?? null}
          oldExists={fileMeta?.oldExists ??
            statusHasOld(effectiveChange?.status ?? diffFile.change.status)}
          newExists={fileMeta?.newExists ??
            statusHasNew(effectiveChange?.status ?? diffFile.change.status)}
          status={statusLabel(effectiveChange?.status ?? diffFile.change.status)}
          loading={fileMetaLoading || (!fileMeta && !fileMetaError)}
          detail={fileMetaError ? "Couldn't read blob sizes from Git for this file." : undefined}
        />
      </div>
    {:else}
    <!-- Hunks Body -->
    <div class="diff-body" bind:this={textBodyEl} tabindex="-1">
        {#if localHunks.length === 0}
          {#if stagingEnabled}
            <div class="empty-diff">
              <span class="text-muted">{emptyMessage}</span>
            </div>
          {:else}
            <EmptyState
              icon={diffFile.change.binary ? "binary" : "check"}
              title={emptyCopy.title}
              description={emptyCopy.description}
            />
          {/if}
        {:else if diffMode === "split"}
          <!-- Split (side-by-side) rendering -->
          <div class="split-grid">
            {#each localHunks as hunk, hIdx (hIdx)}
              {@const prevEnd = hIdx > 0 ? getHunkNewEnd(localHunks[hIdx - 1]) : 0}
              {@const gapAbove = hunk.newStart - 1 - prevEnd}
              {#if gapAbove > 0}
                {@render expandAboveRow(hIdx, gapAbove)}
              {/if}

              <div class="hunk-header font-mono text-xs">
                <span class="hunk-label">{hunkLabel(hunk)}</span>
                {#if stagingEnabled}
                  <button
                    type="button"
                    class="hunk-btn"
                    disabled={isApplying || getHunkChangeLines(hunk).length === 0}
                    title={workingPart === "staged"
                      ? "Unstage every changed line in this hunk"
                      : "Stage every changed line in this hunk"}
                    onclick={() => stageHunk(hunk)}
                  >
                    {workingPart === "staged" ? "Unstage hunk" : "Stage hunk"}
                  </button>
                {/if}
              </div>

              {#each buildSplitRows(hunk.lines) as row}
                {#if row.type === "note"}
                  <div class="split-note font-mono text-xs">{row.text}</div>
                {:else}
                  {#each cellPairs(row) as [side, cell]}
                    <div
                      class="split-cell {side} {cell.kind}"
                      class:selected={isCellSelected(cell, side)}
                      class:empty={cell.kind === "empty"}
                    >
                      {#if cell.kind === "empty"}
                        <div class="empty-hatch"></div>
                      {:else}
                        <div
                          class="gutter-num font-mono"
                          role="button"
                          tabindex="0"
                          onclick={(e) => handleLineClick(splitGutterNumber(cell, side), side, e)}
                          onkeydown={(e) =>
                            e.key === "Enter" &&
                            handleLineClick(splitGutterNumber(cell, side), side, e as any)}
                        >{splitGutterNumber(cell, side) ?? ""}</div>
                        <div class="sign font-mono">{splitSign(cell, side)}</div>
                        <div class="code-text font-mono" title={cell.text}>
                          {@html renderWordDiff(
                            cell.text,
                            cell.wordRanges,
                            cell.kind === "del" ? "del" : cell.kind === "add" ? "add" : "context"
                          )}
                        </div>
                      {/if}
                    </div>
                  {/each}
                {/if}
              {/each}

              {#if hIdx === localHunks.length - 1}
                {@render expandBelowRow(hIdx)}
              {/if}
            {/each}
          </div>
        {:else}
          {#each localHunks as hunk, hIdx (hIdx)}
            <!-- Gap above hunk -->
            {@const prevEnd = hIdx > 0 ? getHunkNewEnd(localHunks[hIdx - 1]) : 0}
            {@const gapAbove = hunk.newStart - 1 - prevEnd}
            {#if gapAbove > 0}
              <div class="expand-row font-mono text-xs">
                <span class="expand-label flex items-center gap-1">
                  <Icon name="arrow-up-down" size={11} strokeWidth={2.2} />
                  <span>{gapAbove} hidden lines</span>
                </span>
                {#if gapAbove <= 20}
                  <button
                    class="expand-btn"
                    onclick={() => expandAboveHunk(hIdx, gapAbove)}
                  >
                    Show all {gapAbove} hidden lines
                  </button>
                {:else}
                  <button
                    class="expand-btn inline-flex items-center gap-1"
                    onclick={() => expandAboveHunk(hIdx, 20)}
                  >
                    <span>Show 20 lines above</span>
                    <Icon name="arrow-up" size={11} strokeWidth={2.2} />
                  </button>
                  <button
                    class="expand-btn"
                    onclick={() => expandAboveHunk(hIdx, gapAbove)}
                  >
                    Show all {gapAbove} lines
                  </button>
                {/if}
              </div>
            {/if}

            <!-- Hunk Header -->
            <div class="hunk-header font-mono text-xs" data-hunk-index={hIdx}>
              <span class="hunk-label">{hunkLabel(hunk)}</span>
              {#if stagingEnabled}
                <button
                  type="button"
                  class="hunk-btn"
                  disabled={isApplying || getHunkChangeLines(hunk).length === 0}
                  title={workingPart === "staged"
                    ? "Unstage every changed line in this hunk"
                    : "Stage every changed line in this hunk"}
                  onclick={() => stageHunk(hunk)}
                >
                  {workingPart === "staged" ? "Unstage hunk" : "Stage hunk"}
                </button>
              {/if}
            </div>

            <!-- Hunk Lines: Unified -->
            {#each hunk.lines as line, lIdx (lIdx)}
              {@const isSelected = isLineSelected(line)}
              <div class="diff-line {line.kind}" class:selected={isSelected}>
                <!-- Old line number -->
                <div
                  class="gutter-num old-num font-mono"
                  role="button"
                  tabindex="0"
                  aria-label={line.oldNo != null ? `Select old line ${line.oldNo}` : undefined}
                  aria-pressed={line.oldNo != null && isSelected ? true : undefined}
                  title={line.oldNo != null ? "Select old line (Shift-click to extend)" : undefined}
                  onclick={(e) => handleLineClick(line.oldNo, "old", e)}
                  onkeydown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      handleLineClick(line.oldNo, "old", e as any);
                    }
                  }}
                >{line.oldNo ?? ""}</div>

                <!-- New line number -->
                <div
                  class="gutter-num new-num font-mono"
                  role="button"
                  tabindex="0"
                  aria-label={line.newNo != null ? `Select new line ${line.newNo}` : undefined}
                  aria-pressed={line.newNo != null && isSelected ? true : undefined}
                  title={line.newNo != null ? "Select new line (Shift-click to extend)" : undefined}
                  onclick={(e) => handleLineClick(line.newNo, "new", e)}
                  onkeydown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      handleLineClick(line.newNo, "new", e as any);
                    }
                  }}
                >{line.newNo ?? ""}</div>

                <!-- Sign column -->
                <div class="sign font-mono" aria-hidden="true">
                  {#if isSelected && (line.kind === "add" || line.kind === "del")}
                    <span class="selected-dot"></span>
                  {:else if line.kind === "add"}+{:else if line.kind === "del"}−{:else}&nbsp;{/if}
                </div>

                <!-- Code text -->
                <div class="code-text font-mono">
                  {@html renderWordDiff(line.text, line.wordRanges, line.kind)}
                </div>
              </div>
            {/each}

            <!-- Gap below last hunk -->
            {#if hIdx === localHunks.length - 1}
              <div class="expand-row font-mono text-xs">
                <span class="expand-label flex items-center gap-1">
                  <Icon name="arrow-up-down" size={11} strokeWidth={2.2} />
                  <span>Next hidden lines</span>
                </span>
                <button
                  class="expand-btn inline-flex items-center gap-1"
                  onclick={() => expandBelowHunk(hIdx, 20)}
                >
                  <span>Show 20 lines below</span>
                  <Icon name="arrow-down" size={11} strokeWidth={2.2} />
                </button>
              </div>
            {/if}
          {/each}
        {/if}
    </div>
    {/if}

    <!-- Floating Action Bar for Range Selection -->
    {#if selectedRange && contentKind === "text"}
      <div class="floating-action-bar">
        <span class="range-badge font-mono text-xs">
          L{selectedRange.start}{selectedRange.start !== selectedRange.end ? `–L${selectedRange.end}` : ""}
          ({selectedRange.side})
        </span>

        <button class="fab-btn" onclick={handleCopyReference}>
          Copy reference
        </button>
        <button class="fab-btn" onclick={handleCopySnippet}>
          Copy snippet
        </button>

        {#if stagingEnabled && workingPart === "unstaged"}
          <span class="selection-count text-xs">
            {lineCountLabel(selectedChangeLines.length)} selected
          </span>
          <button
            class="fab-btn primary"
            disabled={!canApplySelection}
            onclick={stageSelection}
          >
            {isApplying ? "Applying…" : "Stage selection"}
          </button>
          <button
            class="fab-btn danger"
            disabled={!canApplySelection}
            onclick={requestDiscardSelection}
          >
            Discard selection
          </button>
        {:else if stagingEnabled && workingPart === "staged"}
          <span class="selection-count text-xs">
            {lineCountLabel(selectedChangeLines.length)} selected
          </span>
          <button
            class="fab-btn primary"
            disabled={!canApplySelection}
            onclick={unstageSelection}
          >
            {isApplying ? "Applying…" : "Unstage selection"}
          </button>
        {/if}

        <button
          class="fab-btn close-btn"
          onclick={() => {
            selectedRange = null;
          }}
          title="Clear selection"
        >
          <Icon name="close" size={12} strokeWidth={2.2} />
        </button>
      </div>
    {/if}
  {/if}
</div>

<Modal
  open={Boolean(discardTarget)}
  title="Discard selected lines?"
  width={460}
  closeOnEscape={!isApplying}
  closeOnOutsideClick={!isApplying}
  onclose={() => {
    if (!isApplying) discardTarget = null;
  }}
>
  {#if discardTarget}
    <p class="text-base text-strong">
      Discard {lineCountLabel(discardTarget.lines.length)} in
      <span class="discard-path font-mono">{discardTarget.path}</span>?
    </p>
    <p class="text-sm text-muted">
      Only the selected lines will be reverted. A backup is saved so you can undo
      this from Recovery for 30 days.
    </p>
  {/if}
  {#snippet footer()}
    <Button variant="default" onclick={() => (discardTarget = null)} disabled={isApplying}>
      Cancel
    </Button>
    <Button variant="danger" onclick={confirmDiscard} loading={isApplying}>
      {isApplying ? "Discarding…" : "Discard lines"}
    </Button>
  {/snippet}
</Modal>

<style>
  .diff-view-container {
    height: 100%;
    overflow: hidden;
    background-color: var(--bg);
    display: flex;
    flex-direction: column;
    position: relative;
  }

  .empty-diff {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 200px;
  }

  .truncated-card {
    margin: 40px auto;
    padding: 24px;
    background-color: var(--bg-subtle);
    border: 1px solid var(--border);
    border-radius: var(--r-lg);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    max-width: 400px;
    text-align: center;
  }

  .load-btn {
    margin-top: 8px;
    height: 28px;
    padding: 0 14px;
    background-color: var(--accent);
    color: var(--accent-fg);
    border: none;
    border-radius: var(--r-md);
    font-weight: 600;
    cursor: pointer;
  }

  .file-header {
    height: 43px;
    min-height: 43px;
    flex-shrink: 0;
    background-color: var(--bg);
    border-bottom: 1px solid #dcdfe3;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 14px;
    z-index: 10;
  }

  .file-path-group {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .file-path {
    font-family: var(--font-ui);
    font-size: 12px;
    font-weight: 400;
    color: #30363d;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .file-stats {
    display: flex;
    gap: 5px;
    flex-shrink: 0;
  }
  .stat-add {
    color: var(--success);
  }
  .stat-del {
    color: var(--danger);
  }

  /* Unified ↔ split segmented control (DESIGN.md §2.6). */
  .mode-toggle {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 2px;
    background-color: var(--bg-subtle);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    flex-shrink: 0;
  }

  .mode-btn {
    height: 22px;
    padding: 0 10px;
    border: none;
    border-radius: var(--r-sm);
    background-color: transparent;
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    transition: background-color 80ms linear, color 80ms linear;
  }
  .mode-btn:hover {
    background-color: var(--bg-muted);
    color: var(--text);
  }
  .mode-btn.active {
    background-color: var(--bg);
    color: var(--text);
    box-shadow: 0 1px 2px var(--border);
  }
  .mode-btn:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .part-switch {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 2px;
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    background-color: var(--bg-subtle);
    flex-shrink: 0;
  }
  .part-switch button {
    height: 22px;
    padding: 0 8px;
    border: none;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
  }
  .part-switch button:hover {
    color: var(--text);
  }
  .part-switch button.active {
    background-color: var(--bg);
    color: var(--text);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.12);
  }
  .part-switch button:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
  }


  .header-actions {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .header-icon-btn {
    width: 24px;
    height: 24px;
    padding: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background-color: transparent;
    border: 1px solid transparent;
    border-radius: var(--r-sm);
    color: var(--text-muted);
    cursor: pointer;
    transition: all 100ms ease;
  }
  .header-icon-btn:hover {
    background-color: var(--bg-hover, rgba(255, 255, 255, 0.08));
    color: var(--text);
    border-color: var(--border-subtle);
  }
  .header-icon-btn:active {
    background-color: var(--bg-muted);
  }

  .expand-whole-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 8px;
    background-color: var(--bg-subtle);
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-sm);
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    transition: background-color 80ms linear, color 80ms linear;
    white-space: nowrap;
  }
  .expand-whole-btn:hover {
    background-color: var(--bg-muted);
    color: var(--text);
  }

  .whole-file-chip {
    color: var(--text-faint);
    background-color: var(--bg-muted);
    border-radius: var(--r-sm);
    padding: 1px 6px;
    white-space: nowrap;
  }

  .diff-body {
    flex: 1;
    overflow: auto;
    min-width: 100%;
    outline: none;
  }

  .expand-row {
    height: 28px;
    line-height: 28px;
    background-color: var(--bg);
    display: flex;
    align-items: center;
    gap: 12px;
    padding-left: 105px;
    color: var(--text-muted);
    font-size: 10px;
    user-select: none;
    white-space: nowrap;
    min-width: 100%;
    width: max-content;
  }
  .expand-row > * {
    flex-shrink: 0;
  }

  .expand-label {
    color: var(--text-faint);
  }

  .expand-btn {
    background-color: var(--bg);
    border: none;
    border-radius: var(--r-sm);
    padding: 2px 8px;
    color: var(--accent);
    cursor: pointer;
    font-size: 10px;
    transition: background-color 80ms linear;
  }
  .expand-btn:hover {
    background-color: var(--bg-muted);
  }

  .hunk-header {
    height: 30px;
    line-height: 30px;
    padding-left: 105px;
    padding-right: 12px;
    background-color: var(--bg);
    color: #64758a;
    font-size: 11px;
    user-select: none;
    position: sticky;
    left: 0;
    min-width: 100%;
    width: max-content;
    display: flex;
    align-items: center;
    gap: 12px;
    box-sizing: border-box;
  }
  .hunk-label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hunk-btn {
    flex-shrink: 0;
    height: 20px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background-color: var(--bg-subtle);
    color: var(--text);
    font-family: var(--font-ui);
    font-size: 10.5px;
    font-weight: 500;
    cursor: pointer;
    line-height: 1;
  }
  .hunk-btn:hover:not(:disabled) {
    background-color: var(--bg-muted);
  }
  .hunk-btn:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .hunk-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .diff-body {
    user-select: text;
  }

  /* Unified Diff Lines */
  .diff-line {
    display: flex;
    align-items: stretch;
    height: 25px;
    line-height: 25px;
    font-size: 12px;
    color: #353c44;
    letter-spacing: -0.1px;
    tab-size: 4;
    min-width: 100%;
    width: max-content;
  }

  .diff-line.add {
    background-color: var(--diff-add-bg);
  }
  .diff-line.del {
    background-color: var(--diff-del-bg);
  }
  .diff-line.selected {
    background-color: var(--accent-bg) !important;
    box-shadow: inset 3px 0 0 var(--accent);
  }
  .selected-dot {
    display: inline-block;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background-color: var(--accent);
    vertical-align: middle;
  }

  .gutter-num {
    width: 38px;
    text-align: right;
    padding-right: 8px;
    font-size: 10px;
    line-height: 25px;
    color: #7b8794;
    background-color: var(--bg);
    border-right: 1px solid #dce2e9;
    user-select: none;
    flex-shrink: 0;
    cursor: pointer;
    box-sizing: border-box;
  }
  .gutter-num:hover {
    color: var(--text);
  }

  .gutter-num.old-num {
    position: sticky;
    left: 0;
    z-index: 2;
  }

  .gutter-num.new-num {
    position: sticky;
    left: 38px;
    z-index: 2;
  }

  .diff-line.add .gutter-num {
    color: #527c5c;
    border-color: #b9dfc3;
    background-color: var(--diff-gutter-add);
  }
  .diff-line.del .gutter-num {
    color: #a16a73;
    border-color: #edc1c8;
    background-color: var(--diff-gutter-del);
  }

  .sign {
    position: sticky;
    left: 76px;
    z-index: 2;
    width: 24px;
    font-size: 12px;
    line-height: 25px;
    text-align: center;
    color: var(--text-muted);
    background-color: var(--bg);
    user-select: none;
    flex-shrink: 0;
    box-sizing: border-box;
  }
  .diff-line.add .sign {
    color: var(--success);
    background-color: var(--diff-add-bg);
  }
  .diff-line.del .sign {
    color: var(--danger);
    background-color: var(--diff-del-bg);
  }

  .code-text {
    flex: 1;
    white-space: pre;
    padding-left: 4px;
    padding-right: 20px;
    font-size: 12px;
    line-height: 25px;
    color: #353c44;
  }


  /* Split (side-by-side) diff: one grid so every row's two cells stay aligned. */
  .split-grid {
    display: grid;
    grid-template-columns: minmax(320px, 1fr) minmax(320px, 1fr);
    min-width: 100%;
    align-items: stretch;
  }

  .split-grid > .hunk-header,
  .split-grid > .expand-row,
  .split-grid > .split-note {
    grid-column: 1 / -1;
    width: 100%;
    min-width: 0;
  }
  .split-grid > .hunk-header {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .split-note {
    height: 25px;
    line-height: 25px;
    padding-left: 105px;
    background-color: var(--bg);
    color: var(--text-muted);
    font-size: 11px;
  }

  .split-cell {
    display: flex;
    align-items: stretch;
    min-height: 25px;
    line-height: 25px;
    min-width: 0;
    overflow: hidden;
    font-size: 12px;
    letter-spacing: -0.1px;
    tab-size: 4;
    background-color: var(--bg);
    border-right: 1px solid var(--border-subtle);
  }
  .split-cell.new {
    border-right: none;
  }
  .split-cell.add {
    background-color: var(--diff-add-bg);
  }
  .split-cell.del {
    background-color: var(--diff-del-bg);
  }
  .split-cell.selected {
    background-color: var(--accent-bg);
  }
  .split-cell.empty {
    background-color: var(--bg-subtle);
  }

  /* The split panes scroll as a whole, so the gutters are not sticky here. */
  .split-cell .gutter-num,
  .split-cell .sign {
    position: static;
    border-color: var(--border-subtle);
  }
  .split-cell.add .gutter-num {
    color: var(--success);
    background-color: var(--diff-gutter-add);
  }
  .split-cell.del .gutter-num {
    color: var(--danger);
    background-color: var(--diff-gutter-del);
  }
  .split-cell.add .sign {
    color: var(--success);
    background-color: var(--diff-add-bg);
  }
  .split-cell.del .sign {
    color: var(--danger);
    background-color: var(--diff-del-bg);
  }

  .split-cell .code-text {
    min-width: 0;
    padding-right: 12px;
    color: var(--text);
    /* Split panes wrap instead of clipping: every character stays readable and
       both sides of a row still share one grid row. */
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .empty-hatch {
    flex: 1;
    height: 100%;
    background-image: repeating-linear-gradient(
      135deg,
      var(--bg-subtle) 0,
      var(--bg-subtle) 5px,
      var(--bg-muted) 5px,
      var(--bg-muted) 6px
    );
  }

  :global(.diff-add-word) {
    background-color: var(--diff-add-word);
    border-radius: 2px;
  }
  :global(.diff-del-word) {
    background-color: var(--diff-del-word);
    border-radius: 2px;
  }

  /* Floating Action Bar */
  .floating-action-bar {
    position: absolute;
    bottom: 24px;
    right: 28px;
    height: 36px;
    background-color: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    box-shadow: var(--shadow-pop);
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 10px;
    z-index: 100;
  }

  .range-badge {
    color: var(--text-muted);
    font-weight: 600;
    padding-right: 4px;
    border-right: 1px solid var(--border-subtle);
  }

  .fab-btn {
    height: 24px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background-color: var(--bg-subtle);
    color: var(--text);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
  }
  .fab-btn:hover {
    background-color: var(--bg-muted);
  }
  .fab-btn.primary {
    background-color: var(--accent);
    color: var(--accent-fg);
    border-color: var(--accent);
  }
  .fab-btn.danger {
    color: var(--danger);
    border-color: var(--danger);
    background-color: transparent;
  }
  .fab-btn.danger:hover:not(:disabled) {
    background-color: var(--danger-subtle, rgba(218, 54, 51, 0.1));
  }
  .fab-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .selection-count {
    color: var(--text-muted);
    white-space: nowrap;
    padding: 0 2px;
  }
  .fab-btn.ghost {
    background: none;
    border: none;
    color: var(--text-muted);
  }
  .discard-path {
    color: var(--text);
    word-break: break-all;
  }
  .fab-btn.close-btn {
    border: none;
    background: none;
    color: var(--text-muted);
    padding: 0 4px;
  }
</style>
