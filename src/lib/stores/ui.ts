import { writable } from "svelte/store";

export interface ToastMessage {
  id: string;
  message: string;
  kind?: "info" | "error" | "success";
  actionLabel?: string;
  action?: () => void;
  /** Optional secondary link rendered next to the action (e.g. "Recovery"). */
  linkLabel?: string;
  linkAction?: () => void;
  details?: string;
}

export const toasts = writable<ToastMessage[]>([]);

const toastTimers = new Map<string, { timeoutId: any; remaining: number; startedAt: number }>();

export function addToast(toast: Omit<ToastMessage, "id">) {
  const id = "toast-" + Math.random().toString(36).substring(2, 9);
  const newToast: ToastMessage = { ...toast, id };

  toasts.update((list) => [...list.slice(-2), newToast]); // max 3 stacked

  const duration = toast.actionLabel || toast.linkLabel || toast.details ? 8000 : 5000;
  const timerData = {
    timeoutId: setTimeout(() => {
      removeToast(id);
    }, duration),
    remaining: duration,
    startedAt: Date.now(),
  };
  toastTimers.set(id, timerData);

  return id;
}

export function pauseToastTimer(id: string) {
  const t = toastTimers.get(id);
  if (t) {
    clearTimeout(t.timeoutId);
    const elapsed = Date.now() - t.startedAt;
    t.remaining = Math.max(1500, t.remaining - elapsed);
  }
}

export function resumeToastTimer(id: string) {
  const t = toastTimers.get(id);
  if (t) {
    t.startedAt = Date.now();
    t.timeoutId = setTimeout(() => {
      removeToast(id);
    }, t.remaining);
  }
}

export function removeToast(id: string) {
  const t = toastTimers.get(id);
  if (t) {
    clearTimeout(t.timeoutId);
    toastTimers.delete(id);
  }
  toasts.update((list) => list.filter((item) => item.id !== id));
}

// UI Overlays state
// The approved prototype starts with the repository dock collapsed on a fresh
// profile, but a saved preference always wins (RELAY-UI · UI-03).
const SIDEBAR_COLLAPSED_KEY = "vantage_sidebar_collapsed";
const LEGACY_SIDEBAR_COLLAPSED_KEY = "relay_sidebar_collapsed";

function loadSidebarCollapsed(): boolean {
  try {
    const saved = localStorage.getItem(SIDEBAR_COLLAPSED_KEY) ?? localStorage.getItem(LEGACY_SIDEBAR_COLLAPSED_KEY);
    if (saved === "true") return true;
    if (saved === "false") return false;
  } catch {
    // Fall through to the prototype default when storage is unavailable.
  }
  return true;
}

export const sidebarCollapsed = writable<boolean>(loadSidebarCollapsed());

if (typeof window !== "undefined") {
  sidebarCollapsed.subscribe((collapsed) => {
    try {
      localStorage.setItem(SIDEBAR_COLLAPSED_KEY, collapsed ? "true" : "false");
    } catch {
      // Persisting the preference is best-effort.
    }
  });
}

export const paletteOpen = writable<boolean>(false);
export const settingsOpen = writable<boolean>(false);
export const shortcutsHelpOpen = writable<boolean>(false);
export const addRepoDialogOpen = writable<boolean>(false);
export const scanFolderDialogOpen = writable<boolean>(false);
// Which tab the Add-repository dialog should open on. Consumed once when the
// dialog opens so the palette can deep-link straight to the clone form.
export const addRepoInitialTab = writable<"existing" | "clone">("existing");

// Which Settings section to reveal when the dialog opens, and which repo the
// Recovery pane should pre-select. Reset on close so a plain "open settings"
// always lands on General.
export type SettingsSection =
  | "general"
  | "git"
  | "terminal"
  | "integrations"
  | "recovery"
  | "about";

export const settingsSection = writable<SettingsSection>("general");
export const settingsRecoveryRepoId = writable<string | null>(null);

export function openSettingsAt(section: SettingsSection, repoId?: string) {
  settingsSection.set(section);
  settingsRecoveryRepoId.set(repoId ?? null);
  settingsOpen.set(true);
}

/**
 * Toast extra that links a destructive-action confirmation to the Recovery
 * pane (Settings → Recovery), so the user can see the backup is restorable.
 */
export function recoveryToastLink(
  repoId: string
): Pick<ToastMessage, "linkLabel" | "linkAction"> {
  return {
    linkLabel: "Recovery",
    linkAction: () => openSettingsAt("recovery", repoId),
  };
}

// Active modal dialogs
export interface ActiveDialog {
  type:
    | "revert"
    | "reset"
    | "discard"
    | "error_details"
    | "rename"
    | "change_icon"
    | "remove_repo"
    | "branch_create"
    | "branch_rename"
    | "branch_delete";
  repoId: string;
  data?: any;
}

export const activeDialog = writable<ActiveDialog | null>(null);

// Bumped after any local branch create/rename/delete so branch lists (header
// switcher, pane header) refetch without prop-drilling.
export const branchesVersion = writable(0);

export function bumpBranches() {
  branchesVersion.update((n) => n + 1);
}

export function openDialog(dialog: ActiveDialog) {
  activeDialog.set(dialog);
}

export function closeDialog() {
  activeDialog.set(null);
}
