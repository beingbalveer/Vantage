<script lang="ts">
  import { onMount, tick } from "svelte";
  import * as api from "../../api";
  import type { BlameInfo, DefHit } from "../../types";
  import { highlightCodeLines, getLanguageFromPath } from "../../highlight";
  import { formatRelativeTime } from "../../format";
  import { isMac } from "../../keys";
  import { addToast } from "../../stores/ui";
  import Icon from "../common/Icon.svelte";

  interface Props {
    repoId: string;
    path: string;
    rev?: string;
    targetLine?: number;
    onopenineditor?: () => void;
    onnavigatetofile?: (path: string, line: number) => void;
  }

  let {
    repoId,
    path = $bindable(),
    rev = "HEAD",
    targetLine,
    onopenineditor,
    onnavigatetofile,
  }: Props = $props();

  let codeText = $state("");
  let rawLines = $state<string[]>([]);
  let lines = $state<string[]>([]);
  let isLoading = $state(true);
  let isWrapped = $state(false);

  // Line elements for scrolling
  let lineRowEls: Record<number, HTMLElement> = {};
  let viewerBodyEl = $state<HTMLElement | null>(null);

  // Target line highlight
  let highlightedLineNo = $state<number | null>(null);

  // Blame-lite state
  let activeBlame = $state<BlameInfo | null>(null);
  let blameLineNo = $state<number | null>(null);
  let blameTooltipTop = $state<number | null>(null);
  const blameCache = new Map<number, BlameInfo>();

  // Find in File state
  let findOpen = $state(false);
  let searchQuery = $state("");
  let currentMatchIdx = $state(0);
  let findInputEl = $state<HTMLInputElement | null>(null);

  // Peek definition popover state
  let peekHits = $state<DefHit[]>([]);
  let peekWord = $state("");
  let peekOpen = $state(false);

  let language = $derived(getLanguageFromPath(path));

  // Find matches computation
  interface MatchPos {
    lineIdx: number;
    matchIdxInLine: number;
  }

  let matches = $derived.by<MatchPos[]>(() => {
    if (!searchQuery.trim()) return [];
    const list: MatchPos[] = [];
    const qLower = searchQuery.toLowerCase();

    for (let l = 0; l < rawLines.length; l++) {
      const lineLower = rawLines[l].toLowerCase();
      let pos = 0;
      let matchIdx = 0;
      let found = lineLower.indexOf(qLower, pos);
      while (found !== -1) {
        list.push({ lineIdx: l, matchIdxInLine: matchIdx });
        matchIdx++;
        pos = found + qLower.length;
        found = lineLower.indexOf(qLower, pos);
      }
    }
    return list;
  });

  let matchCountText = $derived(
    matches.length > 0
      ? `${currentMatchIdx + 1} of ${matches.length}`
      : searchQuery.trim()
        ? "No matches"
        : ""
  );

  let activeMatch = $derived<MatchPos | null>(
    matches.length > 0 && currentMatchIdx < matches.length
      ? matches[currentMatchIdx]
      : null
  );

  $effect(() => {
    if (repoId && path) {
      loadFile();
    }
  });

  $effect(() => {
    if (targetLine && !isLoading) {
      navigateToLine(targetLine);
    }
  });

  async function loadFile() {
    isLoading = true;
    blameCache.clear();
    activeBlame = null;
    blameLineNo = null;

    try {
      const res = await api.blobText(repoId, rev, path);
      codeText = res.text;
      rawLines = res.text.split("\n");
      lines = highlightCodeLines(res.text, language);
    } catch (e) {
      console.error("Failed to load file:", e);
      codeText = "";
      rawLines = [];
      lines = [];
    } finally {
      isLoading = false;
      if (targetLine) {
        await tick();
        navigateToLine(targetLine);
      }
    }
  }

  function navigateToLine(lineNo: number) {
    highlightedLineNo = lineNo;
    const lineIdx = lineNo - 1;
    const el = lineRowEls[lineIdx];
    if (el) {
      el.scrollIntoView({ block: "center", behavior: "smooth" });
    }
    setTimeout(() => {
      if (highlightedLineNo === lineNo) {
        highlightedLineNo = null;
      }
    }, 2000);
  }

  // Blame-lite hover
  async function handleLineHover(lineIdx: number, e: MouseEvent) {
    const lineNo = lineIdx + 1;
    blameLineNo = lineNo;

    const target = e.currentTarget as HTMLElement | null;
    if (target && viewerBodyEl) {
      const targetRect = target.getBoundingClientRect();
      const bodyRect = viewerBodyEl.getBoundingClientRect();
      blameTooltipTop = targetRect.top - bodyRect.top + viewerBodyEl.scrollTop;
    }

    if (blameCache.has(lineNo)) {
      activeBlame = blameCache.get(lineNo)!;
      return;
    }

    try {
      const b = await api.blameLine(repoId, rev, path, lineNo);
      blameCache.set(lineNo, b);
      if (blameLineNo === lineNo) {
        activeBlame = b;
      }
    } catch {
      if (blameLineNo === lineNo) {
        activeBlame = null;
      }
    }
  }

  function handleLineLeave() {
    blameLineNo = null;
    activeBlame = null;
    blameTooltipTop = null;
  }

  // Find in file
  async function openFindBar() {
    findOpen = true;
    await tick();
    findInputEl?.focus();
    findInputEl?.select();
  }

  function nextMatch() {
    if (matches.length === 0) return;
    currentMatchIdx = (currentMatchIdx + 1) % matches.length;
    scrollToActiveMatch();
  }

  function prevMatch() {
    if (matches.length === 0) return;
    currentMatchIdx = (currentMatchIdx - 1 + matches.length) % matches.length;
    scrollToActiveMatch();
  }

  function scrollToActiveMatch() {
    const match = activeMatch;
    if (!match) return;
    const el = lineRowEls[match.lineIdx];
    if (el) {
      el.scrollIntoView({ block: "center", behavior: "smooth" });
    }
  }

  function renderLineWithMatches(
    html: string,
    lineIdx: number,
    query: string,
    currentMatch: MatchPos | null
  ): string {
    if (!query.trim()) return html;
    const parts = html.split(/(<[^>]+>)/g);
    let matchIdxInLine = 0;
    const qLower = query.toLowerCase();

    return parts
      .map((part) => {
        if (part.startsWith("<")) return part;
        let out = "";
        let pos = 0;
        let partLower = part.toLowerCase();
        let idx = partLower.indexOf(qLower, pos);

        while (idx !== -1) {
          out += part.slice(pos, idx);
          const matchedText = part.slice(idx, idx + query.length);
          const isCurrent =
            currentMatch !== null &&
            currentMatch.lineIdx === lineIdx &&
            currentMatch.matchIdxInLine === matchIdxInLine;
          const cls = isCurrent ? "find-match find-match-current" : "find-match";
          out += `<mark class="${cls}">${matchedText}</mark>`;
          matchIdxInLine++;
          pos = idx + query.length;
          idx = partLower.indexOf(qLower, pos);
        }
        out += part.slice(pos);
        return out;
      })
      .join("");
  }

  // Peek definition
  async function handlePeekDef(word: string) {
    if (!word) return;
    peekWord = word;
    try {
      const ext = path.split(".").pop();
      const hits = await api.grepDefs(repoId, rev, word, ext);
      peekHits = hits;
      peekOpen = hits.length > 0;
      if (hits.length === 0) {
        addToast({ message: `No definitions found for '${word}'`, kind: "info" });
      }
    } catch {
      peekHits = [];
      peekOpen = false;
    }
  }

  function getWordUnderCaret(): string {
    const selection = window.getSelection();
    if (!selection || selection.rangeCount === 0) return "";
    const range = selection.getRangeAt(0);
    const node = range.startContainer;
    const offset = range.startOffset;
    if (!node || node.nodeType !== Node.TEXT_NODE || !node.textContent) return "";

    const text = node.textContent;
    let start = offset;
    let end = offset;
    while (start > 0 && /[a-zA-Z0-9_]/.test(text[start - 1])) start--;
    while (end < text.length && /[a-zA-Z0-9_]/.test(text[end])) end++;
    return text.slice(start, end);
  }

  function handleCodeClick(e: MouseEvent) {
    if (e.altKey) {
      e.preventDefault();
      const sel = window.getSelection()?.toString().trim();
      const word = sel || getWordUnderCaret();
      if (word) handlePeekDef(word);
    }
  }

  function handleCandidateClick(hit: DefHit) {
    peekOpen = false;
    if (hit.path === path) {
      navigateToLine(hit.line);
    } else if (onnavigatetofile) {
      onnavigatetofile(hit.path, hit.line);
    } else {
      path = hit.path;
      loadFile().then(() => {
        navigateToLine(hit.line);
      });
    }
  }

  function handleGlobalKeydown(e: KeyboardEvent) {
    const isMacPlatform = isMac();
    const mod = isMacPlatform ? e.metaKey : e.ctrlKey;

    if (mod && (e.key === "f" || e.key === "F")) {
      e.preventDefault();
      openFindBar();
    } else if (e.key === "F12") {
      e.preventDefault();
      const sel = window.getSelection()?.toString().trim();
      const word = sel || getWordUnderCaret();
      if (word) handlePeekDef(word);
    } else if (e.key === "Escape") {
      if (findOpen) {
        findOpen = false;
      }
      if (peekOpen) {
        peekOpen = false;
      }
      handleLineLeave();
    }
  }
</script>

<svelte:window onkeydown={handleGlobalKeydown} />

<div class="code-viewer">
  <!-- Viewer Header -->
  <div class="viewer-header">
    <div class="header-left">
      <div class="path-chip font-mono text-xs" title={path}>{path}</div>
      <div class="rev-chip font-mono text-xs">{rev}</div>
    </div>

    <div class="actions">
      <button
        class="viewer-btn"
        class:active={isWrapped}
        onclick={() => (isWrapped = !isWrapped)}
        title="Toggle word wrap"
      >
        Wrap
      </button>
      <button
        class="viewer-btn"
        onclick={openFindBar}
        title="Find in file ({isMac() ? '⌘F' : 'Ctrl+F'})"
      >
        Find
      </button>
      <button class="viewer-btn" onclick={onopenineditor} title="Open in editor">
        Open in editor
      </button>
    </div>
  </div>

  <!-- Find Bar (Floating Top-Right) -->
  {#if findOpen}
    <div class="find-bar">
      <input
        bind:this={findInputEl}
        type="text"
        class="find-input font-mono text-xs"
        placeholder="Find..."
        bind:value={searchQuery}
        onkeydown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            if (e.shiftKey) prevMatch();
            else nextMatch();
          } else if (e.key === "Escape") {
            findOpen = false;
          }
        }}
      />
      {#if matchCountText}
        <span class="match-count font-mono text-xs">{matchCountText}</span>
      {/if}
      <button class="find-btn" onclick={prevMatch} title="Previous match (Shift+Enter)">
        <Icon name="chevron-up" size={13} strokeWidth={2.2} />
      </button>
      <button class="find-btn" onclick={nextMatch} title="Next match (Enter)">
        <Icon name="chevron-down" size={13} strokeWidth={2.2} />
      </button>
      <button class="find-btn close" onclick={() => (findOpen = false)} title="Close (Esc)">
        <Icon name="close" size={12} strokeWidth={2.2} />
      </button>
    </div>
  {/if}

  <!-- Code Body -->
  <div class="code-body" bind:this={viewerBodyEl}>
    {#if isLoading}
      <div class="loading-state text-sm text-muted">Loading file…</div>
    {:else}
      <div class="lines-container font-mono" class:wrapped={isWrapped}>
        {#each lines as lineHtml, idx (idx)}
          {@const isTarget = highlightedLineNo === idx + 1}
          <div
            class="code-row"
            class:target-highlight={isTarget}
            bind:this={lineRowEls[idx]}
          >
            <!-- Gutter / Line Number -->
            <div
              class="line-num text-xs font-mono"
              role="presentation"
              onmouseenter={(e) => handleLineHover(idx, e)}
              onmouseleave={handleLineLeave}
              title={activeBlame && blameLineNo === idx + 1
                ? `${activeBlame.sha.slice(0, 7)} · ${activeBlame.author} · ${formatRelativeTime(activeBlame.date)} · ${activeBlame.subject}`
                : ""}
            >
              {idx + 1}
            </div>

            <!-- Code text -->
            <div
              class="line-code"
              role="presentation"
              onclick={handleCodeClick}
              ondblclick={() => {
                const sel = window.getSelection()?.toString().trim();
                if (sel) handlePeekDef(sel);
              }}
            >
              {@html renderLineWithMatches(lineHtml, idx, searchQuery, activeMatch)}
            </div>
          </div>
        {/each}
      </div>

      <!-- Blame Tooltip -->
      {#if activeBlame && blameLineNo != null && blameTooltipTop != null}
        <div class="blame-tooltip font-mono text-xs" style="top: {blameTooltipTop}px;">
          <span class="blame-sha">{activeBlame.sha.slice(0, 7)}</span>
          <span class="blame-sep">·</span>
          <span class="blame-author">{activeBlame.author}</span>
          <span class="blame-sep">·</span>
          <span class="blame-date">{formatRelativeTime(activeBlame.date)}</span>
          <span class="blame-sep">·</span>
          <span class="blame-subject">{activeBlame.subject}</span>
        </div>
      {/if}
    {/if}
  </div>

  <!-- Peek Definition Popover -->
  {#if peekOpen}
    <div class="peek-popover">
      <div class="peek-header">
        <span class="text-strong text-sm">
          Definitions of <code class="peek-word">{peekWord}</code> ({peekHits.length})
        </span>
        <button class="close-btn" onclick={() => (peekOpen = false)} title="Close (Esc)">
          <Icon name="close" size={12} strokeWidth={2.2} />
        </button>
      </div>

      <div class="peek-list">
        {#each peekHits as hit}
          <div
            class="peek-item"
            role="button"
            tabindex="0"
            onclick={() => handleCandidateClick(hit)}
            onkeydown={(e) => e.key === "Enter" && handleCandidateClick(hit)}
          >
            <div class="peek-path font-mono text-xs">{hit.path}:{hit.line}</div>
            <pre class="peek-code font-mono text-xs">{hit.lineText}</pre>
          </div>
        {/each}
      </div>

      <div class="peek-footer font-mono text-xs text-faint">
        Text-based match
      </div>
    </div>
  {/if}
</div>

<style>
  .code-viewer {
    display: flex;
    flex-direction: column;
    height: 100%;
    background-color: var(--bg);
    position: relative;
    overflow: hidden;
  }

  .viewer-header {
    height: 43px;
    min-height: 43px;
    background-color: var(--bg);
    border-bottom: 1px solid #dcdfe3;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 14px;
    flex-shrink: 0;
    z-index: 10;
  }

  .header-left {
    display: flex;
    align-items: center;
    gap: 8px;
    overflow: hidden;
  }

  .path-chip {
    color: var(--text);
    text-overflow: ellipsis;
    overflow: hidden;
    white-space: nowrap;
  }

  .rev-chip {
    padding: 1px 6px;
    background-color: var(--bg-muted);
    border-radius: var(--r-sm);
    color: var(--text-muted);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .viewer-btn {
    height: 24px;
    padding: 0 8px;
    font-size: 11px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background-color: var(--bg);
    color: var(--text);
    cursor: pointer;
  }
  .viewer-btn:hover {
    background-color: var(--bg-muted);
  }
  .viewer-btn.active {
    background-color: var(--accent-bg);
    border-color: var(--accent);
    color: var(--accent);
  }

  /* Find Bar */
  .find-bar {
    position: absolute;
    top: 42px;
    right: 16px;
    width: 290px;
    height: 32px;
    background-color: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    box-shadow: var(--shadow-pop);
    display: flex;
    align-items: center;
    padding: 0 6px;
    gap: 4px;
    z-index: 100;
  }

  .find-input {
    flex: 1;
    height: 22px;
    border: none;
    outline: none;
    background: transparent;
    color: var(--text);
    padding: 0 4px;
  }

  .match-count {
    color: var(--text-muted);
    font-size: 11px;
    white-space: nowrap;
  }

  .find-btn {
    width: 20px;
    height: 20px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: none;
    border: none;
    border-radius: var(--r-sm);
    color: var(--text-muted);
    cursor: pointer;
    font-size: 10px;
  }
  .find-btn:hover {
    background-color: var(--bg-muted);
    color: var(--text);
  }
  .find-btn.close {
    font-size: 11px;
  }

  :global(.find-match) {
    background-color: var(--warning-bg);
    border-radius: 2px;
    color: inherit;
  }
  :global(.find-match-current) {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
    font-weight: 600;
  }

  .code-body {
    flex: 1;
    overflow: auto;
    position: relative;
  }

  .loading-state {
    padding: 24px;
    text-align: center;
  }

  .lines-container {
    font-size: 12.5px;
    line-height: 20px;
    tab-size: 4;
    min-width: 100%;
    width: max-content;
  }

  .lines-container.wrapped {
    width: 100%;
    white-space: pre-wrap;
    word-break: break-all;
  }

  .code-row {
    display: flex;
    height: 20px;
    min-width: 100%;
    width: max-content;
  }
  .lines-container.wrapped .code-row {
    width: 100%;
    height: auto;
    min-height: 20px;
  }

  .code-row.target-highlight {
    background-color: var(--accent-bg);
    transition: background-color 0.4s ease;
  }

  .line-num {
    position: sticky;
    left: 0;
    z-index: 2;
    width: 36px;
    text-align: right;
    padding-right: 6px;
    background-color: var(--bg-inset);
    color: var(--text-faint);
    user-select: none;
    cursor: pointer;
    flex-shrink: 0;
  }
  .line-num:hover {
    color: var(--text);
  }

  .line-code {
    flex: 1;
    white-space: pre;
    padding-left: 8px;
    padding-right: 16px;
    color: var(--text);
  }
  .lines-container.wrapped .line-code {
    white-space: pre-wrap;
    word-break: break-all;
  }

  /* Blame-lite Tooltip */
  .blame-tooltip {
    position: absolute;
    left: 54px;
    background-color: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    box-shadow: var(--shadow-pop);
    padding: 2px 8px;
    color: var(--text);
    z-index: 50;
    pointer-events: none;
    white-space: nowrap;
    display: flex;
    align-items: center;
    gap: 4px;
    height: 20px;
    line-height: 20px;
  }

  .blame-sha {
    color: var(--accent);
  }
  .blame-sep {
    color: var(--text-faint);
  }
  .blame-author {
    color: var(--text-strong);
  }
  .blame-date {
    color: var(--text-muted);
  }
  .blame-subject {
    color: var(--text);
    max-width: 300px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Peek Popover */
  .peek-popover {
    position: absolute;
    bottom: 20px;
    left: 60px;
    right: 60px;
    max-height: 320px;
    background-color: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    box-shadow: var(--shadow-pop);
    display: flex;
    flex-direction: column;
    z-index: 100;
  }

  .peek-header {
    height: 32px;
    padding: 0 10px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    border-bottom: 1px solid var(--border-subtle);
    background-color: var(--bg-subtle);
  }

  .peek-word {
    color: var(--accent);
  }

  .close-btn {
    background: none;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
  }

  .peek-list {
    overflow-y: auto;
    padding: 6px;
    max-height: 240px;
  }

  .peek-item {
    padding: 6px 8px;
    border-radius: var(--r-sm);
    cursor: pointer;
  }
  .peek-item:hover {
    background-color: var(--bg-muted);
  }

  .peek-path {
    color: var(--accent);
  }

  .peek-code {
    color: var(--text);
    margin-top: 2px;
    white-space: pre-wrap;
  }

  .peek-footer {
    height: 24px;
    padding: 0 10px;
    display: flex;
    align-items: center;
    border-top: 1px solid var(--border-subtle);
    background-color: var(--bg-subtle);
  }
</style>
