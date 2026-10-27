<script lang="ts">
  import { isFeatureEnabled } from "../../config/featureFlags.svelte";
  import type { FileChange } from "../../platform/types";
  import type { NormalizedError } from "../../domain/asyncState";
  import { computeVirtualWindow, splitPath } from "../../domain/format";
  import { addToast } from "../../app/ui";
  import { registerDismiss } from "../../app/commands";
  import Icon from "../common/Icon.svelte";
  import Skeleton from "../common/Skeleton.svelte";
  import EmptyState from "../common/EmptyState.svelte";
  import Banner from "../common/Banner.svelte";

  interface Props {
    files: FileChange[];
    selectedFile: string | null;
    isUncommitted: boolean;
    checkedPaths: string[];
    showCommitControls?: boolean;
    focusDiff?: boolean;
    hasCommitDetails?: boolean;
    showCommitDetails?: boolean;
    loading?: boolean;
    pending?: boolean;
    error?: NormalizedError | null;
    emptyTitle?: string;
    emptyDescription?: string;
    onretry?: () => void;
    menuOpenPath?: string | null;
    expandedFile?: string | null;
    onbrowseall?: () => void;
    oncommitrequest?: () => void;
    ondiscardrequest?: () => void;
    ontogglecommitdetails?: () => void;
    ontogglefocusdiff?: () => void;
    onselectfile?: (path: string) => void;
    ontogglepath?: (path: string) => void;
    ontoggleall?: (select: boolean) => void;
    onfilemenu?: (file: FileChange, pos: { x: number; y: number }) => void;
    onexpandwholefile?: (path: string) => void;
    oncollapsecontext?: (path: string) => void;
  }

  let {
    files,
    selectedFile,
    isUncommitted,
    checkedPaths,
    showCommitControls = false,
    focusDiff = false,
    hasCommitDetails = false,
    showCommitDetails = false,
    loading = false,
    pending = false,
    error = null,
    emptyTitle = "No changed files",
    emptyDescription = "",
    onretry,
    menuOpenPath = null,
    expandedFile = null,
    ontogglefocusdiff,
    onselectfile,
    ontogglepath,
    ontoggleall,
    onbrowseall,
    oncommitrequest,
    ondiscardrequest,
    ontogglecommitdetails,
    onfilemenu,
    onexpandwholefile,
    oncollapsecontext,
  }: Props = $props();

  // A Set keeps bulk select/deselect linear; the previous `includes` inside the
  // row template made it O(files × selected).
  let checkedSet = $derived(new Set(checkedPaths));

  let allChecked = $derived(
    files.length > 0 && files.every((f) => checkedSet.has(f.path))
  );

  let someChecked = $derived(
    files.length > 0 && !allChecked && files.some((f) => checkedSet.has(f.path))
  );

  // Row virtualization: changed-file lists routinely reach thousands of rows.
  // Rows are a fixed 36px, so a windowed render keeps the DOM small without
  // changing layout or keyboard semantics.
  const ROW_HEIGHT = 36;
  const OVERSCAN = 10;
  let rowsEl = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let viewportHeight = $state(0);

  function handleRowsScroll(e: Event) {
    scrollTop = (e.currentTarget as HTMLDivElement).scrollTop;
  }

  $effect(() => {
    const el = rowsEl;
    if (!el) return;
    viewportHeight = el.clientHeight;
    const observer = new ResizeObserver(() => {
      viewportHeight = el.clientHeight;
    });
    observer.observe(el);
    return () => observer.disconnect();
  });

  let virtualWindow = $derived(
    computeVirtualWindow(files.length, scrollTop, viewportHeight, ROW_HEIGHT, OVERSCAN)
  );
  let visibleFiles = $derived(
    files.slice(virtualWindow.startIndex, virtualWindow.endIndex)
  );
  let padTop = $derived(virtualWindow.startIndex * ROW_HEIGHT);
  let padBottom = $derived(
    Math.max(0, virtualWindow.totalHeight - virtualWindow.endIndex * ROW_HEIGHT)
  );

  // Keyboard navigation can move the selection outside the rendered window.
  // Scroll it into view; the resulting scroll event updates the window.
  $effect(() => {
    const el = rowsEl;
    if (!el || !selectedFile) return;
    const index = files.findIndex((f) => f.path === selectedFile);
    if (index < 0) return;
    const top = index * ROW_HEIGHT;
    const bottom = top + ROW_HEIGHT;
    if (top < el.scrollTop) {
      el.scrollTop = top;
    } else if (bottom > el.scrollTop + el.clientHeight) {
      el.scrollTop = bottom - el.clientHeight;
    }
  });

  // Keep the count honest: an empty list that is still loading (or failed) is
  // not "0 changed files". (ALPH-150)
  let headerTitle = $derived(
    files.length === 0 && (loading || pending || error)
      ? ""
      : `${files.length} changed ${files.length === 1 ? "file" : "files"}`
  );

  interface FileTriage {
    kind: "dependencies" | "config" | "testsRemoved" | "secrets" | "bigFile" | "binary";
    severity: "info" | "caution" | "warn";
    label: string;
    detail: string;
  }

  function getFileTriage(file: FileChange): FileTriage | null {
    const p = file.path.toLowerCase();
    const name = file.path.split("/").pop() || "";
    const nameLower = name.toLowerCase();

    if (
      nameLower === ".env" ||
      (nameLower.startsWith(".env.") &&
        !nameLower.includes("example") &&
        !nameLower.includes("sample") &&
        !nameLower.includes("template"))
    ) {
      return { kind: "secrets", severity: "warn", label: "Secrets", detail: "Sensitive environment file" };
    }

    const isTest =
      p.includes("/test/") ||
      p.includes("/tests/") ||
      p.includes("/__tests__/") ||
      nameLower.includes("test") ||
      nameLower.includes("spec");

    if (isTest && (file.status === "deleted" || (file.deletions >= 10 && file.deletions > file.additions))) {
      return {
        kind: "testsRemoved",
        severity: "warn",
        label: "TestsRemoved",
        detail: file.status === "deleted" ? "Test file deleted" : `−${file.deletions} test lines removed`,
      };
    }

    const depFiles = [
      "package.json", "package-lock.json", "yarn.lock", "pnpm-lock.yaml", "bun.lockb",
      "cargo.toml", "cargo.lock", "go.mod", "go.sum", "pyproject.toml", "poetry.lock",
      "gemfile", "gemfile.lock", "composer.json", "composer.lock", "pom.xml",
    ];
    if (
      depFiles.includes(nameLower) ||
      (nameLower.startsWith("requirements") && nameLower.endsWith(".txt")) ||
      nameLower.startsWith("pipfile") ||
      nameLower.startsWith("build.gradle")
    ) {
      return { kind: "dependencies", severity: "info", label: "Dependencies", detail: `${name} dependency file` };
    }

    if (
      p.includes(".github/workflows") ||
      nameLower.startsWith("dockerfile") ||
      nameLower.startsWith("docker-compose") ||
      p.includes("/migrations/") ||
      nameLower.endsWith(".tf") ||
      nameLower.endsWith(".tfvars") ||
      nameLower === ".gitlab-ci.yml" ||
      p.includes("/k8s/") ||
      p.includes("/helm/") ||
      (nameLower.startsWith("nginx") && nameLower.endsWith(".conf"))
    ) {
      return { kind: "config", severity: "caution", label: "Config", detail: "Infrastructure or workflow configuration" };
    }

    if (file.binary) {
      return { kind: "binary", severity: "info", label: "Binary", detail: "Binary file" };
    }

    if (file.additions + file.deletions > 500) {
      return { kind: "bigFile", severity: "caution", label: "BigFile", detail: `+${file.additions} −${file.deletions} lines` };
    }

    return null;
  }

  function statusGlyph(status: FileChange["status"]): string {
    switch (status) {
      case "modified":
      case "typeChanged":
        return "•";
      case "deleted":
        return "−";
      case "renamed":
        return "→";
      case "conflicted":
        return "!";
      default:
        return "+";
    }
  }

  let showOptions = $state(false);

  function openFileMenu(file: FileChange, x: number, y: number) {
    onfilemenu?.(file, { x, y });
  }

  function handleRowKeydown(e: KeyboardEvent, file: FileChange) {
    // Keyboard-accessible alternative to right-click (Shift+F10 / Menu key).
    if (onfilemenu && (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10"))) {
      e.preventDefault();
      e.stopPropagation();
      const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
      openFileMenu(file, rect.left + 16, rect.top + 16);
      return;
    }
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onselectfile?.(file.path);
    }
  }

  function handleRowContextMenu(e: MouseEvent, file: FileChange) {
    if (!onfilemenu) return;
    e.preventDefault();
    e.stopPropagation();
    openFileMenu(file, e.clientX, e.clientY);
  }

  function handleFileMenuButton(e: MouseEvent, file: FileChange) {
    e.preventDefault();
    e.stopPropagation();
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    openFileMenu(file, rect.left, rect.bottom + 4);
  }

  $effect(() => {
    const unregister = registerDismiss(() => {
      if (!showOptions) return false;
      showOptions = false;
      return true;
    });
    return unregister;
  });
</script>

<div class="file-list-container">
  <div class="file-list-header">
    {#if isUncommitted && files.length > 0}
      <input
        type="checkbox"
        checked={allChecked}
        indeterminate={someChecked}
        aria-label="{allChecked ? 'Deselect' : 'Select'} all files for commit"
        title="{allChecked ? 'Deselect' : 'Select'} all files"
        class="header-cb"
        onchange={(e) => {
          const target = e.currentTarget as HTMLInputElement;
          ontoggleall?.(target.checked);
        }}
      />
    {/if}

    <span class="header-title">
      {headerTitle}
    </span>

    <div class="header-actions">
      <button
        class="files-more"
        type="button"
        aria-label="File actions"
        aria-haspopup="menu"
        aria-expanded={showOptions}
        title="File actions"
        onclick={(e) => {
          e.stopPropagation();
          showOptions = !showOptions;
        }}
      >
        <Icon name="more-horizontal" size={14} />
      </button>
    </div>

    {#if showOptions}
      <div
        class="options-backdrop"
        role="presentation"
        onclick={() => (showOptions = false)}
      ></div>
      <div class="files-options" role="menu" tabindex="-1">
        {#if isUncommitted}
          <button
            role="menuitem"
            onclick={() => {
              ontoggleall?.(!allChecked);
              showOptions = false;
            }}
          >
            {allChecked ? "Deselect" : "Select"} all files for commit
          </button>
          <button
            role="menuitem"
            onclick={() => {
              oncommitrequest?.();
              showOptions = false;
            }}
          >
            Commit selected files…
          </button>
          <button
            role="menuitem"
            class="danger"
            disabled={checkedPaths.length === 0}
            onclick={() => {
              ondiscardrequest?.();
              showOptions = false;
            }}
          >
            Discard selected changes…
          </button>
        {:else if hasCommitDetails}
          <button
            role="menuitem"
            onclick={() => {
              ontogglecommitdetails?.();
              showOptions = false;
            }}
          >
            {showCommitDetails ? "Hide" : "Show"} commit details
          </button>
        {/if}
        <button
          role="menuitem"
          onclick={() => {
            ontogglefocusdiff?.();
            showOptions = false;
          }}
        >
          {focusDiff ? "Show commit list" : "Focus diff"}
        </button>
        {#if onbrowseall}
          <button
            role="menuitem"
            onclick={() => {
              onbrowseall?.();
              showOptions = false;
            }}
          >
            Browse project files
          </button>
        {/if}
      </div>
    {/if}
  </div>

  {#if error}
    <div class="file-banner">
      <Banner kind="error" message={error.message} {error} retryLabel="Retry" {onretry} />
    </div>
  {/if}

  <div
    class="file-rows"
    class:pending={loading && files.length > 0}
    aria-busy={loading || pending}
    bind:this={rowsEl}
    onscroll={handleRowsScroll}
  >
    {#if loading && files.length === 0}
      <Skeleton rows={6} height={36} shape="file" label="Loading changed files" />
    {:else if files.length === 0 && !pending && !error}
      <EmptyState title={emptyTitle} description={emptyDescription} />
    {/if}

    {#if padTop > 0}
      <div class="virtual-pad" style="height: {padTop}px" aria-hidden="true"></div>
    {/if}

    {#each visibleFiles as file (file.path + "-" + file.status + "-" + file.additions + "-" + file.deletions)}
      {@const { dir, name } = splitPath(file.path)}
      {@const isSelected = selectedFile === file.path}
      {@const isChecked = checkedSet.has(file.path)}
      {@const triage = getFileTriage(file)}

      <div
        class="file-row {isSelected ? 'selected' : ''} {menuOpenPath === file.path ? 'menu-open' : ''}"
        class:conflicted={isFeatureEnabled("ALPH_156_CONFLICT_FLOW") && file.status === "conflicted"}
        data-file-path={file.path}
        onclick={() => onselectfile?.(file.path)}
        onkeydown={(e) => handleRowKeydown(e, file)}
        oncontextmenu={(e) => handleRowContextMenu(e, file)}
        role="button"
        tabindex="0"
        title="{file.path} ({file.status === 'conflicted' ? 'unmerged' : file.status}{file.additions > 0 ? ', +' + file.additions : ''}{file.deletions > 0 ? ', −' + file.deletions : ''}){triage ? ' · ' + triage.label + ': ' + triage.detail : ''}"
      >
        {#if isUncommitted}
          <input
            type="checkbox"
            checked={isChecked}
            aria-label="Select {file.path} for commit"
            onclick={(e) => e.stopPropagation()}
            onkeydown={(e) => e.stopPropagation()}
            onchange={() => ontogglepath?.(file.path)}
            class="row-cb"
          />
        {/if}

        <div class="path-wrap">
          {#if dir}
            <span class="dir-text">{dir}/</span>
          {/if}
          <span class="file-name">{name}</span>
        </div>

        <div class="row-right">
          {#if onfilemenu}
            <button
              type="button"
              class="row-menu-btn"
              aria-label="File actions"
              aria-haspopup="menu"
              aria-expanded={menuOpenPath === file.path}
              title="File actions"
              onclick={(e) => handleFileMenuButton(e, file)}
              onkeydown={(e) => e.stopPropagation()}
            >
              <Icon name="more-horizontal" size={14} />
            </button>
          {/if}
          {#if isFeatureEnabled("ALPH_156_CONFLICT_FLOW") && file.status === "conflicted"}
            <span class="conflict-tag">unmerged</span>
          {/if}
          <span
            class="status-badge {file.status}"
            aria-label={file.status === "conflicted" ? "unmerged" : file.status}
            title={file.status === "conflicted" ? "unmerged" : file.status}
          >
            {statusGlyph(file.status)}
          </span>
        </div>
      </div>
    {/each}

    {#if padBottom > 0}
      <div class="virtual-pad" style="height: {padBottom}px" aria-hidden="true"></div>
    {/if}
  </div>
</div>

<style>
  .file-list-container {
    display: flex;
    flex-direction: column;
    height: 100%;
    width: 100%;
    min-height: 0;
    background-color: var(--bg);
    overflow: hidden;
  }

  .file-list-header {
    position: relative;
    height: 43px;
    min-height: 43px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px;
    background-color: var(--bg);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .header-cb {
    cursor: pointer;
    width: 14px;
    height: 14px;
    margin: 0;
    flex-shrink: 0;
  }

  .header-title {
    font-size: 12px;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .header-actions {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .files-discard {
    height: 22px;
    padding: 0 8px;
    border: 1px solid var(--danger);
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--danger);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    white-space: nowrap;
  }
  .files-discard:hover:not(:disabled) {
    background: var(--danger-bg);
  }
  .files-discard:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .files-more {
    display: flex;
    align-items: center;
    justify-content: center;
    border: 0;
    background: none;
    color: var(--text-muted);
    padding: 2px;
    cursor: pointer;
    opacity: 0;
    transition: opacity 100ms ease;
  }
  .file-list-header:hover .files-more,
  .files-more:focus-visible,
  .files-more[aria-expanded="true"] {
    opacity: 1;
  }

  .options-backdrop {
    position: fixed;
    inset: 0;
    z-index: 25;
    background: transparent;
  }

  .files-options {
    position: absolute;
    right: 5px;
    top: 39px;
    z-index: 30;
    background: var(--bg);
    box-shadow: var(--shadow-pop);
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-sm);
    padding: 5px;
    min-width: 200px;
  }
  .files-options button {
    display: block;
    width: 100%;
    font-size: 12px;
    padding: 8px;
    text-align: left;
    background: none;
    border: 0;
    color: var(--text);
    border-radius: var(--r-sm);
    cursor: pointer;
  }
  .files-options button:hover {
    background: var(--bg-muted);
  }
  .files-options button.danger {
    color: var(--danger);
  }
  .files-options button.danger:hover:not(:disabled) {
    background: var(--danger-bg);
  }
  .files-options button:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .file-banner {
    flex-shrink: 0;
  }

  .file-rows {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    min-height: 0;
    background-color: var(--bg);
    transition: opacity 120ms ease;
  }
  .file-rows.pending {
    opacity: 0.6;
  }
  .virtual-pad {
    flex-shrink: 0;
  }

  .file-row {
    display: flex;
    align-items: center;
    gap: 9px;
    height: 36px;
    padding: 0 12px;
    cursor: pointer;
    border-bottom: 1px solid var(--border-subtle);
    background-color: var(--bg);
    position: relative;
  }
  .file-row:hover {
    background: var(--bg-subtle);
  }
  .file-row.selected {
    background: var(--bg-muted);
  }
  .file-row.conflicted {
    background: var(--danger-subtle, rgba(218, 54, 51, 0.08));
  }
  .file-row.conflicted:hover {
    background: var(--danger-subtle, rgba(218, 54, 51, 0.14));
  }
  .file-row.conflicted.selected {
    background: var(--danger-subtle, rgba(218, 54, 51, 0.18));
  }
  .conflict-tag {
    color: var(--danger);
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    white-space: nowrap;
  }
  .file-row:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  .row-cb {
    cursor: pointer;
    width: 14px;
    height: 14px;
    margin: 0;
    flex-shrink: 0;
    accent-color: var(--accent);
  }

  .path-wrap {
    flex: 1;
    min-width: 0;
    display: flex;
    white-space: nowrap;
    font-size: 12px;
    color: var(--text);
  }
  .dir-text {
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .file-name {
    overflow: hidden;
    text-overflow: ellipsis;
    flex-shrink: 0;
    max-width: 100%;
  }

  .row-right {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  /* Ghost "⋯" action button, revealed on hover/focus/selection. */
  .row-menu-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: none;
    border-radius: var(--r-sm, 4px);
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    flex-shrink: 0;
    opacity: 0;
    transition: opacity 100ms ease, background-color 100ms ease;
  }
  :global([data-theme="dark"]) .row-menu-btn {
    color: var(--text-faint);
  }
  .file-row:hover .row-menu-btn,
  .file-row.selected .row-menu-btn,
  .file-row.menu-open .row-menu-btn,
  .row-menu-btn:focus-visible {
    opacity: 1;
  }
  .row-menu-btn:hover {
    background: rgba(0, 0, 0, 0.08);
    color: var(--text);
  }
  :global([data-theme="dark"]) .row-menu-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: var(--text);
  }

  .status-badge {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 13px;
    height: 13px;
    border: 1.5px solid var(--warning);
    color: var(--warning);
    border-radius: var(--r-sm);
    font-size: 12px;
    line-height: 1;
    flex-shrink: 0;
  }
  .status-badge.added,
  .status-badge.untracked,
  .status-badge.copied {
    color: var(--success);
    border-color: var(--success);
  }
  .status-badge.deleted,
  .status-badge.conflicted {
    color: var(--danger);
    border-color: var(--danger);
  }
  .status-badge.renamed {
    color: var(--accent);
    border-color: var(--accent);
  }

  /* Prototype compact overrides (approved-prototype.html · @media max-width:1400px). */
  @media (max-width: 1400px) {
    .file-list-header {
      padding: 0 10px;
    }
    .header-title {
      font-size: 11px;
    }
    .file-row {
      padding: 0 10px;
    }
    .path-wrap {
      font-size: 11.5px;
    }
  }

  /* Per-file context menu (right-click / Shift+F10). The backdrop is invisible
     and only dismisses on outside click or Escape. */
  .ctx-backdrop {
    position: fixed;
    inset: 0;
    z-index: 99;
    background: transparent;
  }

  .ctx-menu {
    position: fixed;
    z-index: 100;
    min-width: 200px;
    padding: 4px;
    background: var(--bg);
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-md);
    box-shadow: var(--shadow-pop);
    outline: none;
  }

  .ctx-item {
    display: block;
    width: 100%;
    padding: 7px 8px;
    text-align: left;
    font-size: 12px;
    background: none;
    border: 0;
    color: var(--text);
    border-radius: var(--r-sm);
    cursor: pointer;
  }
  .ctx-item:hover,
  .ctx-item:focus-visible {
    background: var(--bg-muted);
  }

  .ctx-divider {
    height: 1px;
    margin: 4px 2px;
    background: var(--border-subtle);
  }
</style>
