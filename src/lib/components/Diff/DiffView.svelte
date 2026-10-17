<script lang="ts">
  import type { DiffFile, DiffMode, Hunk, DiffLine, NoteSide } from "../../types";
  import { getLanguageFromPath, highlightCodeLines } from "../../highlight";
  import * as api from "../../api";
  import { repos, activeRepoId } from "../../stores/repos";
  import { panes, focusedPaneIndex } from "../../stores/panes";
  import { addToast } from "../../stores/ui";
  import { settings, updateSettings } from "../../stores/settings";
  import Icon from "../common/Icon.svelte";

  interface Props {
    diffFile: DiffFile | null;
    diffMode?: DiffMode;
    ignoreWhitespace?: boolean;
    repoId?: string;
    rev?: string;
    onexpandcontext?: (oldStart: number, count: number) => void;
    onviewfile?: (path: string) => void;
    onloadforce?: () => void;
  }

  let {
    diffFile,
    diffMode = "unified",
    ignoreWhitespace = false,
    repoId,
    rev = "HEAD",
    onexpandcontext,
    onviewfile,
    onloadforce,
  }: Props = $props();

  // Active repo ID resolution
  let effectiveRepoId = $derived(
    repoId || $panes[$focusedPaneIndex]?.repoId || $activeRepoId || ""
  );

  let isTruncated = $derived(
    Boolean(diffFile && (diffFile.truncated || (diffFile as any).isTruncated))
  );

  // Local reactive copy of hunks for context expansion
  let localHunks = $state<Hunk[]>([]);

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

  async function expandAllContext() {
    if (!diffFile || !effectiveRepoId) return;
    try {
      const res = await api.blobText(effectiveRepoId, rev, diffFile.change.path);
      if (!res || !res.text) return;
      const allLines = res.text.split("\n");
      if (allLines.length === 0) return;

      const mergedLines: DiffLine[] = [];
      let currentLineNo = 1;

      for (const hunk of localHunks) {
        while (currentLineNo < hunk.newStart && currentLineNo <= allLines.length) {
          mergedLines.push({
            kind: "context",
            oldNo: currentLineNo,
            newNo: currentLineNo,
            text: allLines[currentLineNo - 1],
            wordRanges: null,
          });
          currentLineNo++;
        }

        for (const line of hunk.lines) {
          mergedLines.push(line);
          if (line.newNo != null) {
            currentLineNo = Math.max(currentLineNo, line.newNo + 1);
          }
        }
      }

      while (currentLineNo <= allLines.length) {
        mergedLines.push({
          kind: "context",
          oldNo: currentLineNo,
          newNo: currentLineNo,
          text: allLines[currentLineNo - 1],
          wordRanges: null,
        });
        currentLineNo++;
      }

      localHunks = [
        {
          oldStart: 1,
          oldLines: mergedLines.filter((l) => l.kind !== "add").length,
          newStart: 1,
          newLines: mergedLines.filter((l) => l.kind !== "del").length,
          header: "Entire file context",
          lines: mergedLines,
        },
      ];

      onexpandcontext?.(1, allLines.length);
      addToast({ message: "Expanded all context", kind: "info" });
    } catch (e) {
      console.error("Failed to expand all context:", e);
    }
  }

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
        expandAllContext();
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

<div class="diff-view-container">
  {#if !diffFile}
    <div class="empty-diff">
      <span class="text-muted">Select a file to view changes</span>
    </div>
  {:else if isTruncated}
    <div class="truncated-card">
      <span class="text-strong">Large diff</span>
      <span class="text-sm text-muted">
        This file has a very large diff that was hidden to keep performance instant.
      </span>
      <button class="load-btn" onclick={onloadforce}>Load diff anyway</button>
    </div>
  {:else}
    <!-- File Header -->
    <div class="file-header">
      <div class="file-path-group">
        <span class="file-path" title={diffFile.change.path}>{diffFile.change.path}</span>
        {#if diffFile.change.additions > 0 || diffFile.change.deletions > 0}
          <span class="file-stats text-xs font-mono">
            {#if diffFile.change.additions > 0}
              <span class="stat-add">+{diffFile.change.additions}</span>
            {/if}
            {#if diffFile.change.deletions > 0}
              <span class="stat-del">−{diffFile.change.deletions}</span>
            {/if}
          </span>
        {/if}
      </div>

    </div>

    <!-- Hunks Body -->
    <div class="diff-body">
        {#if localHunks.length === 0}
          <div class="empty-diff">
            <span class="text-muted">No changes in this file</span>
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
            <div class="hunk-header font-mono text-xs">
              {hunkLabel(hunk)}
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
                  onclick={(e) => handleLineClick(line.oldNo, "old", e)}
                  onkeydown={(e) => e.key === "Enter" && handleLineClick(line.oldNo, "old", e as any)}
                >{line.oldNo ?? ""}</div>

                <!-- New line number -->
                <div
                  class="gutter-num new-num font-mono"
                  role="button"
                  tabindex="0"
                  onclick={(e) => handleLineClick(line.newNo, "new", e)}
                  onkeydown={(e) => e.key === "Enter" && handleLineClick(line.newNo, "new", e as any)}
                >{line.newNo ?? ""}</div>

                <!-- Sign column -->
                <div class="sign font-mono">
                  {#if line.kind === "add"}+{/if}
                  {#if line.kind === "del"}−{/if}
                  {#if line.kind === "context"}&nbsp;{/if}
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

    <!-- Floating Action Bar for Range Selection -->
    {#if selectedRange}
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

  .diff-body {
    flex: 1;
    overflow: auto;
    min-width: 100%;
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
    background-color: var(--bg);
    color: #64758a;
    font-size: 11px;
    user-select: none;
    position: sticky;
    left: 0;
    min-width: 100%;
    width: max-content;
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
  .fab-btn.ghost {
    background: none;
    border: none;
    color: var(--text-muted);
  }
  .fab-btn.close-btn {
    border: none;
    background: none;
    color: var(--text-muted);
    padding: 0 4px;
  }
</style>
