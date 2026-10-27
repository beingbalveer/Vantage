// Central keyboard shortcut registry (PRODUCT_SPEC.md §13, ALPH-161).
//
// This module is intentionally dependency-free and pure so matching and
// context-resolution logic is unit-testable without a DOM. Labels and command
// ids live here once and are consumed by the command palette, the Help dialog
// and the global keydown router in `commands.ts`.
//
// Ownership rules:
//   - A command's `keys` are canonical strings understood by `matchShortcut`.
//   - A command's `contexts` describe where it may fire. `global` means any
//     non-editable surface without an overlay; specific contexts win over
//     `global`, so a key bound twice still resolves to one command per context.
//   - Inputs, textareas, selects, contentEditable and the xterm terminal are
//     guarded centrally (`isEditableTarget` / `isTerminalTarget`). A command
//     opts back in with `allowWhenTyping` or `allowInTerminal`.

export type ShortcutCategory = "Navigation" | "Review" | "Diff" | "Panes" | "Terminal";

/**
 * The contexts a command can be active in.
 *
 * - `global`      any non-editable surface (default)
 * - `history`     the focused pane shows its commit history
 * - `diff`        the focused pane can show a file diff
 * - `files`       the focused pane shows the file browser
 * - `terminal`    the embedded terminal owns focus
 * - `dialog-open` an overlay (palette / settings / help / modal) is open
 */
export type ShortcutContext =
  | "global"
  | "history"
  | "diff"
  | "files"
  | "terminal"
  | "dialog-open";

export interface Command {
  id: string;
  /** The one and only human label — shared with the palette and the Help dialog. */
  label: string;
  /** Canonical keybindings, e.g. "Mod+K". Empty = palette/dialog only. */
  keys: string[];
  /** Contexts this command is active in. */
  contexts: ShortcutContext[];
  category: ShortcutCategory;
  /** May fire while focus is in an input / textarea / contentEditable. */
  allowWhenTyping?: boolean;
  /** May fire while focus is inside the xterm terminal. */
  allowInTerminal?: boolean;
  /** Platform-specific bindings used instead of `keys` inside the terminal. */
  terminalKeys?: string[];
  /** Hide from the command palette (still shown in Help when it has a key). */
  hiddenFromPalette?: boolean;
}

export interface Shortcut {
  id: string;
  key: string;
  description: string;
  category: ShortcutCategory;
  global?: boolean;
}

export const SHORTCUTS: Shortcut[] = [
  { id: "palette", key: "Mod+K", description: "Command palette", category: "Navigation", global: true },
  { id: "quick_open", key: "Mod+P", description: "Go to file / Quick open", category: "Navigation", global: true },
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
  { id: "expand_context", key: "e", description: "Expand whole file (all context)", category: "Diff" },
  { id: "toggle_whitespace", key: "w", description: "Toggle ignore whitespace", category: "Diff" },
  { id: "toggle_diff_mode", key: "s", description: "Toggle unified / split diff", category: "Diff" },
  { id: "copy_sha", key: "c", description: "Copy SHA of selected commit", category: "Review" },
  { id: "search", key: "/", description: "Search history or filter files", category: "Navigation" },
  { id: "peek_def", key: "F12", description: "Peek definition", category: "Diff" },
  { id: "help", key: "?", description: "Keyboard shortcuts help", category: "Navigation" },
];

/**
 * The authoritative command list. Labels and bindings exist here and nowhere
 * else; the palette and Help dialog render directly from it.
 *
 * Keep README.md's shortcut table in sync with this list. If a feature needed
 * to make a binding work is not on this branch, leave the command out rather
 * than advertising a dead key.
 */
export const COMMANDS: Command[] = [
  // ---- Global navigation ----------------------------------------------------
  {
    id: "palette",
    label: "Command palette",
    keys: ["Mod+K", "Ctrl+Shift+P"],
    // The palette is its own overlay: allow it to close with the same key.
    contexts: ["global", "dialog-open"],
    category: "Navigation",
    allowInTerminal: true,
    // On Windows/Linux Ctrl+K belongs to the shell, so the terminal only
    // accepts the VS Code-style chord (PRODUCT_SPEC F8/F13).
    terminalKeys: ["Ctrl+Shift+P"],
    hiddenFromPalette: true,
  },
  { id: "add_repo", label: "Add repository…", keys: ["Mod+O"], contexts: ["global"], category: "Navigation" },
  { id: "browse_repo", label: "Browse and add repository…", keys: [], contexts: ["global"], category: "Navigation" },
  {
    id: "toggle_sidebar",
    label: "Toggle sidebar",
    keys: ["Mod+B"],
    contexts: ["global"],
    category: "Navigation",
    allowInTerminal: true,
  },
  {
    id: "prev_repo",
    label: "Previous repository",
    keys: ["Mod+["],
    contexts: ["global"],
    category: "Navigation",
    allowInTerminal: true,
  },
  {
    id: "next_repo",
    label: "Next repository",
    keys: ["Mod+]"],
    contexts: ["global"],
    category: "Navigation",
    allowInTerminal: true,
  },
  { id: "settings", label: "Open settings", keys: ["Mod+,"], contexts: ["global"], category: "Navigation" },
  {
    id: "shortcuts_help",
    label: "Keyboard shortcuts help",
    keys: ["?"],
    contexts: ["global"],
    category: "Navigation",
  },
  {
    id: "switch_tab",
    label: "Switch tab (History / Files)",
    keys: ["t"],
    contexts: ["history", "files"],
    category: "Navigation",
  },
  {
    id: "refresh_all",
    label: "Refresh all repositories",
    keys: ["Mod+Shift+R"],
    contexts: ["global"],
    category: "Navigation",
  },
  {
    id: "escape",
    label: "Close dialog, palette, or popover",
    keys: ["Esc"],
    contexts: ["global", "dialog-open"],
    category: "Navigation",
    // Escape is not typing; it must always be able to dismiss a surface.
    allowWhenTyping: true,
    hiddenFromPalette: true,
  },

  // ---- Panes ---------------------------------------------------------------
  {
    id: "focus_pane_1",
    label: "Focus pane 1",
    keys: ["Mod+1"],
    contexts: ["global"],
    category: "Panes",
    allowInTerminal: true,
  },
  {
    id: "focus_pane_2",
    label: "Focus pane 2",
    keys: ["Mod+2"],
    contexts: ["global"],
    category: "Panes",
    allowInTerminal: true,
  },
  {
    id: "focus_pane_3",
    label: "Focus pane 3",
    keys: ["Mod+3"],
    contexts: ["global"],
    category: "Panes",
    allowInTerminal: true,
  },
  {
    id: "cycle_panes",
    label: "Cycle pane count (1, 2, 3)",
    keys: ["Mod+\\"],
    contexts: ["global"],
    category: "Panes",
    allowInTerminal: true,
  },
  { id: "close_pane", label: "Close focused pane", keys: ["Mod+W"], contexts: ["global"], category: "Panes" },
  {
    id: "toggle_terminal",
    label: "Toggle terminal drawer",
    keys: ["Mod+`"],
    contexts: ["global"],
    category: "Terminal",
    allowInTerminal: true,
  },

  // ---- Focused lists / review ----------------------------------------------
  {
    id: "list_down",
    label: "Next item in focused list",
    keys: ["j"],
    contexts: ["history", "files"],
    category: "Navigation",
  },
  {
    id: "list_up",
    label: "Previous item in focused list",
    keys: ["k"],
    contexts: ["history", "files"],
    category: "Navigation",
  },
  {
    id: "extend_down",
    label: "Extend commit selection down",
    keys: ["Shift+j"],
    contexts: ["history"],
    category: "Review",
  },
  {
    id: "extend_up",
    label: "Extend commit selection up",
    keys: ["Shift+k"],
    contexts: ["history"],
    category: "Review",
  },
  { id: "prev_flag", label: "Previous flagged commit", keys: ["{"], contexts: ["history"], category: "Review" },
  { id: "next_flag", label: "Next flagged commit", keys: ["}"], contexts: ["history"], category: "Review" },
  {
    id: "enter_focus",
    label: "Move focus: commits → files → diff (History tab)",
    keys: ["Enter"],
    contexts: ["history"],
    category: "Navigation",
  },
  {
    id: "mark_reviewed",
    label: "Mark reviewed (advance marker to HEAD)",
    keys: ["r"],
    contexts: ["history", "files"],
    category: "Review",
  },
  {
    id: "copy_sha",
    label: "Copy SHA of selected commit (History tab)",
    keys: ["c"],
    contexts: ["history"],
    category: "Review",
  },
  {
    id: "copy_path",
    label: "Copy repository path (Files tab)",
    keys: ["c"],
    contexts: ["files"],
    category: "Navigation",
  },
  {
    id: "commit",
    label: "Commit selected files (commit box)",
    keys: ["Mod+Enter"],
    contexts: ["history"],
    category: "Review",
    allowWhenTyping: true,
    hiddenFromPalette: true,
  },
  {
    id: "focus_files",
    label: "Focus the file tree (Files tab)",
    keys: ["/"],
    contexts: ["files"],
    category: "Navigation",
  },
  {
    id: "sync",
    label: "Fetch / Pull / Push (Sync)",
    keys: ["Mod+Shift+F"],
    contexts: ["global"],
    category: "Review",
  },

  // ---- Diff -----------------------------------------------------------------
  { id: "next_file", label: "Next file in diff", keys: ["]"], contexts: ["diff"], category: "Diff" },
  { id: "prev_file", label: "Previous file in diff", keys: ["["], contexts: ["diff"], category: "Diff" },
  { id: "next_hunk", label: "Next change hunk", keys: ["n"], contexts: ["diff"], category: "Diff" },
  { id: "prev_hunk", label: "Previous change hunk", keys: ["p"], contexts: ["diff"], category: "Diff" },
  {
    id: "expand_context",
    label: "Expand all context in current file",
    keys: ["e"],
    contexts: ["diff"],
    category: "Diff",
  },
  {
    id: "toggle_whitespace",
    label: "Toggle ignore whitespace",
    keys: ["w"],
    contexts: ["global"],
    category: "Diff",
  },
  {
    id: "peek_def",
    label: "Peek definition (code viewer)",
    keys: ["F12"],
    contexts: ["files"],
    category: "Diff",
  },
  {
    id: "find_in_file",
    label: "Find in file (code viewer)",
    keys: ["Mod+F"],
    contexts: ["files"],
    category: "Diff",
    allowWhenTyping: true,
  },

  // ---- Palette-only commands (labels still live here) -----------------------
  { id: "open_editor", label: "Open in external editor", keys: [], contexts: ["global"], category: "Review" },
  { id: "reveal_file_manager", label: "Reveal repository in file manager", keys: [], contexts: ["global"], category: "Navigation" },
  { id: "save_layout", label: "Save layout as…", keys: [], contexts: ["global"], category: "Panes" },
  { id: "load_layout", label: "Load layout…", keys: [], contexts: ["global"], category: "Panes" },
  {
    id: "toggle_diff_mode",
    label: "Toggle unified / split diff",
    keys: ["s"],
    contexts: ["diff"],
    category: "Diff",
  },
  { id: "recovery", label: "Recovery (backups)", keys: [], contexts: ["global"], category: "Review" },
  { id: "remove_repo", label: "Remove current repository…", keys: [], contexts: ["global"], category: "Navigation" },
];

export const SHORTCUT_CATEGORIES: ShortcutCategory[] = [
  "Navigation",
  "Review",
  "Diff",
  "Panes",
  "Terminal",
];

export function commandById(id: string): Command | undefined {
  return COMMANDS.find((c) => c.id === id);
}

/** Primary keybinding for a command, used for palette/tooltip badges. */
export function shortcutFor(id: string): string {
  return commandById(id)?.keys[0] ?? "";
}

/** Human-formatted list of keybindings, e.g. "⌘K · ⇧⌘P". */
export function formatKeys(keys: string[], mac: boolean = isMac()): string {
  return keys.map((key) => formatKey(key, mac)).filter(Boolean).join(" · ");
}

export function isMac(): boolean {
  return typeof navigator !== "undefined" && /Mac|iPod|iPhone|iPad/.test(navigator.platform);
}

export function formatKey(key: string, mac: boolean = isMac()): string {
  if (!key) return "";
  return key
    .replace(/Ctrl/g, mac ? "⌃" : "Ctrl")
    .replace(/Mod/g, mac ? "⌘" : "Ctrl")
    .replace(/Shift/g, "⇧")
    .replace(/Alt/g, mac ? "⌥" : "Alt")
    .replace(/Enter/g, "↵")
    .replace(/Esc/g, "esc");
}

/** Minimal keyboard event shape so matching can be tested without a DOM. */
export interface KeyEventLike {
  key: string;
  code?: string;
  metaKey: boolean;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
}

export function matchShortcut(e: KeyEventLike, shortcutKey: string, mac: boolean = isMac()): boolean {
  if (!shortcutKey) return false;
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
  // `{`, `}`, `?` are shifted characters on US layouts, but Shift is inherent
  // in the produced key rather than an extra chord.
  if (!needsShift && e.shiftKey && baseKey !== "{" && baseKey !== "}" && baseKey !== "?") return false;
  if (needsAlt && !e.altKey) return false;
  if (!needsAlt && e.altKey) return false;

  if (baseKey === "`" && (e.key === "`" || e.code === "Backquote")) return true;
  if (baseKey === "\\" && (e.key === "\\" || e.code === "Backslash")) return true;
  if (baseKey === "{" && (e.key === "{" || (e.shiftKey && (e.key === "[" || e.code === "BracketLeft")))) return true;
  if (baseKey === "}" && (e.key === "}" || (e.shiftKey && (e.key === "]" || e.code === "BracketRight")))) return true;
  if (baseKey === "?" && (e.key === "?" || (e.shiftKey && (e.key === "/" || e.code === "Slash")))) return true;
  if (baseKey === "Esc") return e.key === "Escape" || e.key === "Esc";
  if (baseKey.toLowerCase() === e.key.toLowerCase()) return true;

  return false;
}

function elementOf(target: unknown): Element | null {
  const el = target as any;
  if (!el) return null;
  if (el.nodeType === 3) return el.parentElement ?? null;
  return typeof el.closest === "function" ? (el as Element) : null;
}

/** True when the event target is inside the xterm terminal (hidden textarea). */
export function isTerminalTarget(target: unknown): boolean {
  const el = elementOf(target);
  return !!(el && el.closest(".xterm"));
}

/**
 * Shared guard: is the event headed for something the user is typing into?
 * Covers form controls, contentEditable, ARIA textboxes, and the xterm terminal
 * (whose hidden textarea lives inside `.xterm`).
 */
export function isEditableTarget(target: unknown): boolean {
  const el = elementOf(target) as any;
  if (!el) return false;
  const tag = typeof el.tagName === "string" ? el.tagName.toUpperCase() : "";
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return true;
  if (el.isContentEditable === true) return true;
  if (typeof el.closest === "function" && el.closest('[contenteditable="true"], [role="textbox"]')) return true;
  if (isTerminalTarget(el)) return true;
  return false;
}

export interface ResolveOptions {
  activeContexts: ShortcutContext[];
  /** Focus is in an input / textarea / contentEditable surface. */
  editable?: boolean;
  /** Focus is inside the xterm terminal. */
  inTerminal?: boolean;
  mac?: boolean;
}

export interface ResolvedCommand {
  command: Command;
  /** The keybinding that matched (canonical form). */
  key: string;
  /** The winning context; "global" when no specific context applied. */
  context: ShortcutContext;
}

interface ScoredCommand extends ResolvedCommand {
  /** Position in COMMANDS, used as the final deterministic tie-breaker. */
  order: number;
}

function winningContext(
  command: Command,
  activeContexts: ShortcutContext[],
  dialogOpen: boolean
): ShortcutContext | null {
  if (dialogOpen) {
    // While an overlay is open only commands that opt into `dialog-open` fire.
    return command.contexts.includes("dialog-open") ? "dialog-open" : null;
  }

  const specific = command.contexts.filter((c) => c !== "global" && c !== "dialog-open");
  const hit = specific.find((c) => activeContexts.includes(c));
  if (hit) return hit;
  if (command.contexts.includes("global")) return "global";
  return null;
}

function contextScore(context: ShortcutContext): number {
  if (context === "dialog-open") return 2;
  if (context === "global") return 0;
  return 1;
}

function modifierCount(key: string): number {
  return key.split("+").length - 1;
}

/**
 * Resolve a keyboard event to exactly one command.
 *
 * Ordering is deterministic:
 *   1. context-specific bindings beat `global` ones,
 *   2. more modifiers beat fewer (Mod+Shift+F beats Mod+F if both matched),
 *   3. the earlier entry in `COMMANDS` wins.
 *
 * The typing/terminal guards are applied here, centrally, so no component can
 * accidentally intercept a keystroke meant for an input or the shell.
 */
export function resolveCommand(
  commands: Command[],
  e: KeyEventLike,
  options: ResolveOptions
): ResolvedCommand | null {
  const { activeContexts, editable = false, inTerminal = false, mac = isMac() } = options;
  const dialogOpen = activeContexts.includes("dialog-open");
  const candidates: ScoredCommand[] = [];

  commands.forEach((command, order) => {
    if (inTerminal) {
      if (!command.allowInTerminal) return;
    } else if (editable && !command.allowWhenTyping) {
      return;
    }

    const keys =
      inTerminal && !mac && command.terminalKeys ? command.terminalKeys : command.keys;
    const key = keys.find((k) => matchShortcut(e, k, mac));
    if (!key) return;

    const context = winningContext(command, activeContexts, dialogOpen);
    if (!context) return;

    candidates.push({ command, key, context, order });
  });

  if (candidates.length === 0) return null;

  candidates.sort(
    (a, b) =>
      contextScore(b.context) - contextScore(a.context) ||
      modifierCount(b.key) - modifierCount(a.key) ||
      a.order - b.order
  );

  const winner = candidates[0];
  return { command: winner.command, key: winner.key, context: winner.context };
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
