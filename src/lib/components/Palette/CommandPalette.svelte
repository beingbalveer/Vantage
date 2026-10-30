<script lang="ts">
  import { get } from "svelte/store";
  import {
    paletteOpen,
    addToast,
    addRepoDialogOpen,
    settingsOpen,
    shortcutsHelpOpen,
    openDialog,
    openTagCreateDialog,
    addRepoInitialTab,
    openSettingsAt,
    stashDialogOpen,
  } from "../../app/ui";
  import { composerRequest } from "../../domain/stores/composer";
  import { repos, activeRepoId, loadRepos, refreshAllStatuses, refreshSingleStatus } from "../../domain/stores/repos";
  import {
    openRepoInFocusedPane,
    openRepoInNextPane,
    panes,
    focusedPaneIndex,
    focusPane,
    setPaneTab,
    paneCount,
    closePane,
    cyclePaneCount,
    toggleTerminalInPane,
  } from "../../domain/stores/panes";
  import { settings, updateSettings } from "../../domain/stores/settings";
  import { navigateTo } from "../../domain/stores/navigation";
  import { formatKey, SHORTCUTS } from "../../platform/keys";
  import { paletteCommands, runCommand, getBaseContexts, loadLayoutByName, readSavedLayouts } from "../../app/commands";
  import { isFeatureEnabled } from "../../config/featureFlags.svelte";
  import { fuzzyScore } from "../../util/fuzzy";
  import { formatRelativeTime } from "../../domain/format";
  import type { CommitSummary } from "../../platform/types";
  import * as api from "../../platform/api";
  import { runUndoLastCommit } from "../../domain/undoCommit";
  import Icon from "../common/Icon.svelte";

  let query = $state("");
  let selectedIndex = $state(0);
  let inputEl = $state<HTMLInputElement | null>(null);

  let mode = $derived.by(() => {
    if (query.startsWith(">")) return "commands";
    if (query.startsWith("/")) return "files";
    if (query.startsWith("#")) return "commits";
    return "all";
  });

  let rawQuery = $derived(
    mode === "all" ? query.trim() : query.slice(1).trim()
  );

  let focusedRepoId = $derived.by(() => {
    const currentPanes = $panes;
    const fIdx = $focusedPaneIndex;
    const p = currentPanes[fIdx];
    return p?.repoId || $activeRepoId || ($repos.length > 0 ? $repos[0].id : null);
  });

  let focusedRepo = $derived.by(() => {
    const id = focusedRepoId;
    return $repos.find((r) => r.id === id) || null;
  });

  interface PaletteItem {
    id: string;
    type: "repo" | "command" | "file" | "commit";
    title: string;
    subtitle?: string;
    shortcut?: string;
    action: (mod?: boolean) => void;
    score: number;
  }

  // Caches for file tree and commits
  const treeCache = new Map<string, string[]>();
  let repoFiles = $state<string[]>([]);
  let repoCommits = $state<CommitSummary[]>([]);

  $effect(() => {
    if (!$paletteOpen) return;

    if (mode === "files" && focusedRepoId) {
      if (treeCache.has(focusedRepoId)) {
        repoFiles = treeCache.get(focusedRepoId)!;
      } else {
        api.treeList(focusedRepoId, "HEAD").then((files) => {
          treeCache.set(focusedRepoId!, files);
          repoFiles = files;
        }).catch(() => {
          repoFiles = [];
        });
      }
    } else if (mode === "commits" && focusedRepoId) {
      api.logPage(focusedRepoId, 0, 50, rawQuery || undefined).then((commits) => {
        repoCommits = commits;
      }).catch(() => {
        repoCommits = [];
      });
    }
  });

  /** Jump the focused pane to a file result instead of only switching tabs. */
  function openFileResult(file: string) {
    if (focusedRepoId) {
      navigateTo({ repoId: focusedRepoId, tab: "files", path: file });
      return;
    }
    setPaneTab(get(focusedPaneIndex), "files");
  }

  /** Jump the focused pane to a commit result. */
  function openCommitResult(commit: CommitSummary) {
    if (focusedRepoId) {
      navigateTo({ repoId: focusedRepoId, tab: "history", commitSha: commit.sha });
      return;
    }
    setPaneTab(get(focusedPaneIndex), "history");
  }

  function splitPath(fullPath: string): { dir: string; name: string } {
    const idx = fullPath.lastIndexOf("/");
    if (idx === -1) {
      return { dir: "", name: fullPath };
    }
    return {
      dir: fullPath.slice(0, idx + 1),
      name: fullPath.slice(idx + 1),
    };
  }

  let items = $derived.by(() => {
    const list: PaletteItem[] = [];

    // Commands come from the single shortcut registry so labels and
    // keybindings cannot drift between the palette, Help and the router.
    const registryCommands: Omit<PaletteItem, "type" | "score">[] = paletteCommands(getBaseContexts()).map((c) => ({
      id: c.id,
      title: c.label,
      shortcut: c.keys.length > 0 ? c.keys.map((k) => formatKey(k)).join(" / ") : undefined,
      action: () => {
        $paletteOpen = false;
        runCommand(c.id);
      },
    }));

    const extraCommands: Omit<PaletteItem, "type" | "score">[] = [
      {
        id: "cmd_clone_repo",
        title: "Clone repository…",
        action: () => {
          $paletteOpen = false;
          addRepoInitialTab.set("clone");
          $addRepoDialogOpen = true;
        },
      },
      {
        id: "cmd_browse_repo",
        title: "Browse and add repository…",
        action: async () => {
          $paletteOpen = false;
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
        },
      },
      {
        id: "cmd_amend_last_commit",
        title: "Amend last commit…",
        action: () => {
          $paletteOpen = false;
          if (!focusedRepoId) {
            addToast({ message: "No repository open in the focused pane", kind: "info" });
            return;
          }
          composerRequest.set({ repoId: focusedRepoId, action: "amend" });
        },
      },
      {
        id: "cmd_undo_last_commit",
        title: "Undo last commit",
        action: async () => {
          $paletteOpen = false;
          if (!focusedRepoId) {
            addToast({ message: "No repository open in the focused pane", kind: "info" });
            return;
          }
          try {
            await runUndoLastCommit(focusedRepoId);
          } catch (e: any) {
            addToast({ message: e.message || "Failed to undo last commit", kind: "error" });
          }
        },
      },
      {
        id: "cmd_create_tag",
        title: "Create tag…",
        action: () => {
          $paletteOpen = false;
          if (!focusedRepoId) {
            addToast({ message: "No focused repository to tag", kind: "info" });
            return;
          }
          const branch = focusedRepo?.branch;
          openTagCreateDialog({
            repoId: focusedRepoId,
            target: "HEAD",
            targetLabel: branch ? `HEAD (${branch})` : "HEAD",
          });
        },
      },
      {
        id: "cmd_push_tags",
        title: "Push tags to origin",
        action: async () => {
          $paletteOpen = false;
          if (!focusedRepoId) {
            addToast({ message: "No focused repository to push tags to", kind: "info" });
            return;
          }
          try {
            await api.tagPushAll(focusedRepoId);
            addToast({ message: "Pushed tags to origin", kind: "success" });
          } catch (e: any) {
            addToast({ message: e.message || "Failed to push tags", kind: "error" });
          }
        },
      },
      {
        id: "cmd_switch_tab",
        title: "Switch tab (History / Files)",
        shortcut: "t",
        action: () => {
          $paletteOpen = false;
          const fIdx = get(focusedPaneIndex);
          const currentPanes = get(panes);
          const currentTab = currentPanes[fIdx]?.tab || "history";
          setPaneTab(fIdx, currentTab === "history" ? "files" : "history");
        },
      },
      {
        id: "cmd_toggle_terminal",
        title: "Toggle terminal drawer",
        shortcut: "Mod+`",
        action: () => {
          $paletteOpen = false;
          const fIdx = get(focusedPaneIndex);
          toggleTerminalInPane(fIdx);
        },
      },
      {
        id: "cmd_toggle_whitespace",
        title: "Toggle ignore whitespace",
        shortcut: "w",
        action: () => {
          $paletteOpen = false;
          const curr = get(settings);
          const next = !curr.ignoreWhitespace;
          updateSettings({ ...curr, ignoreWhitespace: next });
          addToast({ message: `Ignore whitespace: ${next ? "on" : "off"}`, kind: "info" });
        },
      },
      {
        id: "cmd_sync",
        title: "Fetch / Pull / Push (Sync)",
        shortcut: "Mod+Shift+F",
        action: async () => {
          $paletteOpen = false;
          if (!focusedRepoId) {
            addToast({ message: "No active repository to sync", kind: "info" });
            return;
          }
          try {
            addToast({ message: "Fetching origin...", kind: "info" });
            await api.syncFetch(focusedRepoId);
            await refreshSingleStatus(focusedRepoId);
            addToast({ message: "Fetch complete", kind: "success" });
          } catch (e: any) {
            addToast({ message: e.message || "Sync failed", kind: "error" });
          }
        },
      },
      {
        id: "cmd_open_editor",
        title: "Open in external editor",
        action: async () => {
          $paletteOpen = false;
          if (!focusedRepoId) {
            addToast({ message: "No active repository selected", kind: "info" });
            return;
          }
          try {
            await api.openExternalEditor(focusedRepoId, "");
          } catch (e: any) {
            addToast({ message: e.message || "Failed to open in editor", kind: "error" });
          }
        },
      },
      {
        id: "cmd_reveal_file_manager",
        title: "Reveal repository in file manager",
        action: async () => {
          $paletteOpen = false;
          if (!focusedRepo?.path) {
            addToast({ message: "No active repository selected", kind: "info" });
            return;
          }
          try {
            await api.revealInFileManager(focusedRepo.path);
          } catch (e: any) {
            addToast({ message: e.message || "Failed to reveal in file manager", kind: "error" });
          }
        },
      },
      {
        id: "cmd_copy_path",
        title: "Copy repository path",
        shortcut: "c",
        action: async () => {
          $paletteOpen = false;
          if (!focusedRepo?.path) {
            addToast({ message: "No active repository selected", kind: "info" });
            return;
          }
          try {
            await navigator.clipboard.writeText(focusedRepo.path);
            addToast({ message: `Copied path: ${focusedRepo.path}`, kind: "info" });
          } catch (e: any) {
            addToast({ message: e.message || "Failed to copy path", kind: "error" });
          }
        },
      },
      {
        id: "cmd_split_pane",
        title: "Split pane / Cycle panes",
        shortcut: "Mod+\\",
        action: () => {
          $paletteOpen = false;
          cyclePaneCount();
        },
      },
      {
        id: "cmd_close_pane",
        title: "Close focused pane",
        shortcut: "Mod+W",
        action: () => {
          $paletteOpen = false;
          const count = get(paneCount);
          if (count <= 1) {
            addToast({ message: "Cannot close the only open pane", kind: "info" });
            return;
          }
          const fIdx = get(focusedPaneIndex);
          closePane(fIdx);
          addToast({ message: `Closed pane ${fIdx + 1}`, kind: "info" });
        },
      },
      {
        id: "cmd_focus_pane_1",
        title: "Focus pane 1",
        shortcut: "Mod+1",
        action: () => {
          $paletteOpen = false;
          focusPane(0);
        },
      },
      {
        id: "cmd_focus_pane_2",
        title: "Focus pane 2",
        shortcut: "Mod+2",
        action: () => {
          $paletteOpen = false;
          focusPane(1);
        },
      },
      {
        id: "cmd_focus_pane_3",
        title: "Focus pane 3",
        shortcut: "Mod+3",
        action: () => {
          $paletteOpen = false;
          focusPane(2);
        },
      },
      {
        id: "cmd_refresh_all",
        title: "Refresh all repositories",
        shortcut: "Mod+Shift+R",
        action: async () => {
          $paletteOpen = false;
          await refreshAllStatuses();
          addToast({ message: "Refreshed all repositories", kind: "info" });
        },
      },
      {
        id: "cmd_settings",
        title: "Open settings",
        shortcut: "Mod+,",
        action: () => {
          $paletteOpen = false;
          openSettingsAt("general");
        },
      },
      {
        id: "cmd_recovery",
        title: "Recovery (Backups)",
        action: () => {
          $paletteOpen = false;
          openSettingsAt("recovery");
        },
      },
      {
        id: "cmd_stashes",
        title: "Stashes…",
        action: () => {
          $paletteOpen = false;
          $stashDialogOpen = true;
        },
      },
      {
        id: "cmd_shortcuts",
        title: "Keyboard shortcuts help",
        shortcut: "?",
        action: () => {
          $paletteOpen = false;
          $shortcutsHelpOpen = true;
        },
      },
      {
        id: "cmd_new_branch",
        title: "New branch…",
        action: () => {
          $paletteOpen = false;
          if (!focusedRepoId) {
            addToast({ message: "No repository selected to create a branch in", kind: "info" });
            return;
          }
          openDialog({
            type: "branch_create",
            repoId: focusedRepoId,
            data: { startPoint: "HEAD" },
          });
        },
      },
      {
        id: "cmd_rename_branch",
        title: "Rename current branch…",
        action: () => {
          $paletteOpen = false;
          if (!focusedRepo) {
            addToast({ message: "No repository selected", kind: "info" });
            return;
          }
          openDialog({
            type: "branch_rename",
            repoId: focusedRepo.id,
            data: { from: focusedRepo.branch || undefined },
          });
        },
      },
      {
        id: "cmd_remove_repo",
        title: "Remove current repository from Vantage…",
        action: () => {
          $paletteOpen = false;
          const currentPanes = get(panes);
          const fIdx = get(focusedPaneIndex);
          const focusedPane = currentPanes[fIdx];
          if (!focusedPane?.repoId) {
            addToast({ message: "No active repository selected to remove", kind: "info" });
            return;
          }
          const allRepos = get(repos);
          const currentRepo = allRepos.find((r) => r.id === focusedPane.repoId);
          if (!currentRepo) return;

          openDialog({ type: "remove_repo", repoId: currentRepo.id, data: { repo: currentRepo } });
        },
      },
    ];

    // The registry is the single source of truth for commands. The extras
    // below predate it; drop the ones whose behaviour now lives in a registry
    // handler so users don't see the same action twice under different ids.
    const DUPLICATE_EXTRA_IDS = new Set([
      "cmd_browse_repo",
      "cmd_switch_tab",
      "cmd_toggle_terminal",
      "cmd_toggle_whitespace",
      "cmd_sync",
      "cmd_open_editor",
      "cmd_reveal_file_manager",
      "cmd_copy_path",
      "cmd_save_layout",
      "cmd_load_layout",
      "cmd_split_pane",
      "cmd_close_pane",
      "cmd_focus_pane_1",
      "cmd_focus_pane_2",
      "cmd_focus_pane_3",
      "cmd_refresh_all",
      "cmd_settings",
      "cmd_recovery",
      "cmd_shortcuts",
      "cmd_remove_repo",
    ]);

    const commands: Omit<PaletteItem, "type" | "score">[] = [
      ...registryCommands,
      ...extraCommands.filter((command) => !DUPLICATE_EXTRA_IDS.has(command.id)),
    ];

    // Add named layouts if present. These are user data, not registry
    // commands; they still route through the layout command's applier.
    try {
      const saved = readSavedLayouts();
      for (const name of Object.keys(saved)) {
        commands.push({
          id: `load_layout_${name}`,
          title: `Load layout: ${name}`,
          action: () => {
            $paletteOpen = false;
            void loadLayoutByName(name);
          },
        });
      }
    } catch {}

    // 1. Files Mode (/)
    if (mode === "files") {
      if (repoFiles.length > 0) {
        for (const file of repoFiles) {
          if (!rawQuery) {
            const { dir, name } = splitPath(file);
            list.push({
              id: `file_${file}`,
              type: "file",
              title: name,
              subtitle: dir || undefined,
              score: 1,
              action: () => {
                $paletteOpen = false;
                openFileResult(file);
                addToast({ message: `Opened ${name} in Files tab`, kind: "info" });
              },
            });
          } else {
            const match = fuzzyScore(rawQuery, file);
            if (match) {
              const { dir, name } = splitPath(file);
              list.push({
                id: `file_${file}`,
                type: "file",
                title: name,
                subtitle: dir || undefined,
                score: match.score,
                action: () => {
                  $paletteOpen = false;
                  openFileResult(file);
                  addToast({ message: `Opened ${name} in Files tab`, kind: "info" });
                },
              });
            }
          }
        }
      }
    }

    // 2. Commits Mode (#)
    else if (mode === "commits") {
      if (repoCommits.length > 0) {
        for (const commit of repoCommits) {
          if (!rawQuery) {
            list.push({
              id: `commit_${commit.sha}`,
              type: "commit",
              title: commit.subject,
              subtitle: `${commit.short} · ${commit.authorName} · ${formatRelativeTime(commit.date)}`,
              score: 1,
              action: () => {
                $paletteOpen = false;
                openCommitResult(commit);
                addToast({ message: `Commit ${commit.short}: ${commit.subject}`, kind: "info" });
              },
            });
          } else {
            const matchSubj = fuzzyScore(rawQuery, commit.subject);
            const matchAuth = fuzzyScore(rawQuery, commit.authorName);
            const matchSha = fuzzyScore(rawQuery, commit.short);
            const score = Math.max(matchSubj?.score || 0, matchAuth?.score || 0, matchSha?.score || 0);
            if (score > 0) {
              list.push({
                id: `commit_${commit.sha}`,
                type: "commit",
                title: commit.subject,
                subtitle: `${commit.short} · ${commit.authorName} · ${formatRelativeTime(commit.date)}`,
                score,
                action: () => {
                  $paletteOpen = false;
                  openCommitResult(commit);
                  addToast({ message: `Commit ${commit.short}: ${commit.subject}`, kind: "info" });
                },
              });
            }
          }
        }
      }
    }

    // 3. Commands Only Mode (>)
    else if (mode === "commands") {
      for (const cmd of commands) {
        if (!rawQuery) {
          list.push({ ...cmd, type: "command", score: 0 });
        } else {
          const match = fuzzyScore(rawQuery, cmd.title);
          if (match) {
            list.push({ ...cmd, type: "command", score: match.score });
          }
        }
      }
    }

    // 5. Default All Mode (repositories first, then commands)
    else {
      // Repositories first
      for (const r of $repos) {
        if (!rawQuery) {
          list.push({
            id: `repo_${r.id}`,
            type: "repo",
            title: r.name,
            subtitle: r.path,
            score: 100, // Higher score so repos appear first
            action: (mod) => {
              $paletteOpen = false;
              if (mod) {
                openRepoInNextPane(r.id);
              } else {
                openRepoInFocusedPane(r.id);
              }
            },
          });
        } else {
          const match = fuzzyScore(rawQuery, r.name) || fuzzyScore(rawQuery, r.path);
          if (match) {
            list.push({
              id: `repo_${r.id}`,
              type: "repo",
              title: r.name,
              subtitle: r.path,
              score: match.score + 50, // Bonus so repos appear before generic command matches
              action: (mod) => {
                $paletteOpen = false;
                if (mod) {
                  openRepoInNextPane(r.id);
                } else {
                  openRepoInFocusedPane(r.id);
                }
              },
            });
          }
        }
      }

      // Commands second
      for (const cmd of commands) {
        if (!rawQuery) {
          list.push({ ...cmd, type: "command", score: 10 });
        } else {
          const match = fuzzyScore(rawQuery, cmd.title);
          if (match) {
            list.push({ ...cmd, type: "command", score: match.score });
          }
        }
      }
    }

    list.sort((a, b) => b.score - a.score);
    return list.slice(0, 10);
  });

  $effect(() => {
    if ($paletteOpen) {
      const opener =
        document.activeElement instanceof HTMLElement &&
        document.activeElement !== document.body
          ? document.activeElement
          : null;
      query = "";
      selectedIndex = 0;
      const timer = setTimeout(() => inputEl?.focus(), 50);
      return () => {
        clearTimeout(timer);
        if (opener?.isConnected && typeof opener.focus === "function") {
          opener.focus();
        }
      };
    }
  });

  function handleKeydown(e: KeyboardEvent) {
    const mod = e.metaKey || e.ctrlKey;
    if (
      (mod && e.key.toLowerCase() === "k") ||
      (e.ctrlKey && e.shiftKey && e.key.toLowerCase() === "p")
    ) {
      // The registry also carries this binding, but the palette input is an
      // editable target and must not be intercepted by the global router.
      e.preventDefault();
      $paletteOpen = false;
    } else if (e.key === "Escape") {
      $paletteOpen = false;
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      selectedIndex = (selectedIndex + 1) % Math.max(1, items.length);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      selectedIndex = (selectedIndex - 1 + items.length) % Math.max(1, items.length);
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (items[selectedIndex]) {
        items[selectedIndex].action(e.metaKey || e.ctrlKey);
      }
    }
  }
</script>

{#if $paletteOpen}
  <div
    class="palette-backdrop"
    role="presentation"
    onclick={() => ($paletteOpen = false)}
    onkeydown={(e) => {
      if (e.key === "Escape") $paletteOpen = false;
    }}
  >
    <div
      class="palette-modal"
      role="dialog"
      aria-modal="true"
      aria-label="Command palette"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={handleKeydown}
    >
      <!-- Search Input -->
      <div class="input-row">
        <span class="search-icon">
          {#if mode === 'commands'}
            <Icon name="zap" size={15} />
          {:else if mode === 'files'}
            <Icon name="file-text" size={15} />
          {:else if mode === 'commits'}
            <Icon name="commit" size={15} />
          {:else}
            <Icon name="search" size={15} />
          {/if}
        </span>
        <input
          bind:this={inputEl}
          bind:value={query}
          type="text"
          role="combobox"
          aria-label="Search repositories, commands, files, and commits"
          aria-autocomplete="list"
          aria-controls="palette-results"
          aria-expanded="true"
          aria-activedescendant={items[selectedIndex] ? `palette-item-${selectedIndex}` : undefined}
          placeholder={mode === 'commands'
            ? 'Type a command...'
            : mode === 'files'
            ? 'Search files in focused repo...'
            : mode === 'commits'
            ? 'Search commits in focused repo...'
            : 'Search repositories, commands (>), files (/), commits (#)...'}
          class="palette-input"
        />
        {#if !query}
          <div class="hints">
            <button type="button" class="hint-chip" onclick={() => (query = ">")}>&gt; commands</button>
            <button type="button" class="hint-chip" onclick={() => (query = "/")}>/ files</button>
            <button type="button" class="hint-chip" onclick={() => (query = "#")}># commits</button>
          </div>
        {:else if mode !== 'all'}
          <span class="mode-badge">{mode}</span>
        {/if}
      </div>

      <!-- Results list -->
      <div class="results-list" id="palette-results" role="listbox">
        {#if items.length === 0}
          <div class="empty-results text-sm text-muted">
            {#if (mode === 'files' || mode === 'commits') && !focusedRepoId}
              No repository open in focused pane
            {:else}
              No matches found
            {/if}
          </div>
        {:else}
          {#each items as item, idx (item.id)}
            <div
              class="result-row {idx === selectedIndex ? 'selected' : ''}"
              id={`palette-item-${idx}`}
              onclick={() => item.action(false)}
              onkeydown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  item.action(e.metaKey || e.ctrlKey);
                }
              }}
              role="option"
              aria-selected={idx === selectedIndex}
              tabindex="0"
            >
              <span class="row-icon">
                {#if item.type === "repo"}
                  <Icon name="folder" size={14} />
                {:else if item.type === "file"}
                  <Icon name="file-text" size={14} />
                {:else if item.type === "commit"}
                  <Icon name="commit" size={14} />
                {:else}
                  <Icon name="zap" size={14} />
                {/if}
              </span>
              <div class="row-text">
                <span class="row-title text-strong">{item.title}</span>
                {#if item.subtitle}
                  <span class="row-subtitle text-xs">{item.subtitle}</span>
                {/if}
              </div>

              {#if item.shortcut}
                <span class="row-shortcut font-mono text-xs">
                  {item.shortcut}
                </span>
              {/if}

              {#if idx === selectedIndex && item.type === "repo"}
                <span class="row-hint text-xs">Mod+Enter: new pane</span>
              {/if}
            </div>
          {/each}
        {/if}
      </div>

      <!-- Footer -->
      <div class="palette-footer text-xs">
        <span>↑↓ navigate</span>
        <span>·</span>
        <span>↵ select</span>
        <span>·</span>
        <span>⌘↵ new pane</span>
        <span>·</span>
        <span>esc close</span>
      </div>
    </div>
  </div>
{/if}

<style>
  .palette-backdrop {
    position: fixed;
    inset: 0;
    background-color: rgba(0, 0, 0, 0.35);
    z-index: 200;
    display: flex;
    justify-content: center;
    padding-top: 14vh;
  }

  .palette-modal {
    width: 560px;
    max-width: 92vw;
    height: fit-content;
    background-color: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--r-lg);
    box-shadow: var(--shadow-pop);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .input-row {
    height: 48px;
    padding: 0 14px;
    display: flex;
    align-items: center;
    gap: 10px;
    border-bottom: 1px solid var(--border);
  }

  .search-icon {
    font-size: 14px;
    opacity: 0.7;
    display: flex;
    align-items: center;
  }

  .palette-input {
    flex: 1;
    height: 100%;
    border: none;
    outline: none;
    font-size: 15px;
    background: transparent;
    color: var(--text);
    font-family: var(--font-ui);
  }

  .hints {
    display: flex;
    gap: 6px;
  }

  .hint-chip {
    font-size: 11px;
    padding: 2px 6px;
    border: none;
    border-radius: var(--r-sm);
    background-color: var(--bg-muted);
    color: var(--text-muted);
    cursor: pointer;
  }
  .hint-chip:hover {
    background-color: var(--bg-subtle);
    color: var(--text);
  }

  .mode-badge {
    font-size: 11px;
    font-weight: 600;
    padding: 2px 8px;
    border-radius: var(--r-sm);
    background-color: var(--accent-bg);
    color: var(--accent);
    text-transform: uppercase;
  }

  .results-list {
    max-height: 380px;
    overflow-y: auto;
    padding: 6px 0;
  }

  .empty-results {
    padding: 24px;
    text-align: center;
  }

  .result-row {
    height: 38px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 14px;
    cursor: pointer;
  }
  .result-row:hover,
  .result-row.selected {
    background-color: var(--bg-muted);
  }

  .row-icon {
    font-size: 14px;
    width: 18px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .row-text {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: baseline;
    gap: 8px;
  }

  .row-title {
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row-subtitle {
    color: var(--text-faint);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row-shortcut {
    color: var(--text-muted);
    padding: 2px 6px;
    background-color: var(--bg-subtle);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
  }

  .row-hint {
    color: var(--accent);
    font-weight: 500;
  }

  .palette-footer {
    height: 28px;
    background-color: var(--bg-subtle);
    border-top: 1px solid var(--border-subtle);
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    color: var(--text-faint);
    user-select: none;
  }
</style>
