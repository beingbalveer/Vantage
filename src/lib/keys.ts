export interface Shortcut {
  id: string;
  key: string;
  description: string;
  category: "Navigation" | "Review" | "Diff" | "Panes" | "Terminal";
  global?: boolean;
}

export const SHORTCUTS: Shortcut[] = [
  { id: "palette", key: "Mod+K", description: "Command palette", category: "Navigation", global: true },
  { id: "add_repo", key: "Mod+O", description: "Add repository", category: "Navigation", global: true },
  { id: "focus_pane_1", key: "Mod+1", description: "Focus pane 1", category: "Panes", global: true },
  { id: "focus_pane_2", key: "Mod+2", description: "Focus pane 2", category: "Panes", global: true },
  { id: "focus_pane_3", key: "Mod+3", description: "Focus pane 3", category: "Panes", global: true },
  { id: "cycle_panes", key: "Mod+\\", description: "Cycle pane count 1, 2, 3", category: "Panes", global: true },
  { id: "close_pane", key: "Mod+W", description: "Close focused pane", category: "Panes", global: true },
  { id: "save_layout", key: "", description: "Save layout as…", category: "Panes" },
  { id: "load_layout", key: "", description: "Load layout…", category: "Panes" },
  { id: "toggle_sidebar", key: "Mod+B", description: "Toggle sidebar", category: "Navigation", global: true },
  { id: "toggle_terminal", key: "Mod+`", description: "Toggle terminal drawer", category: "Terminal", global: true },
  { id: "prev_repo", key: "Mod+[", description: "Previous repository", category: "Navigation", global: true },
  { id: "next_repo", key: "Mod+]", description: "Next repository", category: "Navigation", global: true },
  { id: "settings", key: "Mod+,", description: "Settings", category: "Navigation", global: true },
  { id: "refresh_all", key: "Mod+Shift+R", description: "Refresh all repos", category: "Navigation", global: true },
  { id: "sync", key: "Mod+Shift+F", description: "Fetch / Pull / Push", category: "Review", global: true },

  { id: "list_down", key: "j", description: "Next item in list", category: "Navigation" },
  { id: "list_up", key: "k", description: "Previous item in list", category: "Navigation" },
  { id: "extend_down", key: "Shift+j", description: "Extend commit selection down", category: "Review" },
  { id: "extend_up", key: "Shift+k", description: "Extend commit selection up", category: "Review" },
  { id: "prev_flag", key: "{", description: "Previous flagged commit", category: "Review" },
  { id: "next_flag", key: "}", description: "Next flagged commit", category: "Review" },
  { id: "enter_focus", key: "Enter", description: "Focus file list / diff", category: "Navigation" },
  { id: "escape", key: "Esc", description: "Back / close dialog / palette", category: "Navigation" },
  { id: "next_file", key: "]", description: "Next file in diff", category: "Diff" },
  { id: "prev_file", key: "[", description: "Previous file in diff", category: "Diff" },
  { id: "next_hunk", key: "n", description: "Next change hunk", category: "Diff" },
  { id: "prev_hunk", key: "p", description: "Previous change hunk", category: "Diff" },
  { id: "expand_context", key: "e", description: "Expand all context in current file", category: "Diff" },
  { id: "toggle_whitespace", key: "w", description: "Toggle ignore whitespace", category: "Diff" },
  { id: "copy_sha", key: "c", description: "Copy SHA of selected commit", category: "Review" },
  { id: "search", key: "/", description: "Search history or filter files", category: "Navigation" },
  { id: "peek_def", key: "F12", description: "Peek definition", category: "Diff" },
  { id: "help", key: "?", description: "Keyboard shortcuts help", category: "Navigation" },
];

export function isMac(): boolean {
  return typeof navigator !== "undefined" && /Mac|iPod|iPhone|iPad/.test(navigator.platform);
}

export function formatKey(key: string): string {
  if (!key) return "";
  const mac = isMac();
  return key
    .replace(/Mod/g, mac ? "⌘" : "Ctrl")
    .replace(/Shift/g, "⇧")
    .replace(/Alt/g, mac ? "⌥" : "Alt")
    .replace(/Enter/g, "↵")
    .replace(/Esc/g, "esc");
}

export function matchShortcut(e: KeyboardEvent, shortcutKey: string): boolean {
  if (!shortcutKey) return false;
  const mac = isMac();
  const parts = shortcutKey.split("+");
  const baseKey = parts[parts.length - 1];

  const needsMod = parts.includes("Mod");
  const needsCtrl = parts.includes("Ctrl");
  const needsShift = parts.includes("Shift");
  const needsAlt = parts.includes("Alt");

  const modPressed = mac ? e.metaKey : e.ctrlKey;
  if (needsMod && !modPressed) return false;
  if (!needsMod && !needsCtrl && (mac ? e.metaKey : e.ctrlKey)) return false;
  if (needsCtrl && !e.ctrlKey) return false;
  if (needsShift && !e.shiftKey) return false;
  if (!needsShift && e.shiftKey && baseKey !== "{" && baseKey !== "}" && baseKey !== "?") return false;
  if (needsAlt && !e.altKey) return false;
  if (!needsAlt && e.altKey) return false;

  if (baseKey === "`" && (e.key === "`" || e.code === "Backquote")) return true;
  if (baseKey === "\\" && (e.key === "\\" || e.code === "Backslash")) return true;
  if (baseKey === "{" && (e.key === "{" || (e.shiftKey && (e.key === "[" || e.code === "BracketLeft")))) return true;
  if (baseKey === "}" && (e.key === "}" || (e.shiftKey && (e.key === "]" || e.code === "BracketRight")))) return true;
  if (baseKey === "?" && (e.key === "?" || (e.shiftKey && (e.key === "/" || e.code === "Slash")))) return true;
  if (baseKey.toLowerCase() === e.key.toLowerCase()) return true;

  return false;
}

export function findFlaggedCommitIndex<T extends { flags?: any[] }>(
  commits: T[],
  currentIndex: number,
  direction: "next" | "prev"
): number {
  if (!commits || commits.length === 0) return currentIndex;
  const step = direction === "next" ? 1 : -1;
  let idx = currentIndex + step;
  while (idx >= 0 && idx < commits.length) {
    if (commits[idx].flags && commits[idx].flags!.length > 0) {
      return idx;
    }
    idx += step;
  }
  return currentIndex;
}
