<script lang="ts">
  // Open-file tab strip for the code viewer (Zed-style): one tab per file,
  // hairline-separated, active tab on the content background.
  import { tick } from "svelte";
  import FileIcon from "../common/FileIcon.svelte";
  import Icon from "../common/Icon.svelte";
  import ContextMenu from "../common/ContextMenu.svelte";
  import { formatKey } from "../../platform/keys";
  import type { MenuItem } from "../../app/menu";

  interface Props {
    tabs: string[];
    active: string | null;
    onactivate: (path: string) => void;
    onclose: (path: string) => void;
    onclosepaths?: (paths: string[]) => void;
    oncopy?: (path: string) => void;
    onreveal?: (path: string) => void;
    onopenexternal?: (path: string) => void;
  }

  let {
    tabs,
    active,
    onactivate,
    onclose,
    onclosepaths,
    oncopy,
    onreveal,
    onopenexternal,
  }: Props = $props();

  let stripEl = $state<HTMLDivElement | null>(null);
  let menu = $state<{ x: number; y: number; path: string } | null>(null);

  function openTabMenu(e: MouseEvent, path: string) {
    e.preventDefault();
    e.stopPropagation();
    menu = { x: e.clientX, y: e.clientY, path };
  }

  // Keyboard-accessible alternative to right-click (Shift+F10 / Menu key).
  function openTabMenuFromKeyboard(e: KeyboardEvent, path: string) {
    if (e.key !== "ContextMenu" && !(e.shiftKey && e.key === "F10")) return;
    e.preventDefault();
    e.stopPropagation();
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    menu = { x: rect.left + 16, y: rect.top + 16, path };
  }

  let menuItems = $derived.by<MenuItem[]>(() => {
    const target = menu;
    if (!target) return [];

    const index = tabs.indexOf(target.path);
    const items: MenuItem[] = [
      {
        id: "close",
        label: "Close",
        shortcut: formatKey("Mod+W"),
        onselect: () => onclose(target.path),
      },
      {
        id: "close-others",
        label: "Close Others",
        disabled: !onclosepaths || tabs.length <= 1,
        onselect: () => onclosepaths?.(tabs.filter((path) => path !== target.path)),
      },
      {
        id: "close-right",
        label: "Close to the Right",
        disabled: !onclosepaths || index < 0 || index >= tabs.length - 1,
        onselect: () => onclosepaths?.(tabs.slice(index + 1)),
      },
      {
        id: "close-all",
        label: "Close All",
        shortcut: formatKey("Mod+Shift+W"),
        disabled: !onclosepaths,
        onselect: () => onclosepaths?.([...tabs]),
      },
    ];

    if (oncopy) {
      items.push({
        id: "copy-path",
        label: "Copy Path",
        separatorBefore: true,
        onselect: () => oncopy(target.path),
      });
    }
    if (onreveal) {
      items.push({
        id: "reveal",
        label: "Reveal in File Manager",
        onselect: () => onreveal(target.path),
      });
    }
    if (onopenexternal) {
      items.push({
        id: "open-external",
        label: "Open in External Editor",
        onselect: () => onopenexternal(target.path),
      });
    }
    return items;
  });

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
      oncontextmenu={(e) => openTabMenu(e, path)}
    >
      <button
        type="button"
        role="tab"
        aria-selected={path === active}
        aria-controls="file-viewer"
        aria-haspopup="menu"
        aria-expanded={menu?.path === path}
        tabindex={path === active ? 0 : -1}
        class="file-tab-main"
        title={path}
        onclick={() => onactivate(path)}
        onkeydown={(e) => openTabMenuFromKeyboard(e, path)}
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

{#if menu}
  {#key menu}
    <ContextMenu
      x={menu.x}
      y={menu.y}
      items={menuItems}
      label="File tab actions"
      onclose={() => (menu = null)}
    />
  {/key}
{/if}

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
