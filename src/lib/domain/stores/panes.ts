import { writable, get } from "svelte/store";
import type { PaneLayout } from "../../platform/types";
import { repos, sortedRepos } from "./repos";

export interface PaneState {
  id: string;
  repoId: string | null;
  tab: "history" | "files";
  terminalOpen: boolean;
  terminalHeight: number;
}

function getSavedTerminalHeight(): number {
  if (typeof window === "undefined" || !window.localStorage) return 240;
  try {
    const saved = localStorage.getItem("vantage_terminal_height");
    if (saved) {
      const val = parseInt(saved, 10);
      if (!Number.isNaN(val) && val >= 80 && val <= 1200) {
        return val;
      }
    }
  } catch {}
  return 240;
}

const initialTerminalHeight = getSavedTerminalHeight();

export const paneCount = writable<number>(1);
export const focusedPaneIndex = writable<number>(0);
export const panes = writable<PaneState[]>([
  {
    id: "pane-0",
    repoId: null,
    tab: "history",
    terminalOpen: false,
    terminalHeight: initialTerminalHeight,
  },
  {
    id: "pane-1",
    repoId: null,
    tab: "history",
    terminalOpen: false,
    terminalHeight: initialTerminalHeight,
  },
  {
    id: "pane-2",
    repoId: null,
    tab: "history",
    terminalOpen: false,
    terminalHeight: initialTerminalHeight,
  },
]);

export function focusPane(index: number) {
  if (index < 0 || index > 2) return;
  // Never leave focus on a pane that is not rendered: reveal it first, then focus.
  if (index >= get(paneCount)) {
    setPaneCount(index + 1);
  }
  focusedPaneIndex.set(index);
}

export function cyclePaneCount() {
  const current = get(paneCount);
  const next = current >= 3 ? 1 : current + 1;
  setPaneCount(next);
}

export function setPaneCount(count: number) {
  const bounded = Math.max(1, Math.min(3, count));
  paneCount.set(bounded);

  // If new pane opened and has no repo, assign next available repo
  panes.update((list) => {
    const sorted = get(sortedRepos);
    return list.map((p, i) => {
      if (i < bounded && !p.repoId && sorted[i]) {
        return { ...p, repoId: sorted[i].id };
      }
      return { ...p };
    });
  });

  const f = get(focusedPaneIndex);
  if (f >= bounded) {
    focusedPaneIndex.set(bounded - 1);
  }
}

export function openRepoInFocusedPane(repoId: string) {
  const count = get(paneCount);
  const currentF = get(focusedPaneIndex);
  const idx = Math.max(0, Math.min(count - 1, currentF));
  panes.update((list) => {
    return list.map((p, i) => (i === idx ? { ...p, repoId } : { ...p }));
  });
}

export function openRepoInNextPane(repoId: string) {
  const currentCount = get(paneCount);
  if (currentCount < 3) {
    setPaneCount(currentCount + 1);
    focusedPaneIndex.set(currentCount);
    panes.update((list) => {
      return list.map((p, i) => (i === currentCount ? { ...p, repoId } : { ...p }));
    });
  } else {
    const nextIdx = (get(focusedPaneIndex) + 1) % 3;
    focusedPaneIndex.set(nextIdx);
    panes.update((list) => {
      return list.map((p, i) => (i === nextIdx ? { ...p, repoId } : { ...p }));
    });
  }
}

export function toggleTerminalInPane(paneIdx: number) {
  panes.update((list) => {
    return list.map((p, i) => (i === paneIdx ? { ...p, terminalOpen: !p.terminalOpen } : { ...p }));
  });
}

export function setPaneTab(paneIdx: number, tab: "history" | "files") {
  panes.update((list) => {
    return list.map((p, i) => (i === paneIdx ? { ...p, tab } : { ...p }));
  });
}

export function closePane(index: number) {
  const count = get(paneCount);
  if (count <= 1) return;
  panes.update((list) => {
    const nextList = list.filter((_, i) => i !== index);
    while (nextList.length < 3) {
      nextList.push({
        id: `pane-${Date.now()}-${nextList.length}`,
        repoId: null,
        tab: "history",
        terminalOpen: false,
        terminalHeight: 240,
      });
    }
    return nextList;
  });
  const newCount = count - 1;
  paneCount.set(newCount);
  const currentF = get(focusedPaneIndex);
  if (currentF >= newCount) {
    focusedPaneIndex.set(newCount - 1);
  }
}

