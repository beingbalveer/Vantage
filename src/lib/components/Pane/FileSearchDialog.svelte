<script lang="ts">
  // Centered quick-open for the file explorer. Search is a primary action, so
  // it gets a dominant overlay instead of a cramped row in the tree toolbar.
  import { tick } from "svelte";
  import Modal from "../common/Modal.svelte";
  import FileIcon from "../common/FileIcon.svelte";
  import Icon from "../common/Icon.svelte";
  import { rankPaths } from "../../util/fuzzy";

  interface Props {
    open: boolean;
    files: string[];
    repoName?: string;
    onselect: (path: string) => void;
    onclose: () => void;
  }

  let { open, files, repoName = "", onselect, onclose }: Props = $props();

  /** Rendering cap keeps huge repositories responsive; the count stays visible. */
  const RESULT_LIMIT = 200;

  let query = $state("");
  let cursor = $state(0);
  let inputEl = $state<HTMLInputElement | null>(null);
  let listEl = $state<HTMLDivElement | null>(null);

  let matches = $derived(rankPaths(query, files));
  let visible = $derived(matches.slice(0, RESULT_LIMIT));
  // Clamp so a shrinking result set never leaves the highlight out of range.
  let selected = $derived(Math.min(cursor, Math.max(0, visible.length - 1)));

  // A fresh query every time the overlay opens.
  $effect(() => {
    if (!open) return;
    query = "";
    cursor = 0;
  });

  // Keep the highlighted row in view as the cursor moves.
  $effect(() => {
    const index = selected;
    if (!open) return;
    tick().then(() => {
      listEl?.querySelector(`[data-index="${index}"]`)?.scrollIntoView({ block: "nearest" });
    });
  });

  function splitPath(path: string): { dir: string; name: string } {
    const idx = path.lastIndexOf("/");
    return idx === -1
      ? { dir: "", name: path }
      : { dir: path.slice(0, idx + 1), name: path.slice(idx + 1) };
  }

  function move(delta: number) {
    if (visible.length === 0) return;
    cursor = (selected + delta + visible.length) % visible.length;
  }

  function choose(path: string | undefined) {
    if (!path) return;
    onselect(path);
    onclose();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      move(1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      move(-1);
    } else if (e.key === "PageDown") {
      e.preventDefault();
      move(10);
    } else if (e.key === "PageUp") {
      e.preventDefault();
      move(-10);
    } else if (e.key === "Enter") {
      e.preventDefault();
      choose(visible[selected]);
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onclose();
    }
  }
</script>

<Modal {open} width={640} bodyPadding={false} showCloseButton={false} ariaLabel="Search files" {onclose}>
  <div class="file-search">
    <div class="search-field">
      <span class="search-icon">
        <Icon name="search" size={16} />
      </span>
      <input
        bind:this={inputEl}
        bind:value={query}
        type="text"
        class="search-input text-base"
        role="combobox"
        aria-label="Search files"
        aria-autocomplete="list"
        aria-controls="file-search-results"
        aria-expanded="true"
        aria-activedescendant={visible.length > 0 ? `file-search-option-${selected}` : undefined}
        placeholder="Search files by name or path…"
        spellcheck="false"
        autocomplete="off"
        oninput={() => (cursor = 0)}
        onkeydown={handleKeydown}
      />
      {#if query}
        <button
          type="button"
          class="clear-btn"
          tabindex="-1"
          title="Clear search"
          aria-label="Clear search"
          onclick={() => {
            query = "";
            cursor = 0;
            inputEl?.focus();
          }}
        >
          <Icon name="close" size={11} strokeWidth={2.2} />
        </button>
      {/if}
      <kbd class="esc-hint text-xs">esc</kbd>
    </div>

    <div bind:this={listEl} class="results" id="file-search-results" role="listbox" aria-label="Files">
      {#if visible.length === 0}
        <div class="empty text-sm text-muted">
          {#if files.length === 0}
            No files in this repository
          {:else}
            No files match “{query}”
          {/if}
        </div>
      {:else}
        {#each visible as path, i (path)}
          {@const parts = splitPath(path)}
          <div
            class="result {i === selected ? 'selected' : ''}"
            id={`file-search-option-${i}`}
            data-index={i}
            role="option"
            aria-selected={i === selected}
            tabindex="-1"
            title={path}
            onmousemove={() => (cursor = i)}
            onclick={() => choose(path)}
            onkeydown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                choose(path);
              }
            }}
          >
            <FileIcon filename={parts.name} size={14} />
            <span class="result-name text-sm">{parts.name}</span>
            {#if parts.dir}
              <span class="result-dir text-xs text-muted">{parts.dir}</span>
            {/if}
          </div>
        {/each}
      {/if}
    </div>

    <div class="footer text-xs text-muted">
      {#if repoName}
        <span class="repo-name">{repoName}</span>
        <span class="dot">·</span>
      {/if}
      <span>
        {#if query.trim()}
          {matches.length} of {files.length}
        {:else}
          {files.length}
        {/if}
        files
      </span>
      {#if matches.length > RESULT_LIMIT}
        <span class="dot">·</span>
        <span>showing first {RESULT_LIMIT}</span>
      {/if}
      <span class="hints">
        <span>↑↓ navigate</span>
        <span class="dot">·</span>
        <span>↵ open</span>
        <span class="dot">·</span>
        <span>esc close</span>
      </span>
    </div>
  </div>
</Modal>

<style>
  .file-search {
    display: flex;
    flex-direction: column;
    min-height: 0;
    width: 100%;
    /* Fixed height: the overlay must not resize (and drift toward the center)
       as the result count changes. */
    height: min(70vh, 560px);
  }

  .search-field {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 52px;
    padding: 0 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .search-icon {
    display: flex;
    align-items: center;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .search-input {
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 0;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--text);
    font-family: var(--font-ui);
  }
  .search-input::placeholder {
    color: var(--text-faint);
  }

  .clear-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    flex-shrink: 0;
  }
  .clear-btn:hover {
    background-color: var(--bg-muted);
    color: var(--text);
  }

  .esc-hint {
    padding: 1px 5px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    color: var(--text-muted);
    font-family: var(--font-mono);
    flex-shrink: 0;
  }

  .results {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    padding: 6px;
  }

  .empty {
    padding: 28px 16px;
    text-align: center;
  }

  .result {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding: 0 10px;
    border-radius: var(--r-sm);
    cursor: pointer;
  }
  .result:hover,
  .result.selected {
    background-color: var(--bg-muted);
  }

  .result-name {
    flex-shrink: 0;
    color: var(--text);
    white-space: nowrap;
  }

  .result-dir {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .footer {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 14px;
    border-top: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }

  .repo-name {
    max-width: 180px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text);
    font-weight: 600;
  }

  .dot {
    color: var(--text-faint);
  }

  .hints {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-left: auto;
  }
</style>
