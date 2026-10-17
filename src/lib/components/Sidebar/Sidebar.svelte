<script lang="ts">
  import { sortedRepos, statuses, loadRepos } from "../../stores/repos";
  import { sidebarCollapsed, addRepoDialogOpen, addToast } from "../../stores/ui";
  import * as api from "../../api";
  import RepoRow from "./RepoRow.svelte";
  import Icon from "../common/Icon.svelte";

  interface Props {
    width?: number;
  }
  let { width = 232 }: Props = $props();

  let repoList = $derived($sortedRepos);
  let statusMap = $derived($statuses);
  let isDraggingOver = $state(false);

  function openAddRepo() {
    $addRepoDialogOpen = true;
  }

  function handleDragOver(e: DragEvent) {
    e.preventDefault();
    isDraggingOver = true;
  }

  function handleDragLeave() {
    isDraggingOver = false;
  }

  async function handleDrop(e: DragEvent) {
    e.preventDefault();
    isDraggingOver = false;
    const files = e.dataTransfer?.files;
    if (files && files.length > 0) {
      const path = (files[0] as any).path || files[0].name;
      if (path) {
        try {
          const entry = await api.repoAdd(path);
          addToast({ message: `Added repository ${entry.name}`, kind: "success" });
          await loadRepos();
        } catch (err: any) {
          addToast({ message: err.message || "That folder isn't a Git repository.", kind: "error" });
        }
      }
    }
  }
</script>

<aside
  class="relay-sidebar {$sidebarCollapsed ? 'collapsed' : ''}"
  style={$sidebarCollapsed ? "" : `width: ${width}px;`}
  ondragover={handleDragOver}
  ondragleave={handleDragLeave}
  ondrop={handleDrop}
  aria-label="Repositories"
>
  {#if isDraggingOver}
    <div class="drop-overlay">
      <div class="drop-dashed-box">
        <span>Drop to add repository</span>
      </div>
    </div>
  {/if}

  <div class="repo-list">
    {#each repoList as repo (repo.id)}
      <RepoRow {repo} status={statusMap[repo.id]} />
    {/each}

    <div class="dock-separator"></div>

    <button
      class="add-repo-btn"
      onclick={openAddRepo}
      aria-label="Add repository"
      title="Add repository (⌘O)"
    >
      <Icon name="plus" size={16} />
      {#if !$sidebarCollapsed}
        <span>Add repository</span>
      {/if}
    </button>
  </div>
</aside>

<style>
  .relay-sidebar {
    height: 100%;
    background: #f7f7f7;
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    position: relative;
    overflow: visible;
    z-index: 25;
  }

  .relay-sidebar.collapsed {
    width: 60px;
  }

  .repo-list {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 5px 0 0;
  }

  .dock-separator {
    height: 1px;
    background: #d5d9df;
    margin: 6px 12px;
  }

  .add-repo-btn {
    display: flex;
    align-items: center;
    gap: 14px;
    width: 100%;
    height: 32px;
    padding: 0 20px;
    background: transparent;
    border: none;
    border-radius: 0;
    color: #65717f;
    font-size: 12px;
    font-weight: 400;
    cursor: pointer;
    transition: background-color 80ms linear;
  }
  .add-repo-btn:hover {
    background-color: #f0f1f2;
    color: #30343a;
  }
  .relay-sidebar.collapsed .add-repo-btn {
    justify-content: center;
    padding: 0;
  }

  .drop-overlay {
    position: absolute;
    inset: 4px;
    background-color: rgba(24, 115, 213, 0.08);
    backdrop-filter: blur(2px);
    z-index: 10;
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: none;
  }

  .drop-dashed-box {
    border: 2px dashed var(--accent);
    border-radius: var(--r-md);
    padding: 16px;
    color: var(--accent);
    font-size: 12px;
    font-weight: 600;
    text-align: center;
    background-color: var(--bg);
  }
</style>
