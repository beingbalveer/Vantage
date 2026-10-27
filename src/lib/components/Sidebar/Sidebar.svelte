<script lang="ts">
  import { onDestroy } from "svelte";
  import { sortedRepos, statuses, loadRepos, reorderRepos } from "../../domain/stores/repos";
  import { sidebarCollapsed, addRepoDialogOpen, addToast } from "../../app/ui";
  import * as api from "../../platform/api";
  import { moveItem, findDropTarget, type DropPosition } from "../../domain/reorder";
  import RepoRow from "./RepoRow.svelte";
  import Icon from "../common/Icon.svelte";

  interface Props {
    width?: number;
  }
  let { width = 232 }: Props = $props();

  let repoList = $derived($sortedRepos);
  let statusMap = $derived($statuses);
  let isDraggingOver = $state(false);

  // Drag and drop reordering state. Native HTML5 drag events are unreliable in
  // the Tauri webview, so rows are reordered with pointer events instead.
  let draggedRepoId = $state<string | null>(null);
  let dragOverRepoId = $state<string | null>(null);
  let dropPosition = $state<DropPosition | null>(null);
  let listEl = $state<HTMLDivElement | null>(null);

  const DRAG_THRESHOLD = 4;
  const AUTO_SCROLL_EDGE = 28;
  const AUTO_SCROLL_STEP = 12;

  let pointerDrag: {
    id: string;
    pointerId: number;
    startX: number;
    startY: number;
  } | null = null;
  let autoScrollDir: -1 | 0 | 1 = 0;
  let autoScrollFrame: number | null = null;
  let lastPointer = { x: 0, y: 0 };

  function openAddRepo() {
    $addRepoDialogOpen = true;
  }

  function handleDragOver(e: DragEvent) {
    if (draggedRepoId) return;
    if (e.dataTransfer?.types.includes("Files")) {
      e.preventDefault();
      isDraggingOver = true;
    }
  }

  function handleDragLeave(e: DragEvent) {
    const related = e.relatedTarget as Node | null;
    const current = e.currentTarget as HTMLElement | null;
    if (current && related && current.contains(related)) {
      return;
    }
    isDraggingOver = false;
  }

  async function handleDrop(e: DragEvent) {
    if (draggedRepoId) return;
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

  /** Start tracking a possible row drag. A move past the threshold begins it. */
  function handleRowPointerDown(e: PointerEvent, id: string) {
    if (e.button !== 0 || e.metaKey || e.ctrlKey || e.altKey) return;
    const target = e.target;
    if (
      target instanceof Element &&
      target.closest("button, a, input, textarea, select, [contenteditable='true']")
    ) {
      return;
    }
    stopPointerDrag();
    pointerDrag = { id, pointerId: e.pointerId, startX: e.clientX, startY: e.clientY };
    lastPointer = { x: e.clientX, y: e.clientY };
    // Capture so a fast drag that leaves the row keeps reporting moves.
    try {
      (e.currentTarget as HTMLElement | null)?.setPointerCapture?.(e.pointerId);
    } catch {
      // Synthetic events (tests) have no active pointer to capture.
    }
    window.addEventListener("pointermove", handlePointerMove, { passive: false });
    window.addEventListener("pointerup", handlePointerUp);
    window.addEventListener("pointercancel", handlePointerCancel);
    window.addEventListener("blur", handleWindowBlur);
  }

  function handlePointerMove(e: PointerEvent) {
    if (!pointerDrag || e.pointerId !== pointerDrag.pointerId) return;
    lastPointer = { x: e.clientX, y: e.clientY };
    if (!draggedRepoId) {
      const dx = e.clientX - pointerDrag.startX;
      const dy = e.clientY - pointerDrag.startY;
      if (Math.hypot(dx, dy) < DRAG_THRESHOLD) return;
      draggedRepoId = pointerDrag.id;
      document.body.classList.add("repo-dragging");
    }
    // Keep the browser from selecting text or starting a native drag.
    e.preventDefault();
    updateDropTargetFromPoint(e.clientX, e.clientY);
    updateAutoScroll(e.clientY);
  }

  function updateDropTargetFromPoint(x: number, y: number) {
    if (!draggedRepoId || !listEl) return;
    const bounds = listEl.getBoundingClientRect();
    if (x < bounds.left || x > bounds.right || y < bounds.top || y > bounds.bottom) {
      dragOverRepoId = null;
      dropPosition = null;
      return;
    }

    const rows = Array.from(listEl.querySelectorAll<HTMLElement>("[data-repo-id]"));
    const target = findDropTarget(
      rows.map((row) => {
        const rect = row.getBoundingClientRect();
        return { id: row.dataset.repoId ?? "", top: rect.top, height: rect.height };
      }),
      y,
      draggedRepoId
    );
    if (!target) {
      dragOverRepoId = null;
      dropPosition = null;
      return;
    }
    dragOverRepoId = target.id;
    dropPosition = target.position;
  }

  function updateAutoScroll(y: number) {
    if (!listEl) return;
    const rect = listEl.getBoundingClientRect();
    const dir = y < rect.top + AUTO_SCROLL_EDGE ? -1 : y > rect.bottom - AUTO_SCROLL_EDGE ? 1 : 0;
    if (dir === autoScrollDir) return;
    autoScrollDir = dir;
    if (dir !== 0 && autoScrollFrame === null) {
      autoScrollFrame = requestAnimationFrame(autoScrollStep);
    }
  }

  function autoScrollStep() {
    autoScrollFrame = null;
    if (!listEl || autoScrollDir === 0 || !draggedRepoId) return;
    const before = listEl.scrollTop;
    listEl.scrollTop = before + autoScrollDir * AUTO_SCROLL_STEP;
    if (listEl.scrollTop === before) return;
    autoScrollFrame = requestAnimationFrame(autoScrollStep);
    // The pointer is stationary while scrolling, so refresh the drop target.
    updateDropTargetFromPoint(lastPointer.x, lastPointer.y);
  }

  function handlePointerUp(e: PointerEvent) {
    if (!pointerDrag || e.pointerId !== pointerDrag.pointerId) return;
    const wasDragging = draggedRepoId !== null;
    const sourceId = draggedRepoId;
    const targetId = dragOverRepoId;
    const position = dropPosition;
    pointerDrag = null;
    stopPointerDrag();
    clearDrag();
    if (!wasDragging) return;
    // A completed drag must not also activate the repo under the pointer.
    suppressNextClick();
    if (!sourceId || !targetId || !position || sourceId === targetId) return;
    void applyReorder(sourceId, targetId, position);
  }

  function handlePointerCancel(e: PointerEvent) {
    if (!pointerDrag || e.pointerId !== pointerDrag.pointerId) return;
    const wasDragging = draggedRepoId !== null;
    pointerDrag = null;
    stopPointerDrag();
    clearDrag();
    if (wasDragging) suppressNextClick();
  }

  function handleWindowBlur() {
    pointerDrag = null;
    stopPointerDrag();
    clearDrag();
  }

  function stopPointerDrag() {
    window.removeEventListener("pointermove", handlePointerMove);
    window.removeEventListener("pointerup", handlePointerUp);
    window.removeEventListener("pointercancel", handlePointerCancel);
    window.removeEventListener("blur", handleWindowBlur);
    document.body.classList.remove("repo-dragging");
    autoScrollDir = 0;
    if (autoScrollFrame !== null) {
      cancelAnimationFrame(autoScrollFrame);
      autoScrollFrame = null;
    }
  }

  /** Swallow the click that follows a finished drag. */
  function suppressNextClick() {
    const swallow = (event: MouseEvent) => {
      event.preventDefault();
      event.stopPropagation();
      window.removeEventListener("click", swallow, true);
    };
    window.addEventListener("click", swallow, true);
    setTimeout(() => window.removeEventListener("click", swallow, true), 100);
  }

  async function applyReorder(sourceId: string, targetId: string, position: DropPosition) {
    const next = moveItem([...repoList], sourceId, targetId, position);
    if (!next) return;

    // Keep pinned status aligned with the destination region.
    const destination = repoList.find((r) => r.id === targetId);
    const moved = next.find((r) => r.id === sourceId);
    if (destination && moved) moved.pinned = destination.pinned;

    await reorderRepos(next);
  }

  /** Keyboard equivalent of drag-and-drop reordering (Alt+↑/↓ on a row). */
  async function moveRepoBy(id: string, delta: -1 | 1) {
    const list = [...repoList];
    const sourceIdx = list.findIndex((r) => r.id === id);
    const targetIdx = sourceIdx + delta;
    if (sourceIdx === -1 || targetIdx < 0 || targetIdx >= list.length) return;

    const [removed] = list.splice(sourceIdx, 1);
    // Clone so adopting the destination group does not mutate the store item
    // before `reorderRepos` rebuilds the list.
    const moved = { ...removed };
    list.splice(targetIdx, 0, moved);
    // Mirror drag-and-drop: adopt the destination row's pinned state so the
    // visual order (pinned first) matches what the user just requested.
    const destination = repoList[targetIdx];
    if (destination) moved.pinned = destination.pinned;

    await reorderRepos(list);
    // The keyed row keeps focus across the reorder; make sure it is visible.
    queueMicrotask(() => {
      document
        .querySelector<HTMLElement>(`[data-repo-id="${CSS.escape(id)}"]`)
        ?.scrollIntoView({ block: "nearest" });
    });
  }

  function clearDrag() {
    draggedRepoId = null;
    dragOverRepoId = null;
    dropPosition = null;
  }

  onDestroy(() => {
    pointerDrag = null;
    stopPointerDrag();
    document.body.classList.remove("repo-dragging");
  });
</script>

<aside
  class="relay-sidebar {$sidebarCollapsed ? 'collapsed' : ''}"
  style={$sidebarCollapsed ? "" : `width: ${width}px;`}
  ondragover={handleDragOver}
  ondragleave={handleDragLeave}
  ondrop={handleDrop}
  aria-label="Repositories"
>
  {#if isDraggingOver && !draggedRepoId}
    <div class="drop-overlay">
      <div class="drop-dashed-box">
        <span>Drop to add repository</span>
      </div>
    </div>
  {/if}

  <div class="repo-list" bind:this={listEl}>
    {#each repoList as repo (repo.id)}
      <RepoRow
        {repo}
        status={statusMap[repo.id]}
        isDragging={draggedRepoId === repo.id}
        dropIndicator={dragOverRepoId === repo.id ? dropPosition : null}
        onreorderstart={(e) => handleRowPointerDown(e, repo.id)}
        onmove={(delta) => void moveRepoBy(repo.id, delta)}
      />
    {/each}

    <div class="repo-list-end" aria-hidden="true"></div>

    <div class="dock-separator"></div>

    <button
      class="add-repo-btn"
      onclick={openAddRepo}
      aria-label="Add repository"
      title="Add repository (⌘O)"
    >
      <Icon name="plus" size={16} />
    </button>
  </div>
</aside>

<style>
  .relay-sidebar {
    height: 100%;
    background: var(--bg-subtle);
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

  .repo-list-end {
    height: 12px;
    margin: 0 6px;
  }

  .dock-separator {
    height: 1px;
    background: var(--border-subtle);
    margin: 6px 12px;
  }

  .add-repo-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 32px;
    padding: 0;
    background: transparent;
    border: none;
    border-radius: 0;
    color: var(--text-muted);
    cursor: pointer;
    transition: background-color 80ms linear;
  }
  .add-repo-btn:hover {
    background-color: var(--bg-muted);
    color: var(--text);
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
