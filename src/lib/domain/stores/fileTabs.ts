import { writable } from "svelte/store";

/**
 * Open editor tabs for the file browser, keyed by repository so a repo keeps
 * its open files when the pane switches between Commits and Files (or the pane
 * is remounted). Only paths are stored; content lives in the viewer.
 */
export interface FileTabsState {
  tabs: string[];
  active: string | null;
}

export type FileTabsMap = Record<string, FileTabsState>;

const EMPTY: FileTabsState = { tabs: [], active: null };

export const fileTabs = writable<FileTabsMap>({});

export function getFileTabState(map: FileTabsMap, repoId: string): FileTabsState {
  return map[repoId] ?? EMPTY;
}

/** Open (or focus, when already open) a file tab. */
export function openFileTab(repoId: string, path: string): void {
  if (!repoId || !path) return;
  fileTabs.update((map) => {
    const current = map[repoId] ?? EMPTY;
    if (current.active === path && current.tabs.includes(path)) return map;
    const tabs = current.tabs.includes(path) ? current.tabs : [...current.tabs, path];
    return { ...map, [repoId]: { tabs, active: path } };
  });
}

export function activateFileTab(repoId: string, path: string): void {
  if (!repoId || !path) return;
  fileTabs.update((map) => {
    const current = map[repoId] ?? EMPTY;
    if (!current.tabs.includes(path) || current.active === path) return map;
    return { ...map, [repoId]: { tabs: current.tabs, active: path } };
  });
}

/** Close a tab; focus the neighbour that takes its place. */
export function closeFileTab(repoId: string, path: string): void {
  if (!repoId) return;
  fileTabs.update((map) => {
    const current = map[repoId];
    if (!current) return map;
    const index = current.tabs.indexOf(path);
    if (index < 0) return map;
    const tabs = current.tabs.filter((tab) => tab !== path);
    const active =
      current.active === path
        ? tabs[Math.min(index, tabs.length - 1)] ?? null
        : current.active;
    return { ...map, [repoId]: { tabs, active } };
  });
}

/** Drop tabs whose files no longer exist (tree refresh after a checkout). */
export function pruneFileTabs(repoId: string, existing: string[]): void {
  if (!repoId) return;
  const alive = new Set(existing);
  fileTabs.update((map) => {
    const current = map[repoId];
    if (!current || current.tabs.every((tab) => alive.has(tab))) return map;
    const tabs = current.tabs.filter((tab) => alive.has(tab));
    const active = current.active && alive.has(current.active) ? current.active : tabs[0] ?? null;
    return { ...map, [repoId]: { tabs, active } };
  });
}
