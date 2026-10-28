<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";
  import * as api from "../../platform/api";
  import type { BlameInfo, DefHit } from "../../platform/types";
  import { RequestTracker } from "../../platform/request";
  import { highlightCodeLines, getLanguageFromPath } from "../../design/highlight";
  import { formatRelativeTime } from "../../domain/format";
  import { scrollBehavior } from "../../design/motion";
  import { isMac } from "../../platform/keys";
  import { addToast } from "../../app/ui";
  import { registerHandler, registerDismiss, ownsPaneKeyboard } from "../../app/commands";
  import Icon from "../common/Icon.svelte";
  import FileIcon from "../common/FileIcon.svelte";
  import FilePreviewFallback from "./FilePreviewFallback.svelte";
  import Modal from "../common/Modal.svelte";
  import Button from "../common/Button.svelte";
  import { setEditorGuard } from "../../app/editorGuard";
  import { formatBytes } from "../../domain/diff-content";

  interface Props {
    repoId: string;
    path: string;
    rev?: string;
    targetLine?: number;
    paneIndex?: number;
    onopenineditor?: () => void;
    onnavigatetofile?: (path: string, line: number) => void;
  }

  let {
    repoId,
    path = $bindable(),
    rev = "HEAD",
    targetLine,
    paneIndex = 0,
    onopenineditor,
    onnavigatetofile,
  }: Props = $props();

  const MARKDOWN_PATH = /\.(md|markdown|mdx)$/i;

  let codeText = $state("");
  let rawLines = $state<string[]>([]);
  let lines = $state<string[]>([]);
  let isLoading = $state(true);
  let isWrapped = $state(false);

  // Explicit edit mode. Browsing stays read-only; the pencil is the only way
  // in, and saving is an explicit action (tick or Mod+S).
  let editing = $state(false);
  let draft = $state("");
  let editBase = $state("");
  let saving = $state(false);
  let showingWorktree = $state(false);
  let editEl = $state<HTMLTextAreaElement | null>(null);
  let highlightEl = $state<HTMLPreElement | null>(null);
  let leavePromptOpen = $state(false);
  let leaveResolve: ((ok: boolean) => void) | null = null;
  let composing = $state(false);

  let dirty = $derived(editing && draft !== editBase);

  /** Above this size, per-keystroke highlighting costs more than it is worth. */
  const EDIT_HIGHLIGHT_LIMIT = 100_000;
  let editHighlighted = $derived(editing && draft.length <= EDIT_HIGHLIGHT_LIMIT);
  let highlightedDraft = $derived.by(() =>
    editHighlighted ? highlightCodeLines(draft, language).join("\n") : ""
  );

  // Single-file preview metadata. Text files use the existing line view;
  // images render inline; binaries and oversized blobs get an honest fallback.
  let fileSize = $state<number | null>(null);
  let isBinary = $state(false);
  let isTooLarge = $state(false);
  let imageUrl = $state<string | null>(null);
  let imageMime = $state<string | null>(null);
  let imageBroken = $state(false);
  let imageDims = $state<{ w: number; h: number } | null>(null);
  let svgSourceMode = $state(false);

  let isSvg = $derived(/\.svg$/i.test(path));
  let showSvgSource = $derived(isSvg && svgSourceMode && !!codeText);
  let showImage = $derived(!isLoading && !!imageUrl && !imageBroken && !showSvgSource);
  let fallbackKind = $derived<"binary" | "too-large" | "image-error">(
    imageBroken || (!!imageMime && !imageUrl && !isTooLarge)
      ? "image-error"
      : isTooLarge
        ? "too-large"
        : "binary"
  );
  let showFallback = $derived(
    !isLoading &&
      !showImage &&
      !showSvgSource &&
      (isBinary || isTooLarge || imageBroken)
  );
  let textVisible = $derived(!showImage && !showFallback);

  // Markdown opens rendered; the header toggle flips to the raw source.
  let isMarkdown = $derived(MARKDOWN_PATH.test(path));
  let previewMode = $state(true);
  let previewEl = $state<HTMLDivElement | null>(null);
  let previewHtml = $state("");
  let markdownLib: {
    marked: typeof import("marked").marked;
    sanitize: (html: string) => string;
  } | null = null;

  $effect(() => {
    // Markdown opens rendered — unless the caller is navigating to a line, in
    // which case the raw source is the only view where that line exists.
    if (MARKDOWN_PATH.test(path)) previewMode = !targetLine;
  });

  // Rendering is lazy: marked + DOMPurify only load when a document is first
  // previewed, so they stay out of the startup bundle.
  $effect(() => {
    const text = codeText;
    const show = isMarkdown && previewMode;
    if (!show || !text) {
      previewHtml = "";
      return;
    }
    let cancelled = false;
    (async () => {
      if (!markdownLib) {
        const [markedMod, purifyMod] = await Promise.all([import("marked"), import("dompurify")]);
        markdownLib = {
          marked: markedMod.marked,
          sanitize: (html: string) => purifyMod.default.sanitize(html),
        };
      }
      if (cancelled) return;
      // Repo documents are untrusted input: parse, then strip anything active.
      const raw = markdownLib.marked.parse(text, { async: false, gfm: true }) as string;
      previewHtml = markdownLib.sanitize(raw);
    })().catch((e) => {
      console.error("Failed to render markdown:", e);
      if (!cancelled) previewHtml = "";
    });
    return () => {
      cancelled = true;
    };
  });

  // Hide images whose relative source cannot resolve inside the viewer.
  $effect(() => {
    previewHtml;
    const el = previewEl;
    if (!el) return;
    for (const img of el.querySelectorAll("img")) {
      if (img.complete && img.naturalWidth === 0) {
        img.style.display = "none";
        continue;
      }
      img.addEventListener("error", () => (img.style.display = "none"), { once: true });
    }
  });

  async function openPreviewLink(anchor: HTMLAnchorElement) {
    const href = anchor.getAttribute("href") ?? "";
    if (href.startsWith("#")) {
      const id = href.slice(1);
      previewEl?.querySelector<HTMLElement>(`[id="${CSS.escape(id)}"]`)?.scrollIntoView({
        block: "start",
        behavior: scrollBehavior(),
      });
      return;
    }
    if (!/^https?:\/\//i.test(href)) return; // relative links stay inert
    try {
      await api.openUrl(href);
    } catch (err: any) {
      addToast({ message: err?.message || "Couldn't open link", kind: "error" });
    }
  }

  // A rendered document must never navigate the webview: intercept both mouse
  // and keyboard activation and hand http(s) targets to the system browser.
  function handlePreviewActivate(e: Event) {
    const anchor = (e.target as HTMLElement | null)?.closest("a");
    if (!anchor) return;
    e.preventDefault();
    void openPreviewLink(anchor);
  }

  function handlePreviewKeydown(e: KeyboardEvent) {
    if (e.key !== "Enter" && e.key !== " ") return;
    handlePreviewActivate(e);
  }

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
  // Debounced copy of the query: typing should not rescan every line and retag
  // the whole file on each keystroke.
  let appliedQuery = $state("");
  let findTimer: ReturnType<typeof setTimeout> | undefined;
  let currentMatchIdx = $state(0);
  let findInputEl = $state<HTMLInputElement | null>(null);

  // Peek definition popover state
  let peekHits = $state<DefHit[]>([]);
  let peekWord = $state("");
  let peekOpen = $state(false);

  let language = $derived(getLanguageFromPath(path));

  // Latest-wins guards: switching files must not let a slow BlobText or blame
  // response overwrite the file the user is looking at now.
  const fileRequests = new RequestTracker();
  const blameRequests = new RequestTracker();

  onDestroy(() => {
    fileRequests.dispose();
    blameRequests.dispose();
  });

  // Find matches computation
  interface MatchPos {
    lineIdx: number;
    matchIdxInLine: number;
  }

  $effect(() => {
    const query = searchQuery;
    clearTimeout(findTimer);
    findTimer = setTimeout(() => {
      appliedQuery = query;
      currentMatchIdx = 0;
    }, 120);
    return () => clearTimeout(findTimer);
  });

  let matches = $derived.by<MatchPos[]>(() => {
    if (!appliedQuery.trim()) return [];
    const list: MatchPos[] = [];
    const qLower = appliedQuery.toLowerCase();

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
      : appliedQuery.trim()
        ? "No matches"
        : ""
  );

  let activeMatch = $derived<MatchPos | null>(
    matches.length > 0 && currentMatchIdx < matches.length
      ? matches[currentMatchIdx]
      : null
  );

  // Cache the highlighted HTML for every line. Without this the marker
  // injection (split + scan + `<mark>` wrapping) ran for every visible line on
  // every render, including unrelated hover/blame updates.
  let highlightedLines = $derived(
    lines.map((html, idx) => renderLineWithMatches(html, idx, appliedQuery, activeMatch))
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
    const request = fileRequests.begin();
    blameRequests.cancel();
    isLoading = true;
    blameCache.clear();
    activeBlame = null;
    blameLineNo = null;
    imageBroken = false;
    imageDims = null;
    svgSourceMode = false;

    try {
      const res = await api.blobPreview(repoId, rev, path);
      if (!request.isCurrent()) return;
      codeText = res.text;
      rawLines = res.text.split("\n");
      lines = res.text ? highlightCodeLines(res.text, language) : [];
      fileSize = res.size;
      isBinary = res.binary;
      isTooLarge = res.tooLarge;
      imageUrl = res.image;
      imageMime = res.mime;
    } catch (e) {
      if (!request.isCurrent()) return;
      console.error("Failed to load file:", e);
      codeText = "";
      rawLines = [];
      lines = [];
      fileSize = null;
      isBinary = false;
      isTooLarge = false;
      imageUrl = null;
      imageMime = null;
    } finally {
      if (!request.isCurrent()) return;
      isLoading = false;
      if (targetLine) {
        await tick();
        if (request.isCurrent()) navigateToLine(targetLine);
      }
    }
  }

  // ---- Edit mode -----------------------------------------------------------

  /**
   * Reset the editor when the viewer is pointed at another file. Navigation
   * surfaces ask the guard first, so this normally runs on an explicit leave.
   */
  let editTarget = $state("");
  $effect(() => {
    const key = `${repoId}\u0000${rev}\u0000${path}`;
    if (key === editTarget) return;
    editTarget = key;
    leaveResolve?.(false);
    leaveResolve = null;
    leavePromptOpen = false;
    editing = false;
    draft = "";
    editBase = "";
    showingWorktree = false;
  });

  async function startEditing() {
    if (editing || isLoading || !textVisible) return;
    try {
      // Edit the working-tree copy, never the branch blob: saving the viewed
      // revision would silently overwrite uncommitted work.
      const res = await api.blobPreview(repoId, api.WORKTREE_REV, path);
      if (res.binary || res.tooLarge) {
        addToast({ message: "This file can't be edited as text", kind: "info" });
        return;
      }
      editBase = res.text;
      draft = res.text;
      showingWorktree = true;
      editing = true;
      await tick();
      // The read view may have been scrolled, and focusing the textarea can
      // scroll its ancestors. Reset both layers after focus so the editor
      // always opens at the top with the caret in the first line.
      editEl?.focus();
      viewerBodyEl?.scrollTo({ top: 0, left: 0 });
      if (editEl) {
        editEl.scrollTop = 0;
        editEl.scrollLeft = 0;
        editEl.setSelectionRange(0, 0);
      }
    } catch (e: any) {
      addToast({
        message: e?.message || `Couldn't read ${path} from the working tree`,
        kind: "error",
      });
    }
  }

  async function saveEdit(): Promise<boolean> {
    if (!editing) return true;
    if (!dirty) {
      editing = false;
      return true;
    }
    saving = true;
    try {
      const size = await api.writeFile(repoId, path, draft);
      codeText = draft;
      rawLines = draft.split("\n");
      lines = draft ? highlightCodeLines(draft, language) : [];
      fileSize = size;
      isBinary = false;
      isTooLarge = false;
      imageUrl = null;
      imageMime = null;
      editBase = draft;
      editing = false;
      addToast({ message: `Saved ${path}`, kind: "success" });
      return true;
    } catch (e: any) {
      addToast({ message: e?.message || `Couldn't save ${path}`, kind: "error" });
      return false;
    } finally {
      saving = false;
    }
  }

  function discardEdit() {
    draft = editBase;
    editing = false;
    showingWorktree = false;
  }

  /** Keep the highlight layer under the transparent textarea in sync. */
  function syncEditScroll() {
    if (!editEl || !highlightEl) return;
    highlightEl.scrollTop = editEl.scrollTop;
    highlightEl.scrollLeft = editEl.scrollLeft;
  }

  function resolveLeave(ok: boolean) {
    leavePromptOpen = false;
    const resolve = leaveResolve;
    leaveResolve = null;
    resolve?.(ok);
  }

  /** Navigation calls this before replacing the viewer. */
  function confirmLeave(): Promise<boolean> {
    if (!editing) return Promise.resolve(true);
    if (!dirty) {
      discardEdit();
      return Promise.resolve(true);
    }
    return new Promise((resolve) => {
      leaveResolve?.(false);
      leaveResolve = resolve;
      leavePromptOpen = true;
    });
  }

  function requestCancel(): boolean {
    if (!editing) return false;
    void confirmLeave();
    return true;
  }

  async function saveFromPrompt() {
    const ok = await saveEdit();
    resolveLeave(ok);
  }

  function discardFromPrompt() {
    discardEdit();
    resolveLeave(true);
  }

  // The pane's navigation surfaces consult this guard (file tabs, pane view
  // switch, repo switch) before they unmount or replace the viewer.
  $effect(() => {
    setEditorGuard(paneIndex, editing ? { path, dirty, confirmLeave } : null);
    return () => setEditorGuard(paneIndex, null);
  });

  function navigateToLine(lineNo: number) {
    highlightedLineNo = lineNo;
    const lineIdx = lineNo - 1;
    const el = lineRowEls[lineIdx];
    if (el) {
      el.scrollIntoView({ block: "center", behavior: scrollBehavior() });
    }
    setTimeout(() => {
      if (highlightedLineNo === lineNo) {
        highlightedLineNo = null;
      }
    }, 2000);
  }

  function handleImageLoad(e: Event) {
    const img = e.currentTarget as HTMLImageElement;
    if (img.naturalWidth && img.naturalHeight) {
      imageDims = { w: img.naturalWidth, h: img.naturalHeight };
    }
  }

  function handleImageError() {
    imageBroken = true;
  }

  // Binaries, media and documents cannot be rendered in the webview; hand them
  // to the OS default application instead.
  async function openExternally() {
    try {
      await api.openWithDefaultApp(repoId, path);
      addToast({ message: `Opened '${path}' in the default application`, kind: "info" });
    } catch (e: any) {
      console.error("Failed to open file externally:", e);
      addToast({
        message: e?.message || "Couldn't open this file externally.",
        kind: "error",
      });
    }
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

    const request = blameRequests.begin();
    try {
      const b = await api.blameLine(repoId, rev, path, lineNo);
      if (!request.isCurrent() || blameLineNo !== lineNo) return;
      blameCache.set(lineNo, b);
      activeBlame = b;
    } catch {
      if (!request.isCurrent() || blameLineNo !== lineNo) return;
      activeBlame = null;
    }
  }

  function handleLineLeave() {
    blameLineNo = null;
    activeBlame = null;
    blameTooltipTop = null;
  }

  // Find in file
  async function toggleFindBar() {
    if (findOpen) {
      findOpen = false;
    } else {
      findOpen = true;
      await tick();
      findInputEl?.focus();
      findInputEl?.select();
    }
  }

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
      el.scrollIntoView({ block: "center", behavior: scrollBehavior() });
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

  function peekFromSelection(): boolean {
    const sel = window.getSelection()?.toString().trim();
    const word = sel || getWordUnderCaret();
    if (!word) return false;
    void handlePeekDef(word);
    return true;
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.altKey && (e.key === "z" || e.key === "Z")) {
      e.preventDefault();
      isWrapped = !isWrapped;
    }
  }

  // Shortcuts for the code viewer are owned by the registry so the router can
  // apply the shared typing guard and context resolution.
  $effect(() => {
    const unregister = [
      registerHandler(
        "find_in_file",
        () => {
          if (!ownsPaneKeyboard(paneIndex)) return false;
          void openFindBar();
          return true;
        },
        "files"
      ),
      registerHandler(
        "peek_def",
        () => (ownsPaneKeyboard(paneIndex) ? peekFromSelection() : false),
        "files"
      ),
      registerHandler(
        "save_file",
        () => {
          if (!ownsPaneKeyboard(paneIndex) || !editing) return false;
          void saveEdit();
          return true;
        },
        "files"
      ),
      registerDismiss(() => {
        if (!ownsPaneKeyboard(paneIndex)) return false;
        if (leavePromptOpen) {
          resolveLeave(false);
          return true;
        }
        if (editing) return requestCancel();
        let closed = false;
        if (findOpen) {
          findOpen = false;
          closed = true;
        }
        if (peekOpen) {
          peekOpen = false;
          closed = true;
        }
        if (blameLineNo != null) {
          handleLineLeave();
          closed = true;
        }
        return closed;
      }),
    ];
    return () => unregister.forEach((off) => off());
  });
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="code-viewer">
  <!-- Viewer Header -->
  <div class="viewer-header">
    <div class="header-left">
      <FileIcon filename={path} size={15} />
      <div class="path-chip font-mono text-xs" title={path}>{path}</div>
      <div class="rev-chip font-mono text-xs">{showingWorktree ? "working tree" : rev}</div>
    </div>

    <div class="actions">
      {#if editing}
        {#if dirty}
          <span class="dirty-chip text-xs">unsaved</span>
        {/if}
        <button
          type="button"
          class="viewer-icon-btn"
          class:active={dirty}
          disabled={!dirty || saving}
          onclick={() => void saveEdit()}
          title={`Save (${isMac() ? "⌘S" : "Ctrl+S"})`}
          aria-label="Save file"
        >
          <Icon name="check" size={15} />
        </button>
        <button
          type="button"
          class="viewer-icon-btn"
          onclick={() => requestCancel()}
          title="Discard changes (Esc)"
          aria-label="Discard changes"
        >
          <Icon name="close" size={14} />
        </button>
      {:else}
        {#if isMarkdown}
          <button
            type="button"
            class="viewer-icon-btn"
            onclick={() => (previewMode = !previewMode)}
            title={previewMode ? "View raw markdown" : "View rendered markdown"}
            aria-label={previewMode ? "View raw markdown" : "View rendered markdown"}
          >
            <Icon name={previewMode ? "code" : "eye"} size={15} />
          </button>
        {/if}

        {#if isSvg && imageUrl}
          <button
            type="button"
            class="viewer-icon-btn"
            class:active={svgSourceMode}
            aria-pressed={svgSourceMode}
            onclick={() => (svgSourceMode = !svgSourceMode)}
            title={svgSourceMode ? "View rendered SVG" : "View SVG source"}
            aria-label={svgSourceMode ? "View rendered SVG" : "View SVG source"}
          >
            <Icon name={svgSourceMode ? "eye" : "code"} size={15} />
          </button>
        {/if}

        {#if textVisible && !(isMarkdown && previewMode)}
          <button
            type="button"
            class="viewer-icon-btn"
            class:active={isWrapped}
            aria-pressed={isWrapped}
            onclick={() => (isWrapped = !isWrapped)}
            title="Toggle word wrap (Alt+Z) · Currently {isWrapped ? 'On' : 'Off'}"
            aria-label="Toggle word wrap"
          >
            <Icon name="wrap" size={15} />
          </button>

          <button
            type="button"
            class="viewer-icon-btn"
            class:active={findOpen}
            onclick={toggleFindBar}
            title="Find in file ({isMac() ? '⌘F' : 'Ctrl+F'})"
            aria-label="Find in file"
          >
            <Icon name="search" size={14} />
          </button>
        {/if}

        {#if textVisible && !isLoading}
          <button
            type="button"
            class="viewer-icon-btn"
            onclick={() => void startEditing()}
            title="Edit file"
            aria-label="Edit file"
          >
            <Icon name="pencil" size={14} />
          </button>
        {/if}

        <button
          type="button"
          class="viewer-icon-btn"
          onclick={onopenineditor}
          title="Open in external editor (VS Code)"
          aria-label="Open in external editor"
        >
          <Icon name="external-link" size={14} />
        </button>
      {/if}
    </div>
  </div>

  <!-- Find Bar (Floating Top-Right) -->
  {#if findOpen && textVisible}
    <div class="find-bar">
      <input
        bind:this={findInputEl}
        type="text"
        class="find-input font-mono text-xs"
        placeholder="Find..."
        aria-label="Find in file"
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
  <div class="code-body" class:edit-active={editing} bind:this={viewerBodyEl}>
    {#if isLoading}
      <div class="loading-state text-sm text-muted" role="status">Loading file…</div>
    {:else if showImage}
      <div class="image-preview">
        <div class="image-stage">
          <img
            src={imageUrl}
            alt={path}
            onload={handleImageLoad}
            onerror={handleImageError}
          />
        </div>
        <div class="image-caption text-xs">
          <span class="text-muted">{imageMime}</span>
          {#if imageDims}
            <span class="text-faint">{imageDims.w} × {imageDims.h}</span>
          {/if}
          {#if fileSize != null}
            <span class="text-faint">{formatBytes(fileSize)}</span>
          {/if}
        </div>
      </div>
    {:else if showFallback}
      <FilePreviewFallback
        {path}
        size={fileSize}
        kind={fallbackKind}
        onopenexternal={openExternally}
      />
    {:else if editing}
      <div class="edit-surface font-mono">
        {#if editHighlighted}
          <pre class="edit-highlight" aria-hidden="true" bind:this={highlightEl}>{@html highlightedDraft}</pre>
        {/if}
        <textarea
          bind:this={editEl}
          bind:value={draft}
          class="edit-area"
          class:transparent={editHighlighted && !composing}
          wrap="off"
          spellcheck="false"
          autocomplete="off"
          autocapitalize="off"
          aria-label={`Edit ${path}`}
          onscroll={syncEditScroll}
          oncompositionstart={() => (composing = true)}
          oncompositionend={() => (composing = false)}
        ></textarea>
      </div>
    {:else if isMarkdown && previewMode}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <div
        class="markdown-body"
        bind:this={previewEl}
        onclick={handlePreviewActivate}
        onkeydown={handlePreviewKeydown}
        role="document"
      >
        {@html previewHtml}
      </div>
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
              class="line-num font-mono"
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
              {@html highlightedLines[idx] ?? lineHtml}
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
            aria-label={`${hit.path} line ${hit.line}`}
            onclick={() => handleCandidateClick(hit)}
            onkeydown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                handleCandidateClick(hit);
              }
            }}
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

<Modal
  bind:open={leavePromptOpen}
  title="Unsaved changes"
  description={`${path} has edits that are not saved.`}
  width={420}
  onclose={() => resolveLeave(false)}
>
  <p class="text-sm text-muted">Save before leaving, or discard the edits.</p>
  {#snippet footer()}
    <Button variant="ghost" onclick={() => resolveLeave(false)}>Cancel</Button>
    <Button variant="danger" onclick={discardFromPrompt}>Discard</Button>
    <Button variant="primary" loading={saving} onclick={() => void saveFromPrompt()}>
      Save
    </Button>
  {/snippet}
</Modal>

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
    height: 32px;
    min-height: 32px;
    background-color: var(--bg);
    border-bottom: 1px solid var(--border);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 12px;
    flex-shrink: 0;
    z-index: 10;
  }

  .header-left {
    display: flex;
    align-items: center;
    gap: 8px;
    overflow: hidden;
    min-width: 0;
  }

  .path-chip {
    color: var(--text);
    text-overflow: ellipsis;
    overflow: hidden;
    white-space: nowrap;
    font-size: 12.5px;
  }

  .rev-chip {
    padding: 1px 6px;
    background-color: var(--bg-hover, rgba(125, 125, 125, 0.1));
    border-radius: var(--r-sm);
    color: var(--text-muted);
    font-size: 11px;
    flex-shrink: 0;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }

  .viewer-icon-btn {
    width: 26px;
    height: 26px;
    padding: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 1px solid transparent;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    transition: background 120ms ease, color 120ms ease, border-color 120ms ease;
  }
  .viewer-icon-btn:hover {
    background: rgba(125, 125, 125, 0.15);
    color: var(--text);
  }
  .viewer-icon-btn:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .viewer-icon-btn:disabled:hover {
    background: transparent;
    color: var(--text-muted);
  }
  .viewer-icon-btn.active {
    background: rgba(59, 130, 246, 0.15);
    border-color: rgba(59, 130, 246, 0.35);
    color: var(--accent);
  }
  :global([data-theme="dark"]) .viewer-icon-btn.active {
    background: rgba(59, 130, 246, 0.25);
    border-color: rgba(59, 130, 246, 0.45);
    color: var(--accent);
  }

  .dirty-chip {
    padding: 1px 6px;
    border-radius: var(--r-sm);
    background-color: var(--bg-muted);
    color: var(--accent);
    flex-shrink: 0;
  }

  /* Edit surface: a transparent textarea sits over the same highlighter the
     read view uses, so syntax colors survive while typing. Both layers share
     every metric that affects layout. */
  .edit-surface {
    position: absolute;
    inset: 0;
    overflow: hidden;
    font-size: var(--code-font-size);
    line-height: var(--code-line-height);
  }

  .edit-highlight,
  .edit-area {
    position: absolute;
    inset: 0;
    box-sizing: border-box;
    margin: 0;
    padding: 12px 16px;
    border: 0;
    line-height: inherit;
    tab-size: var(--code-tab-size, 4);
    white-space: pre;
    font-family: inherit;
    font-size: inherit;
    -webkit-font-smoothing: antialiased;
  }

  .edit-highlight {
    overflow: hidden;
    pointer-events: none;
    color: var(--text);
    background-color: var(--bg);
  }

  .edit-area {
    overflow: auto;
    resize: none;
    outline: none;
    color: var(--text);
    background-color: transparent;
  }
  .edit-area.transparent {
    color: transparent;
    caret-color: var(--text);
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
    border-radius: var(--r-sm);
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
  .code-body.edit-active {
    /* The textarea owns scrolling while editing; the outer scroller would
       otherwise offset the highlight layer. */
    overflow: hidden;
  }

  .loading-state {
    padding: 24px;
    text-align: center;
  }

  /* Image preview: a calm canvas with a subtle transparency checkerboard so
     PNG/SVG transparency is visible without looking like a gradient. */
  .image-preview {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    gap: var(--s-2);
    padding: var(--s-4);
    box-sizing: border-box;
  }

  .image-stage {
    flex: 1;
    min-height: 160px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--s-4);
    background-color: var(--bg);
    background-image:
      linear-gradient(45deg, var(--bg-muted) 25%, transparent 25%),
      linear-gradient(-45deg, var(--bg-muted) 25%, transparent 25%),
      linear-gradient(45deg, transparent 75%, var(--bg-muted) 75%),
      linear-gradient(-45deg, transparent 75%, var(--bg-muted) 75%);
    background-size: 16px 16px;
    background-position: 0 0, 0 8px, 8px -8px, -8px 0;
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-md);
    overflow: hidden;
  }

  .image-stage img {
    display: block;
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
  }

  .image-caption {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--s-3);
    flex-shrink: 0;
  }

  .lines-container {
    font-family: ui-monospace, "SF Mono", Menlo, Monaco, "Cascadia Code", "Roboto Mono", Consolas, monospace;
    font-size: var(--code-font-size);
    line-height: var(--code-line-height);
    tab-size: var(--code-tab-size, 4);
    min-width: 100%;
    width: max-content;
    -webkit-font-smoothing: antialiased;
  }

  .lines-container.wrapped {
    width: 100%;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    word-break: normal;
  }

  .code-row {
    display: flex;
    height: var(--code-line-height);
    min-width: 100%;
    width: max-content;
    /* Skip layout/paint for rows outside the viewport; `auto` remembers the
       measured size after first render so the scrollbar stays stable. */
    content-visibility: auto;
    contain-intrinsic-size: auto var(--code-line-height);
  }
  .lines-container.wrapped .code-row {
    width: 100%;
    height: auto;
    min-height: var(--code-line-height);
  }

  .code-row:hover {
    background-color: var(--bg-hover, rgba(125, 125, 125, 0.05));
  }

  .code-row.target-highlight {
    background-color: rgba(59, 130, 246, 0.15);
    border-left: 2px solid var(--accent);
    transition: background-color 0.4s ease;
  }

  .line-num {
    position: sticky;
    left: 0;
    z-index: 2;
    width: 44px;
    text-align: right;
    padding-right: 12px;
    padding-left: 8px;
    background-color: var(--bg);
    color: var(--text-faint);
    user-select: none;
    cursor: pointer;
    flex-shrink: 0;
    font-size: calc(var(--code-font-size) * 0.9);
    font-family: inherit;
    border-right: 1px solid var(--border);
    transition: color 100ms ease;
  }
  .line-num:hover {
    color: var(--text);
  }

  .line-code {
    flex: 1;
    white-space: pre;
    padding-left: 12px;
    padding-right: 20px;
    color: var(--text);
  }
  .lines-container.wrapped .line-code {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    word-break: normal;
  }

  /* VS Code Highlight Tokens inside CodeViewer */
  :global(.code-viewer .hljs-keyword) { color: var(--syn-keyword); font-weight: 500; }
  :global(.code-viewer .hljs-literal) { color: var(--syn-keyword); }
  :global(.code-viewer .hljs-symbol) { color: var(--syn-keyword); }
  :global(.code-viewer .hljs-built_in) { color: var(--syn-type); }
  :global(.code-viewer .hljs-type),
  :global(.code-viewer .hljs-class) { color: var(--syn-type); }
  :global(.code-viewer .hljs-string),
  :global(.code-viewer .hljs-addition) { color: var(--syn-string); }
  :global(.code-viewer .hljs-regexp) { color: var(--syn-regexp, var(--syn-string)); }
  :global(.code-viewer .hljs-number) { color: var(--syn-number); }
  :global(.code-viewer .hljs-comment),
  :global(.code-viewer .hljs-quote) { color: var(--syn-comment); font-style: italic; }
  :global(.code-viewer .hljs-title.function_),
  :global(.code-viewer .hljs-title) { color: var(--syn-function); }
  :global(.code-viewer .hljs-function) { color: var(--syn-function); }
  :global(.code-viewer .hljs-section) { color: var(--syn-function); font-weight: 600; }
  :global(.code-viewer .hljs-tag),
  :global(.code-viewer .hljs-name) { color: var(--syn-tag, var(--syn-keyword)); }
  :global(.code-viewer .hljs-attr),
  :global(.code-viewer .hljs-attribute) { color: var(--syn-attr, var(--syn-variable)); }
  :global(.code-viewer .hljs-variable),
  :global(.code-viewer .hljs-params),
  :global(.code-viewer .hljs-property) { color: var(--syn-variable); }
  :global(.code-viewer .hljs-meta),
  :global(.code-viewer .hljs-doctag) { color: var(--syn-meta); }

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
