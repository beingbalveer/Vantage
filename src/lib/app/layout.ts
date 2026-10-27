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
 * Width of a pane's leading column. Every pane owns its own value: the commits
 * column and the file tree share it inside one pane (switching views must not
 * move the divider), but resizing one pane never moves another pane's divider.
 * Persisted under one key so a restart restores the last used width.
 */
export const LEFT_COLUMN_KEY = "vantage_left_col_width";

export function defaultLeftColumnWidth(): number {
  return viewportPaneWidths().commits;
}

export function savedLeftColumnWidth(): number {
  return savedPaneWidth(LEFT_COLUMN_KEY, defaultLeftColumnWidth());
}

export function persistLeftColumnWidth(width: number): void {
  try {
    localStorage.setItem(LEFT_COLUMN_KEY, String(Math.round(width)));
  } catch {
    // Persistence is best-effort.
  }
}
