import { writable } from "svelte/store";

/** Approved prototype defaults; saved user dimensions take precedence. */
export function defaultPaneWidths(viewport: number) {
  if (viewport >= 1750) return { sidebar: 268, commits: 292, files: 300 };
  if (viewport <= 1400) return { sidebar: 222, commits: 244, files: 235 };
  return { sidebar: 246, commits: 274, files: 260 };
}
export function viewportPaneWidths() {
  return defaultPaneWidths(typeof window === "undefined" ? 1600 : window.innerWidth);
}
export function savedPaneWidth(key: string, fallback: number): number {
  try {
    const raw =
      localStorage.getItem(key) ??
      (key.startsWith("vantage_")
        ? localStorage.getItem(key.replace("vantage_", "relay_"))
        : null);
    const value = Number(raw);
    return Number.isFinite(value) && value >= 100 ? value : fallback;
  } catch { return fallback; }
}

/**
 * Width of the pane's leading column. The commits column and the file tree are
 * the same edge on screen, so they share one value: switching views must not
 * move the divider. Persisted under a single key, seeded from the older
 * per-view keys so existing layouts keep their width.
 */
const LEFT_COLUMN_KEY = "vantage_left_col_width";

function loadLeftColumnWidth(): number {
  try {
    const raw =
      localStorage.getItem(LEFT_COLUMN_KEY) ??
      localStorage.getItem("vantage_commits_col_width") ??
      localStorage.getItem("vantage_files_tree_width");
    const value = Number(raw);
    if (Number.isFinite(value) && value >= 100) return value;
  } catch {
    // Storage can be unavailable; fall through to the viewport default.
  }
  return viewportPaneWidths().commits;
}

export const leftColumnWidth = writable<number>(loadLeftColumnWidth());

export function setLeftColumnWidth(width: number) {
  leftColumnWidth.set(width);
  try {
    localStorage.setItem(LEFT_COLUMN_KEY, String(Math.round(width)));
  } catch {
    // Persistence is best-effort.
  }
}
