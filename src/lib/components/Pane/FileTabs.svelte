<script lang="ts">
  // Open-file tab strip for the code viewer (Zed-style): one tab per file,
  // hairline-separated, active tab on the content background.
  import { tick } from "svelte";
  import FileIcon from "../common/FileIcon.svelte";
  import Icon from "../common/Icon.svelte";

  interface Props {
    tabs: string[];
    active: string | null;
    onactivate: (path: string) => void;
    onclose: (path: string) => void;
  }

  let { tabs, active, onactivate, onclose }: Props = $props();

  let stripEl = $state<HTMLDivElement | null>(null);

  // Keep the active tab in view when the strip is narrower than its tabs.
  $effect(() => {
    const path = active;
    const el = stripEl;
    if (!el || !path) return;
    tick().then(() => {
      const tab = el.querySelector<HTMLElement>(`[data-tab-path="${CSS.escape(path)}"]`);
      if (!tab) return;
      const left = tab.offsetLeft;
      const right = left + tab.offsetWidth;
      if (left < el.scrollLeft) el.scrollLeft = left;
      else if (right > el.scrollLeft + el.clientWidth) el.scrollLeft = right - el.clientWidth;
    });
  });

  // Arrow keys walk the strip, matching the tablist pattern (focus follows the
  // newly selected tab, close buttons stay reachable with Tab).
  function handleKeydown(e: KeyboardEvent) {
    if (tabs.length < 2) return;
    const index = active ? tabs.indexOf(active) : -1;
    let next = -1;
    if (e.key === "ArrowRight") next = (index + 1) % tabs.length;
    else if (e.key === "ArrowLeft") next = (index - 1 + tabs.length) % tabs.length;
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = tabs.length - 1;
    else return;
    e.preventDefault();
    const path = tabs[next];
    onactivate(path);
    tick().then(() => {
      stripEl
        ?.querySelector<HTMLElement>(`[data-tab-path="${CSS.escape(path)}"] .file-tab-main`)
        ?.focus();
    });
  }

  function basename(path: string): string {
    const parts = path.split("/");
    return parts[parts.length - 1] || path;
  }
</script>

<div
  class="file-tabs"
  role="tablist"
  aria-label="Open files"
  tabindex="-1"
  bind:this={stripEl}
  onkeydown={handleKeydown}
>
  {#each tabs as path (path)}
    <div
      class="file-tab"
      class:active={path === active}
      role="presentation"
      data-tab-path={path}
    >
      <button
        type="button"
        role="tab"
        aria-selected={path === active}
        aria-controls="file-viewer"
        tabindex={path === active ? 0 : -1}
        class="file-tab-main"
        title={path}
        onclick={() => onactivate(path)}
      >
        <FileIcon filename={basename(path)} size={13} />
        <span class="file-tab-name text-xs">{basename(path)}</span>
      </button>
      <button
        type="button"
        class="file-tab-close"
        title="Close tab"
        aria-label={`Close ${basename(path)}`}
        onclick={(e) => {
          e.stopPropagation();
          onclose(path);
        }}
      >
        <Icon name="close" size={11} strokeWidth={2.2} />
      </button>
    </div>
  {/each}
</div>

<style>
  .file-tabs {
    display: flex;
    align-items: stretch;
    height: 32px;
    flex-shrink: 0;
    background-color: var(--bg-subtle);
    border-bottom: 1px solid var(--border);
    overflow-x: auto;
    overflow-y: hidden;
  }

  .file-tab {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    max-width: 220px;
    min-width: 0;
    border-right: 1px solid var(--border-subtle);
    background-color: transparent;
    color: var(--text-muted);
    transition: background-color 80ms linear, color 80ms linear;
  }
  .file-tab:hover {
    background-color: var(--bg-muted);
    color: var(--text);
  }
  .file-tab.active {
    background-color: var(--bg);
    color: var(--text);
    box-shadow: inset 0 -2px 0 var(--accent);
  }

  .file-tab-main {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 100%;
    min-width: 0;
    padding: 0 2px 0 10px;
    border: 0;
    border-radius: 0;
    background: transparent;
    color: inherit;
    cursor: pointer;
  }
  .file-tab-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .file-tab-close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    margin: 0 4px 0 2px;
    padding: 0;
    border: 0;
    border-radius: 0;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    flex-shrink: 0;
    opacity: 0;
  }
  .file-tab:hover .file-tab-close,
  .file-tab.active .file-tab-close,
  .file-tab-close:focus-visible {
    opacity: 1;
  }
  .file-tab-close:hover {
    background-color: var(--bg-muted);
    color: var(--text);
  }
  .file-tab-main:focus-visible,
  .file-tab-close:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
</style>
