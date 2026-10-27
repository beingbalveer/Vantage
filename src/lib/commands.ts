// Runtime side of the shortcut registry (ALPH-161).
//
// `keys.ts` owns the pure metadata + matching. This module owns the single
// handler per command/context, the global keydown router, and the shared
// "never intercept typing" guard. Components register handlers for commands
// that only make sense inside them (list navigation, hunk navigation, …).
//
// There is exactly one `window` keydown listener in the app: the
// `<svelte:window onkeydown>` in App.svelte calls `dispatchKeyboard` below.

import { get } from "svelte/store";
import {
  paletteOpen,
  settingsOpen,
  shortcutsHelpOpen,
  addRepoDialogOpen,
  scanFolderDialogOpen,
  sidebarCollapsed,
  activeDialog,
  closeDialog,
  openDialog,
  openSettingsAt,
  addToast,
} from "./stores/ui";
import {
  repos,
  sortedRepos,
  activeRepoId,
  refreshAllStatuses,
  refreshSingleStatus,
  statuses,
  loadRepos,
} from "./stores/repos";
import {
  panes,
  focusedPaneIndex,
  paneCount,
  setPaneCount,
  cyclePaneCount,
  closePane,
  openRepoInFocusedPane,
  toggleTerminalInPane,
  setPaneTab,
} from "./stores/panes";
import { settings, updateSettings } from "./stores/settings";
import * as api from "./api";
import {
  COMMANDS,
  commandById,
  resolveCommand,
  isEditableTarget,
  isTerminalTarget,
  type Command,
  type ShortcutContext,
} from "./keys";

type Handler = () => boolean | void | Promise<unknown>;

// ---- handler registries ----------------------------------------------------

/**
 * Component-scoped handlers: id -> context -> handlers (most recent first).
 * Multiple panes may register the same id; handlers must return `false` when
 * they do not own the focused pane so the next registered handler can run.
 */
const componentHandlers = new Map<string, Map<ShortcutContext, Handler[]>>();

/** Overlay dismiss hooks (menus, popovers, hover cards). */
const dismissStack: Array<() => boolean> = [];

/**
 * Register a handler for a command in a specific context. Returns an
 * unsubscribe function (use it from a Svelte `$effect`).
 */
export function registerHandler(
  id: string,
  fn: Handler,
  context: ShortcutContext = "global"
): () => void {
  let map = componentHandlers.get(id);
  if (!map) {
    map = new Map();
    componentHandlers.set(id, map);
  }
  const list = map.get(context) ?? [];
  list.push(fn);
  map.set(context, list);

  return () => {
    const current = componentHandlers.get(id);
    const handlers = current?.get(context);
    if (!current || !handlers) return;
    const idx = handlers.lastIndexOf(fn);
    if (idx >= 0) handlers.splice(idx, 1);
    if (handlers.length === 0) current.delete(context);
    if (current.size === 0) componentHandlers.delete(id);
  };
}

/** Register an overlay-dismiss hook. Returns an unsubscribe function. */
export function registerDismiss(fn: () => boolean): () => void {
  dismissStack.push(fn);
  return () => {
    const idx = dismissStack.lastIndexOf(fn);
    if (idx >= 0) dismissStack.splice(idx, 1);
  };
}

// ---- store helpers ---------------------------------------------------------

export function focusedRepoId(): string | null {
  const pane = get(panes)[activePaneIndex()];
  return pane?.repoId || get(activeRepoId) || get(repos)[0]?.id || null;
}

/** The focused pane index, clamped to the panes actually mounted. */
function activePaneIndex(): number {
  const idx = get(focusedPaneIndex);
  if (typeof document === "undefined") return idx;
  const mounted = document.querySelectorAll(".pane-container").length;
  return mounted > 0 && idx >= mounted ? mounted - 1 : idx;
}

/**
 * Keyboard ownership for pane-scoped components. Normally the focused pane
 * owns the keyboard; when only one pane is mounted (multi-pane rendering is
 * tracked separately) that pane still owns it even if the focus store points
 * at a pane that is not rendered yet.
 */
export function ownsPaneKeyboard(paneIndex: number): boolean {
  if (get(focusedPaneIndex) === paneIndex) return true;
  if (typeof document === "undefined") return true;
  return document.querySelectorAll(".pane-container").length <= 1;
}

function focusedRepo() {
  const id = focusedRepoId();
  return id ? get(repos).find((r) => r.id === id) || null : null;
}

function overlayOpen(): boolean {
  return (
    get(paletteOpen) ||
    get(settingsOpen) ||
    get(shortcutsHelpOpen) ||
    get(addRepoDialogOpen) ||
    get(scanFolderDialogOpen) ||
    get(activeDialog) !== null
  );
}

/** Contexts contributed by the app surface, ignoring overlays. */
export function getBaseContexts(): ShortcutContext[] {
  const contexts: ShortcutContext[] = ["global"];
  const pane = get(panes)[activePaneIndex()];
  if (pane?.tab === "files") {
    contexts.push("files");
  } else {
    contexts.push("history");
    // A file diff is visible alongside history whenever a file is selected.
    contexts.push("diff");
  }
  return contexts;
}

/** Active contexts for the global router, including overlay/terminal state. */
export function getActiveContexts(target?: unknown): ShortcutContext[] {
  if (overlayOpen()) return ["global", "dialog-open"];
  const contexts = getBaseContexts();
  const focusTarget =
    target ?? (typeof document !== "undefined" ? document.activeElement : null);
  if (isTerminalTarget(focusTarget)) contexts.push("terminal");
  return contexts;
}

// ---- global handlers -------------------------------------------------------

function togglePalette(): boolean {
  // Do not stack the palette on top of another modal.
  if (
    get(settingsOpen) ||
    get(shortcutsHelpOpen) ||
    get(addRepoDialogOpen) ||
    get(scanFolderDialogOpen) ||
    get(activeDialog) !== null
  ) {
    return false;
  }
  paletteOpen.update((open) => !open);
  return true;
}

function focusPane(index: number) {
  if (get(paneCount) < index + 1) setPaneCount(index + 1);
  focusedPaneIndex.set(index);
}

function closeFocusedPane() {
  const count = get(paneCount);
  if (count <= 1) {
    addToast({ message: "Cannot close the only open pane", kind: "info" });
    return;
  }
  const idx = get(focusedPaneIndex);
  closePane(idx);
}

function moveRepo(direction: 1 | -1) {
  const list = get(sortedRepos);
  if (list.length === 0) return;
  const currentId = focusedRepoId();
  const idx = list.findIndex((r) => r.id === currentId);
  const nextIdx = ((idx < 0 ? 0 : idx) + direction + list.length) % list.length;
  const next = list[nextIdx];
  activeRepoId.set(next.id);
  openRepoInFocusedPane(next.id);
}

function toggleWhitespace() {
  const current = get(settings);
  const next = !current.ignoreWhitespace;
  updateSettings({ ...current, ignoreWhitespace: next });
  addToast({ message: `Ignore whitespace: ${next ? "on" : "off"}`, kind: "info" });
}

/** Mark a repo reviewed, optionally to a specific commit (range selection). */
export async function markRepoReviewed(repoId: string, sha?: string) {
  try {
    if (sha) {
      await api.markerSet(repoId, sha);
    } else {
      await api.markerSetToHead(repoId);
    }
    addToast({ message: "Marked as reviewed", kind: "success" });
    await refreshSingleStatus(repoId);
    // The sidebar entries carry the marker too, so reload them. The status was
    // already refreshed above, so skip the full N-repo sweep.
    await loadRepos({ refresh: false });
  } catch (e: any) {
    addToast({ message: e.message || "Failed to mark as reviewed", kind: "error" });
  }
}

async function markReviewed() {
  const id = focusedRepoId();
  if (!id) {
    addToast({ message: "No focused repository to mark reviewed", kind: "info" });
    return;
  }
  await markRepoReviewed(id);
}

async function syncFocused() {
  const repo = focusedRepo();
  if (!repo) {
    addToast({ message: "No active repository to sync", kind: "info" });
    return;
  }
  const st = get(statuses)[repo.id];
  try {
    if (st?.ahead && st.ahead > 0 && !st.behind) {
      await api.syncPush(repo.id, false);
    } else if (st?.behind && st.behind > 0 && !st.ahead) {
      await api.syncPull(repo.id);
    } else if (!st?.upstream) {
      await api.syncPush(repo.id, true);
    } else {
      await api.syncFetch(repo.id);
    }
    await refreshSingleStatus(repo.id);
    addToast({ message: "Sync complete", kind: "success" });
  } catch (e: any) {
    addToast({ message: e.message || "Sync failed", kind: "error" });
  }
}

async function browseRepo() {
  try {
    const selected = await api.pickDirectory();
    if (selected) {
      const entry = await api.repoAdd(selected);
      addToast({ message: `Added repository ${entry.name}`, kind: "success" });
      await loadRepos();
    }
  } catch (e: any) {
    addToast({ message: e.message || "Failed to add repository", kind: "error" });
  }
}

async function openEditor() {
  const id = focusedRepoId();
  if (!id) {
    addToast({ message: "No active repository selected", kind: "info" });
    return;
  }
  try {
    await api.openExternalEditor(id, "");
  } catch (e: any) {
    addToast({ message: e.message || "Failed to open in editor", kind: "error" });
  }
}

async function revealFileManager() {
  const repo = focusedRepo();
  if (!repo?.path) {
    addToast({ message: "No active repository selected", kind: "info" });
    return;
  }
  try {
    await api.revealInFileManager(repo.path);
  } catch (e: any) {
    addToast({ message: e.message || "Failed to reveal in file manager", kind: "error" });
  }
}

async function copyRepoPath() {
  const repo = focusedRepo();
  if (!repo?.path) {
    addToast({ message: "No active repository selected", kind: "info" });
    return;
  }
  try {
    await navigator.clipboard.writeText(repo.path);
    addToast({ message: `Copied path: ${repo.path}`, kind: "info" });
  } catch (e: any) {
    addToast({ message: e.message || "Failed to copy path", kind: "error" });
  }
}

function removeCurrentRepo() {
  const repo = focusedRepo();
  if (!repo) {
    addToast({ message: "No active repository selected to remove", kind: "info" });
    return;
  }
  openDialog({ type: "remove_repo", repoId: repo.id, data: { repo } });
}

const SAVED_LAYOUTS_KEY = "vantage_saved_layouts";
const LEGACY_SAVED_LAYOUTS_KEY = "relay_saved_layouts";

export interface SavedLayout {
  name: string;
  paneCount: number;
  panes: { repoId: string | null; tab: "history" | "files" }[];
  savedAt?: string;
}

export function readSavedLayouts(): Record<string, SavedLayout> {
  try {
    const raw =
      localStorage.getItem(SAVED_LAYOUTS_KEY) ||
      localStorage.getItem(LEGACY_SAVED_LAYOUTS_KEY) ||
      "{}";
    const parsed = JSON.parse(raw);
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

function currentLayoutSnapshot(): Pick<SavedLayout, "paneCount" | "panes"> {
  const count = Math.max(1, Math.min(3, get(paneCount)));
  const currentPanes = get(panes);
  return {
    paneCount: count,
    panes: currentPanes.slice(0, count).map((p) => ({
      repoId: p.repoId,
      tab: p.tab,
    })),
  };
}

/** Save the current pane arrangement under a user-provided name. */
export async function saveLayoutAs(name: string): Promise<boolean> {
  const trimmed = name.trim();
  if (!trimmed) return false;
  try {
    const saved = readSavedLayouts();
    saved[trimmed] = {
      name: trimmed,
      ...currentLayoutSnapshot(),
      savedAt: new Date().toISOString(),
    };
    localStorage.setItem(SAVED_LAYOUTS_KEY, JSON.stringify(saved));
    addToast({ message: `Layout "${trimmed}" saved`, kind: "success" });
    return true;
  } catch (e: any) {
    addToast({ message: e.message || "Failed to save layout", kind: "error" });
    return false;
  }
}

/** Apply a `{ paneCount, panes }` layout object to the pane stores. */
export function applyNamedLayout(name: string, target: any) {
  if (!target) return;
  const targetCount = Math.max(1, Math.min(3, target.paneCount || 1));
  setPaneCount(targetCount);
  panes.update((list) =>
    list.map((p, i) => {
      if (i < targetCount && target.panes?.[i]) {
        return { ...p, repoId: target.panes[i].repoId, tab: target.panes[i].tab || "history" };
      }
      return p;
    })
  );
  addToast({ message: `Loaded layout "${name}"`, kind: "success" });
}

/** Load a named layout, or the backend-persisted layout when `name` is null. */
export async function loadLayoutByName(name: string | null): Promise<void> {
  try {
    const target = name ? readSavedLayouts()[name] : await api.layoutGet();
    if (!target) {
      addToast({ message: `Layout "${name}" was not found`, kind: "error" });
      return;
    }
    applyNamedLayout(name ?? "last used", target);
  } catch (e: any) {
    addToast({ message: e.message || "Failed to load layout", kind: "error" });
  }
}

/** Open the non-blocking layout dialog instead of a native `prompt()`. */
function saveLayout() {
  openDialog({ type: "layout", repoId: "", data: { mode: "save" } });
}

function loadLayout() {
  openDialog({ type: "layout", repoId: "", data: { mode: "load" } });
}

/** Close the topmost overlay or popover. Returns true when something closed. */
function handleEscape(): boolean {
  if (get(paletteOpen)) {
    paletteOpen.set(false);
    return true;
  }
  if (get(settingsOpen)) {
    settingsOpen.set(false);
    return true;
  }
  if (get(shortcutsHelpOpen)) {
    shortcutsHelpOpen.set(false);
    return true;
  }
  if (get(addRepoDialogOpen)) {
    addRepoDialogOpen.set(false);
    return true;
  }
  if (get(scanFolderDialogOpen)) {
    scanFolderDialogOpen.set(false);
    return true;
  }
  if (get(activeDialog)) {
    closeDialog();
    return true;
  }
  for (let i = dismissStack.length - 1; i >= 0; i--) {
    if (dismissStack[i]()) return true;
  }
  return false;
}

const GLOBAL_HANDLERS: Record<string, Handler> = {
  palette: togglePalette,
  add_repo: () => {
    addRepoDialogOpen.set(true);
  },
  browse_repo: browseRepo,
  toggle_sidebar: () => {
    sidebarCollapsed.update((v) => !v);
  },
  prev_repo: () => moveRepo(-1),
  next_repo: () => moveRepo(1),
  settings: () => {
    openSettingsAt("general");
  },
  shortcuts_help: () => {
    shortcutsHelpOpen.set(true);
  },
  refresh_all: async () => {
    await refreshAllStatuses();
    addToast({ message: "Refreshed all repositories", kind: "info" });
  },
  escape: handleEscape,

  focus_pane_1: () => focusPane(0),
  focus_pane_2: () => focusPane(1),
  focus_pane_3: () => focusPane(2),
  cycle_panes: () => cyclePaneCount(),
  close_pane: closeFocusedPane,
  toggle_terminal: () => toggleTerminalInPane(get(focusedPaneIndex)),

  sync: syncFocused,
  mark_reviewed: markReviewed,
  copy_path: copyRepoPath,
  toggle_whitespace: toggleWhitespace,

  open_editor: openEditor,
  reveal_file_manager: revealFileManager,
  remove_repo: removeCurrentRepo,
  save_layout: saveLayout,
  load_layout: loadLayout,
  recovery: () => {
    openSettingsAt("recovery");
  },
};

// ---- dispatch --------------------------------------------------------------

function invoke(fn: Handler): boolean {
  try {
    const result = fn();
    if (result === false) return false;
    if (result && typeof (result as Promise<unknown>).catch === "function") {
      (result as Promise<unknown>).catch((err) => {
        console.error("Command handler failed:", err);
      });
    }
    return true;
  } catch (err) {
    console.error("Command handler failed:", err);
    return false;
  }
}

function contextsForCommand(
  command: Command,
  activeContexts: ShortcutContext[]
): ShortcutContext[] {
  if (activeContexts.includes("dialog-open")) {
    return command.contexts.includes("dialog-open") ? ["dialog-open"] : [];
  }
  const contexts: ShortcutContext[] = [];
  for (const context of command.contexts) {
    if (
      context !== "global" &&
      context !== "dialog-open" &&
      activeContexts.includes(context)
    ) {
      contexts.push(context);
    }
  }
  if (command.contexts.includes("global")) contexts.push("global");
  return contexts;
}

/**
 * Run a command by id. Component handlers win over global handlers, and a
 * context-specific handler wins over the global fallback for the same id.
 * When no context is supplied (palette/menu invocation) the command's active
 * contexts are resolved from current app state.
 */
export function runCommand(id: string, context?: ShortcutContext): boolean {
  const command = commandById(id);
  const contexts = context
    ? [context]
    : command
      ? contextsForCommand(command, getActiveContexts())
      : (["global"] as ShortcutContext[]);

  const byContext = componentHandlers.get(id);
  if (byContext) {
    for (const ctx of contexts) {
      const handlers = byContext.get(ctx);
      if (!handlers) continue;
      for (let i = handlers.length - 1; i >= 0; i--) {
        if (invoke(handlers[i])) return true;
      }
    }
    if (!contexts.includes("global")) {
      const globalHandlers = byContext.get("global");
      if (globalHandlers) {
        for (let i = globalHandlers.length - 1; i >= 0; i--) {
          if (invoke(globalHandlers[i])) return true;
        }
      }
    }
  }

  const globalHandler = GLOBAL_HANDLERS[id];
  if (globalHandler) return invoke(globalHandler);
  return false;
}

/**
 * The single global keydown router. Applies the shared typing/terminal guard,
 * resolves the owning context, and runs exactly one command.
 */
export function dispatchKeyboard(e: KeyboardEvent): boolean {
  if (e.defaultPrevented) return false;
  if ((e as any).isComposing) return false;

  const editable = isEditableTarget(e.target);
  const inTerminal = isTerminalTarget(e.target);
  const resolved = resolveCommand(COMMANDS, e, {
    activeContexts: getActiveContexts(e.target),
    editable,
    inTerminal,
  });
  if (!resolved) return false;

  const handled = runCommand(resolved.command.id, resolved.context);
  if (handled && e.cancelable) e.preventDefault();
  return handled;
}

/**
 * Commands shown in the command palette for the current surface. Global and
 * palette-only commands stay visible; context commands (copy SHA, peek, …)
 * only appear where their binding can actually fire.
 */
export function paletteCommands(activeContexts: ShortcutContext[] = getBaseContexts()): Command[] {
  return COMMANDS.filter(
    (command) =>
      !command.hiddenFromPalette && contextsForCommand(command, activeContexts).length > 0
  );
}
