<script lang="ts">
  // Header branch switcher: mirrors the Fetch button's two-line chrome and
  // opens a compact branch list. Selecting a branch opens (or focuses) that
  // branch's view; changing the checkout stays an explicit action.
  import { tick } from "svelte";
  import { loadRepos, refreshSingleStatus } from "../../domain/stores/repos";
  import { openRepoInFocusedPane } from "../../domain/stores/panes";
  import { addToast, branchesVersion, requestCheckout } from "../../app/ui";
  import { registerDismiss } from "../../app/commands";
  import { formatRelativeTime } from "../../domain/format";
  import * as api from "../../platform/api";
  import type { BranchInfo, RepoEntry, RepoStatus } from "../../platform/types";
  import Icon from "../common/Icon.svelte";

  interface Props {
    repo: RepoEntry | null;
    status: RepoStatus | null;
    /** Left edge (px) of the pane's leading column. */
    left: number;
    /** Width (px) of that column. */
    width: number;
  }

  let { repo, status, left, width }: Props = $props();

  let branches = $state<BranchInfo[]>([]);
  let open = $state(false);
  let filter = $state("");
  let busy = $state(false);
  let menuEl = $state<HTMLDivElement | null>(null);

  let currentBranch = $derived(repo?.branch || status?.branch || "HEAD");
  let filteredBranches = $derived(
    branches.filter(
      (b) => !filter || b.name.toLowerCase().includes(filter.toLowerCase())
    )
  );

  let generation = 0;
  async function loadBranches(id: string) {
    const gen = ++generation;
    try {
      const list = await api.branchList(id);
      if (gen === generation && repo?.id === id) branches = list;
    } catch {
      if (gen === generation) branches = [];
    }
  }

  $effect(() => {
    const id = repo?.id;
    void $branchesVersion;
    branches = [];
    open = false;
    filter = "";
    if (id) void loadBranches(id);
    return () => {
      generation++;
    };
  });

  $effect(() =>
    registerDismiss(() => {
      if (!open) return false;
      close();
      return true;
    })
  );

  function close() {
    open = false;
    filter = "";
  }

  async function toggle() {
    if (open) {
      close();
      return;
    }
    if (!repo) return;
    open = true;
    filter = "";
    void loadBranches(repo.id);
    await tick();
    const current = menuEl?.querySelector<HTMLElement>(".branch-row.current");
    (current ?? menuEl?.querySelector<HTMLElement>(".branch-row"))?.focus();
  }

  function handleMenuKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      close();
      return;
    }
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    if (!menuEl) return;
    e.preventDefault();
    const rows = Array.from(menuEl.querySelectorAll<HTMLElement>(".branch-row"));
    if (rows.length === 0) return;
    const index = rows.indexOf(document.activeElement as HTMLElement);
    rows[(index + (e.key === "ArrowDown" ? 1 : -1) + rows.length) % rows.length]?.focus();
  }

  async function selectBranch(name: string) {
    if (!repo || busy) return;
    close();
    if (name === currentBranch) return;

    // Uncommitted work needs a decision first; clean trees switch immediately.
    if ((status?.dirtyCount ?? 0) > 0) {
      requestCheckout({
        repoId: repo.id,
        repoName: repo.name,
        target: name,
        dirtyCount: status?.dirtyCount ?? 0,
      });
      return;
    }

    busy = true;
    try {
      await api.checkout(repo.id, name, "leave");
      await refreshSingleStatus(repo.id);
      await loadRepos();
      void loadBranches(repo.id);
      addToast({ message: `Checked out ${name}`, kind: "success" });
    } catch (e: any) {
      addToast({ message: e?.message || `Couldn't check out ${name}`, kind: "error" });
    } finally {
      busy = false;
    }
  }
</script>

<div class="branch-control" style="left: {left}px; width: {width}px">
  <button
    type="button"
    class="branch-trigger"
    aria-haspopup="menu"
    aria-expanded={open}
    title={`Branch: ${currentBranch}`}
    disabled={busy}
    onclick={() => void toggle()}
  >
    <span class="branch-icon"><Icon name="git-branch" size={16} /></span>
    <span class="branch-name text-strong">{currentBranch}</span>
    <span class="branch-chevron"><Icon name="chevron-down" size={12} /></span>
  </button>

  {#if open}
    <div class="menu-backdrop" role="presentation" onclick={close}></div>

    <div
      bind:this={menuEl}
      class="branch-menu"
      role="menu"
      aria-label="Branches"
      tabindex="-1"
      onkeydown={handleMenuKeydown}
    >
      {#if branches.length > 5}
        <div class="menu-filter">
          <input
            type="text"
            class="menu-filter-input"
            placeholder="Filter branches…"
            bind:value={filter}
            onkeydown={(e) => e.stopPropagation()}
          />
        </div>
      {/if}

      <div class="menu-list">
        {#each filteredBranches as b (b.name)}
          <button
            type="button"
            class="branch-row"
            class:current={b.name === currentBranch}
            role="menuitemradio"
            aria-checked={b.name === currentBranch}
            tabindex={b.name === currentBranch ? 0 : -1}
            onclick={() => void selectBranch(b.name)}
          >
            <Icon name="git-branch" size={13} />
            <span class="row-name">{b.name}</span>
            {#if b.isHead}
              <span class="head-pill">HEAD</span>
            {/if}
            {#if b.updatedAt}
              <span class="row-time text-xs text-muted">{formatRelativeTime(b.updatedAt)}</span>
            {/if}
          </button>
        {/each}

        {#if filteredBranches.length === 0}
          <div class="empty text-xs text-muted">No branches found</div>
        {/if}
      </div>

    </div>
  {/if}
</div>

<style>
  .branch-control {
    position: absolute;
    top: calc(50% - 17px);
    height: 34px;
  }

  .branch-trigger {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 100%;
    padding: 0 10px;
    border: 0;
    border-left: 1px solid var(--border);
    background: var(--bg-muted);
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
    transition: background 100ms ease;
  }
  .branch-trigger:hover,
  .branch-trigger[aria-expanded="true"] {
    background: color-mix(in srgb, var(--bg-muted) 86%, var(--text) 14%);
  }
  .branch-trigger:focus-visible {
    outline: none;
    box-shadow: inset 0 0 0 1px var(--accent);
  }

  .branch-icon,
  .branch-chevron {
    display: inline-flex;
    align-items: center;
    flex-shrink: 0;
    color: var(--text-muted);
  }

  .branch-name {
    flex: 1;
    min-width: 0;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    line-height: 1.2;
  }

  .menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 30;
    background: transparent;
  }

  .branch-menu {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    z-index: 31;
    width: max(280px, 100%);
    max-width: 380px;
    max-height: min(480px, 70vh);
    display: flex;
    flex-direction: column;
    padding: 0;
    background-color: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    box-shadow: var(--shadow-pop);
    overflow: hidden;
  }

  .menu-filter {
    padding: 6px 4px 6px;
  }
  .menu-filter-input {
    width: 100%;
    height: 30px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background-color: var(--bg-subtle);
    color: var(--text);
    font: inherit;
    outline: none;
  }
  .menu-filter-input:focus {
    border-color: var(--accent);
  }

  .menu-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .branch-row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 8px 10px;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .branch-row:hover,
  .branch-row:focus-visible {
    background-color: var(--bg-muted);
    outline: none;
  }
  .branch-row.current {
    color: var(--accent);
    background-color: var(--accent-bg);
  }

  .row-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .head-pill {
    flex-shrink: 0;
    text-transform: uppercase;
    font-weight: 600;
    color: var(--text-muted);
  }

  .row-time {
    flex-shrink: 0;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }


  .empty {
    padding: 14px 8px;
    text-align: center;
  }


</style>
