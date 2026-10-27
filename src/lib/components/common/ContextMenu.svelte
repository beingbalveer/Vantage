<script lang="ts">
  import { tick } from "svelte";
  import { clampMenuPosition, type MenuItem } from "../../app/menu";

  interface Props {
    x: number;
    y: number;
    items: MenuItem[];
    label?: string;
    onclose: () => void;
  }

  let { x, y, items, label = "Actions", onclose }: Props = $props();

  let menuEl = $state<HTMLElement | null>(null);
  let menuSize = $state({ width: 0, height: 0 });
  let pos = $derived(
    clampMenuPosition(x, y, menuSize.width, menuSize.height, window.innerWidth, window.innerHeight)
  );
  // The element focused before the menu opened; focus is restored here on
  // Escape / action so the menu never permanently steals focus.
  let previousFocus: HTMLElement | null = null;

  function menuItems(): HTMLButtonElement[] {
    if (!menuEl) return [];
    return Array.from(
      menuEl.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not([disabled])')
    );
  }

  function close(restoreFocus: boolean) {
    onclose();
    if (restoreFocus && previousFocus && document.contains(previousFocus)) {
      previousFocus.focus();
    }
  }

  function moveFocus(delta: number) {
    const els = menuItems();
    if (els.length === 0) return;
    const current = els.indexOf(document.activeElement as HTMLButtonElement);
    const next = (current + delta + els.length) % els.length;
    els[next].focus();
  }

  $effect(() => {
    previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    tick().then(() => {
      if (!menuEl) return;
      const rect = menuEl.getBoundingClientRect();
      menuSize = { width: rect.width, height: rect.height };
      // Move focus into the menu but keep menu items tabbable so Tab can
      // leave the menu (the menu is not a focus trap).
      const els = menuItems();
      if (els.length > 0) els[0].focus();
      else menuEl.focus();
    });
  });

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close(true);
      return;
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      moveFocus(1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      moveFocus(-1);
    } else if (e.key === "Home") {
      e.preventDefault();
      const els = menuItems();
      if (els.length > 0) els[0].focus();
    } else if (e.key === "End") {
      e.preventDefault();
      const els = menuItems();
      if (els.length > 0) els[els.length - 1].focus();
    }
  }

  // Close when focus leaves the menu entirely (e.g. Tab out) without trapping it.
  function handleFocusOut(e: FocusEvent) {
    const next = e.relatedTarget as Node | null;
    if (next && menuEl && menuEl.contains(next)) return;
    onclose();
  }

  function runItem(item: MenuItem) {
    if (item.disabled) return;
    close(true);
    item.onselect();
  }
</script>

<div
  class="menu-backdrop"
  role="presentation"
  onclick={() => close(false)}
  oncontextmenu={(e) => {
    e.preventDefault();
    close(false);
  }}
></div>

<div
  bind:this={menuEl}
  class="context-menu"
  style="left: {pos.left}px; top: {pos.top}px;"
  role="menu"
  aria-label={label}
  tabindex="-1"
  onclick={(e) => e.stopPropagation()}
  oncontextmenu={(e) => e.preventDefault()}
  onkeydown={handleKeydown}
  onfocusout={handleFocusOut}
>
  {#each items as item (item.id)}
    {#if item.separatorBefore}
      <div class="menu-divider" role="separator"></div>
    {/if}
    <button
      type="button"
      class="menu-item {item.danger ? 'danger' : ''}"
      role="menuitem"
      disabled={item.disabled}
      onclick={() => runItem(item)}
    >
      <span class="menu-label">{item.label}</span>
      {#if item.shortcut}
        <span class="menu-shortcut">{item.shortcut}</span>
      {/if}
    </button>
  {/each}
</div>

<style>
  /* Backdrop sits just under the menu (DESIGN.md §2 z-index: popovers/menus 100). */
  .menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 99;
  }

  .context-menu {
    position: fixed;
    z-index: 100;
    min-width: 190px;
    max-width: 320px;
    background-color: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    box-shadow: var(--shadow-pop);
    padding: 4px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    outline: none;
  }

  .menu-divider {
    height: 1px;
    margin: 4px 2px;
    background-color: var(--border-subtle);
  }

  .menu-item {
    display: flex;
    align-items: center;
    gap: 16px;
    width: 100%;
    min-height: 28px;
    padding: 0 10px;
    border: none;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--text);
    font-size: 12px;
    text-align: left;
    cursor: pointer;
  }
  .menu-item:hover:not(:disabled),
  .menu-item:focus-visible {
    background-color: var(--bg-muted);
    outline: none;
  }
  .menu-item:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .menu-item.danger {
    color: var(--danger);
  }
  .menu-item.danger:hover:not(:disabled),
  .menu-item.danger:focus-visible {
    background-color: var(--danger-bg);
  }

  .menu-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .menu-shortcut {
    flex-shrink: 0;
    font-size: 11px;
    color: var(--text-faint);
  }
</style>
