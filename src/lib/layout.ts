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
