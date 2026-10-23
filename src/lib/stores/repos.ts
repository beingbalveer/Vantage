import { writable, derived, get } from "svelte/store";
import type { RepoEntry, RepoStatus } from "../types";
import * as api from "../api";

export const repos = writable<RepoEntry[]>([]);
export const statuses = writable<Record<string, RepoStatus>>({});
export const activeRepoId = writable<string | null>(null);

// Derived sorted repos (pinned first, then by lastActivityAt descending)
export const sortedRepos = derived([repos, statuses], ([$repos, $statuses]) => {
  return [...$repos].sort((a, b) => {
    if (a.pinned !== b.pinned) {
      return a.pinned ? -1 : 1;
    }
    const statA = $statuses[a.id];
    const statB = $statuses[b.id];
    const timeA = statA?.lastActivityAt ? new Date(statA.lastActivityAt).getTime() : 0;
    const timeB = statB?.lastActivityAt ? new Date(statB.lastActivityAt).getTime() : 0;
    return timeB - timeA;
  });
});

export async function loadRepos() {
  try {
    const list = await api.repoList();
    repos.set(list);
    if (list.length > 0) {
      const curActive = get(activeRepoId);
      if (!curActive || !list.some((r) => r.id === curActive)) {
        activeRepoId.set(list[0].id);
      }
      // Ensure backend review markers are updated to HEAD to clear legacy unreviewed markers
      for (const r of list) {
        if (r.marker) {
          api.markerSetToHead(r.id).catch(() => {});
        }
      }
    } else {
      activeRepoId.set(null);
    }
    await refreshAllStatuses();
  } catch (e) {
    console.error("Failed to load repos:", e);
  }
}

export async function removeRepo(repoId: string): Promise<void> {
  await api.repoRemove(repoId);
  const currentList = get(repos);
  const remaining = currentList.filter((r) => r.id !== repoId);
  const nextId = remaining.length > 0 ? remaining[0].id : null;

  // Update active repo if needed
  if (get(activeRepoId) === repoId) {
    activeRepoId.set(nextId);
  }

  // Update repo list
  repos.set(remaining);

  // Reload repo list and statuses from backend
  await loadRepos();
}

export async function refreshAllStatuses() {
  try {
    const statList = await api.repoRefreshAll();
    statuses.update((prev) => {
      const next = { ...prev };
      for (const s of statList) {
        next[s.repoId] = s;
      }
      return next;
    });
  } catch (e) {
    console.error("Failed to refresh statuses:", e);
  }
}

export async function refreshSingleStatus(repoId: string) {
  try {
    const s = await api.repoStatus(repoId);
    statuses.update((prev) => ({ ...prev, [repoId]: s }));
  } catch (e) {
    console.error(`Failed to refresh status for ${repoId}:`, e);
  }
}

// Setup real-time event listeners
export function initRepoListeners() {
  api.onRepoStatus((status) => {
    statuses.update((prev) => ({ ...prev, [status.repoId]: status }));
  });

  api.onHeadChanged(({ repoId }) => {
    refreshSingleStatus(repoId);
  });
}
