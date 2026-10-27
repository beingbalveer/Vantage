import { writable } from "svelte/store";
import type { ToastMessage } from "../util/toast";

export {
  toasts,
  addToast,
  pauseToastTimer,
  resumeToastTimer,
  removeToast,
  type ToastMessage,
} from "../util/toast";

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
export const stashDialogOpen = writable<boolean>(false);
/**
 * Centered file-search overlay. Carries the owning pane so only that pane's
 * Files tab renders the dialog when several panes show the explorer.
 */
export interface FileSearchRequest {
  paneIndex: number;
  repoId: string;
}
export const fileSearchOpen = writable<FileSearchRequest | null>(null);

// Dirty-tree branch switching: a single prompt that offers leave / bring / stash.
export interface CheckoutRequest {
  repoId: string;
  repoName: string;
  target: string;
  dirtyCount: number;
}

export const checkoutRequest = writable<CheckoutRequest | null>(null);

export function requestCheckout(request: CheckoutRequest) {
  checkoutRequest.set(request);
}

export function clearCheckoutRequest() {
  checkoutRequest.set(null);
}

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
  | "about"
  | "flags";

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
    | "branch_delete"
    | "layout";
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

// Tag dialogs (create + view/push)
export interface TagDialogRequest {
  repoId: string;
  /** Commit SHA or ref the tag will point at (e.g. a full SHA or "HEAD"). */
  target: string;
  /** Human-friendly label for the target (short SHA or "HEAD"). */
  targetLabel: string;
}

export const tagCreateRequest = writable<TagDialogRequest | null>(null);

export function openTagCreateDialog(request: TagDialogRequest) {
  tagCreateRequest.set(request);
}

export function closeTagCreateDialog() {
  tagCreateRequest.set(null);
}

export interface TagDetailsRequest {
  repoId: string;
  name: string;
  annotated: boolean;
  message: string | null;
  date: string | null;
  target: string;
}

export const tagDetailsRequest = writable<TagDetailsRequest | null>(null);

export function openTagDetailsDialog(request: TagDetailsRequest) {
  tagDetailsRequest.set(request);
}

export function closeTagDetailsDialog() {
  tagDetailsRequest.set(null);
}
