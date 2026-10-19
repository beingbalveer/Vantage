/**
 * Frontend dispatcher for native menu commands.
 *
 * `src-tauri/src/menu.rs` emits `menu://command` with the ids below. Each
 * branch calls the same stores/API used by `CommandPalette.svelte` and the
 * global shortcuts in `keys.ts`, so a menu item cannot drift from the in-app
 * behaviour. Keep the ids in sync with `MenuAction::id()`.
 */
import { get } from "svelte/store";
import type { RepoEntry } from "./types";
import * as api from "./api";
import {
  activeRepoId,
  refreshSingleStatus,
  repos,
  statuses,
} from "./stores/repos";
import {
  closePane,
  cyclePaneCount,
  focusedPaneIndex,
  paneCount,
  panes,
  setPaneCount,
  toggleTerminalInPane,
} from "./stores/panes";
import {
  addRepoDialogOpen,
  addToast,
  paletteOpen,
  settingsOpen,
  shortcutsHelpOpen,
  sidebarCollapsed,
} from "./stores/ui";

/** Event listened to by `TopHeader.svelte` to reuse its branch menu. */
export const OPEN_BRANCH_MENU_EVENT = "vantage:open-branch-menu";

function focusedRepoId(): string | null {
  const pane = get(panes)[get(focusedPaneIndex)];
  return pane?.repoId ?? get(activeRepoId) ?? get(repos)[0]?.id ?? null;
}

function focusedRepo(): RepoEntry | null {
  const id = focusedRepoId();
  return id ? get(repos).find((repo) => repo.id === id) ?? null : null;
}

async function runRemote(operation: "fetch" | "pull" | "push") {
  const repo = focusedRepo();
  if (!repo) {
    addToast({ message: "No repository selected", kind: "info" });
    return;
  }

  const status = get(statuses)[repo.id];
  if (operation !== "fetch" && status?.isCheckedOut === false) {
    addToast({
      message: "Checkout this branch before pulling or pushing.",
      kind: "error",
    });
    return;
  }

  try {
    if (operation === "fetch") {
      await api.syncFetch(repo.id);
      addToast({ message: `Fetched ${repo.name}`, kind: "success" });
    } else if (operation === "pull") {
      await api.syncPull(repo.id);
      addToast({ message: `Pulled ${repo.name}`, kind: "success" });
    } else {
      const hasUpstream = !!status?.upstream;
      await api.syncPush(repo.id, !hasUpstream);
      addToast({
        message: hasUpstream
          ? `Pushed ${repo.name}`
          : `Published ${repo.branch || "branch"}`,
        kind: "success",
      });
    }
    await refreshSingleStatus(repo.id);
  } catch (error: any) {
    addToast({
      message: error?.message || `${operation} failed`,
      kind: "error",
    });
  }
}

export async function dispatchMenuCommand(command: string): Promise<void> {
  // Menu actions behave like palette actions: dismiss the palette first.
  paletteOpen.set(false);

  switch (command) {
    case "settings":
      settingsOpen.set(true);
      return;

    case "repo-add-existing":
      addRepoDialogOpen.set(true);
      return;

    case "repo-open-editor": {
      const repo = focusedRepo();
      if (!repo) {
        addToast({ message: "No repository selected", kind: "info" });
        return;
      }
      try {
        await api.openExternalEditor(repo.id, "");
      } catch (error: any) {
        addToast({
          message: error?.message || "Failed to open in editor",
          kind: "error",
        });
      }
      return;
    }

    case "repo-show-folder": {
      const repo = focusedRepo();
      if (!repo) {
        addToast({ message: "No repository selected", kind: "info" });
        return;
      }
      try {
        await api.revealInFileManager(repo.path);
      } catch (error: any) {
        addToast({
          message: error?.message || "Failed to reveal in file manager",
          kind: "error",
        });
      }
      return;
    }

    case "branch-checkout": {
      if (!focusedRepo()) {
        addToast({ message: "No repository selected", kind: "info" });
        return;
      }
      window.dispatchEvent(new CustomEvent(OPEN_BRANCH_MENU_EVENT));
      return;
    }

    case "remote-fetch":
      await runRemote("fetch");
      return;

    case "remote-pull":
      await runRemote("pull");
      return;

    case "remote-push":
      await runRemote("push");
      return;

    case "view-split-pane":
      cyclePaneCount();
      return;

    case "view-close-pane": {
      if (get(paneCount) <= 1) {
        addToast({ message: "Cannot close the only open pane", kind: "info" });
        return;
      }
      const index = get(focusedPaneIndex);
      closePane(index);
      addToast({ message: `Closed pane ${index + 1}`, kind: "info" });
      return;
    }

    case "view-focus-pane-1":
    case "view-focus-pane-2":
    case "view-focus-pane-3": {
      const index = Number(command.slice(-1)) - 1;
      if (get(paneCount) < index + 1) setPaneCount(index + 1);
      focusedPaneIndex.set(index);
      return;
    }

    case "view-toggle-terminal":
      toggleTerminalInPane(get(focusedPaneIndex));
      return;

    case "view-toggle-sidebar":
      sidebarCollapsed.update((collapsed) => !collapsed);
      return;

    case "help-shortcuts":
      shortcutsHelpOpen.set(true);
      return;

    default:
      console.warn(`Unhandled native menu command: ${command}`);
  }
}
