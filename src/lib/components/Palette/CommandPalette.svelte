<script lang="ts">
  import { get } from "svelte/store";
  import { paletteOpen } from "../../stores/ui";
  import { repos, loadRepos, refreshSingleStatus, refreshAllStatuses, activeRepoId, removeRepo } from "../../stores/repos";
  import {
    openRepoInFocusedPane,
    openRepoInNextPane,
    cyclePaneCount,
    setPaneCount,
    paneCount,
    closePane,
    panes,
    focusedPaneIndex,
    setPaneTab,
    toggleTerminalInPane,
  } from "../../stores/panes";
  import { settings, updateSettings } from "../../stores/settings";
  import { SHORTCUTS, formatKey } from "../../keys";
  import { fuzzyScore } from "../../fuzzy";
  import { formatRelativeTime } from "../../format";
  import {
    addRepoDialogOpen,
    settingsOpen,
    shortcutsHelpOpen,
    addToast,
    sidebarCollapsed,
    openDialog,
    addRepoInitialTab,
    openSettingsAt,
  } from "../../stores/ui";
  import type { Layout, CommitSummary } from "../../types";
  import * as api from "../../api";
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

    // Commands definition
    const commands: Omit<PaletteItem, "type" | "score">[] = [
      {
        id: "cmd_add_repo",
        title: "Add repository…",
        shortcut: "Mod+O",
        action: () => {
          $paletteOpen = false;
          $addRepoDialogOpen = true;
        },
      },
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
        id: "cmd_toggle_sidebar",
        title: "Toggle sidebar",
        shortcut: "Mod+B",
        action: () => {
          $paletteOpen = false;
          $sidebarCollapsed = !$sidebarCollapsed;
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
        id: "cmd_save_layout",
        title: "Save layout as…",
        action: async () => {
          $paletteOpen = false;
          const name = prompt("Enter a name for this layout (e.g. 'frontend + backend'):");
          if (!name || !name.trim()) return;
          try {
            const count = get(paneCount);
            const currentPanes = get(panes);
            const layoutData: Layout = {
              paneCount: count,
              panes: currentPanes.slice(0, count).map((p) => ({
                repoId: p.repoId,
                tab: p.tab,
                widths: null,
              })),
              sidebarWidth: 232,
              sidebarCollapsed: get(sidebarCollapsed),
              terminalHeight: 240,
              terminalOpen: currentPanes.some((p) => p.terminalOpen),
              windowBounds: null,
            };
            await api.layoutSet(layoutData);

            try {
              const raw = localStorage.getItem("vantage_saved_layouts") || localStorage.getItem("relay_saved_layouts") || "{}";
              const saved = JSON.parse(raw);
              saved[name.trim()] = {
                name: name.trim(),
                paneCount: count,
                panes: currentPanes.slice(0, count).map((p) => ({
                  repoId: p.repoId,
                  tab: p.tab,
                })),
                savedAt: new Date().toISOString(),
              };
              localStorage.setItem("vantage_saved_layouts", JSON.stringify(saved));
            } catch {}

            addToast({ message: `Layout "${name.trim()}" saved`, kind: "success" });
          } catch (e: any) {
            addToast({ message: e.message || "Failed to save layout", kind: "error" });
          }
        },
      },
      {
        id: "cmd_load_layout",
        title: "Load layout…",
        action: async () => {
          $paletteOpen = false;
          try {
            const raw = localStorage.getItem("vantage_saved_layouts") || localStorage.getItem("relay_saved_layouts") || "{}";
            const saved = JSON.parse(raw);
            const names = Object.keys(saved);

            let chosenLayout: any = null;
            if (names.length > 0) {
              const listPrompt = names.map((n, i) => `${i + 1}. ${n}`).join("\n");
              const chosen = prompt(
                `Select layout to load:\n${listPrompt}\n\nEnter name or number (or leave empty to load default persisted layout):`
              );
              if (chosen === null) return;
              if (chosen.trim()) {
                const num = parseInt(chosen.trim(), 10);
                const name = !isNaN(num) && names[num - 1] ? names[num - 1] : chosen.trim();
                if (saved[name]) {
                  chosenLayout = saved[name];
                }
              }
            }

            if (!chosenLayout) {
              chosenLayout = await api.layoutGet();
            }

            if (chosenLayout) {
              const targetCount = Math.max(1, Math.min(3, chosenLayout.paneCount || 1));
              setPaneCount(targetCount);
              panes.update((list) => {
                return list.map((p, i) => {
                  if (i < targetCount && chosenLayout.panes?.[i]) {
                    return {
                      ...p,
                      repoId: chosenLayout.panes[i].repoId,
                      tab: chosenLayout.panes[i].tab || "history",
                    };
                  }
                  return p;
                });
              });
              addToast({
                message: `Loaded layout (${targetCount} ${targetCount === 1 ? "pane" : "panes"})`,
                kind: "success",
              });
            }
          } catch (e: any) {
            addToast({ message: e.message || "Failed to load layout", kind: "error" });
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
          focusedPaneIndex.set(0);
        },
      },
      {
        id: "cmd_focus_pane_2",
        title: "Focus pane 2",
        shortcut: "Mod+2",
        action: () => {
          $paletteOpen = false;
          if (get(paneCount) < 2) setPaneCount(2);
          focusedPaneIndex.set(1);
        },
      },
      {
        id: "cmd_focus_pane_3",
        title: "Focus pane 3",
        shortcut: "Mod+3",
        action: () => {
          $paletteOpen = false;
          if (get(paneCount) < 3) setPaneCount(3);
          focusedPaneIndex.set(2);
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
        id: "cmd_shortcuts",
        title: "Keyboard shortcuts help",
        shortcut: "?",
        action: () => {
          $paletteOpen = false;
          $shortcutsHelpOpen = true;
        },
      },
      {
        id: "cmd_remove_repo",
        title: "Remove current repository from Relay…",
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

    // Add named layouts if present
    try {
      const raw = localStorage.getItem("vantage_saved_layouts") || localStorage.getItem("relay_saved_layouts") || "{}";
      const saved = JSON.parse(raw);
      for (const name of Object.keys(saved)) {
        commands.push({
          id: `cmd_load_layout_${name}`,
          title: `Load layout: ${name}`,
          action: () => {
            $paletteOpen = false;
            const targetLayout = saved[name];
            if (targetLayout) {
              const targetCount = Math.max(1, Math.min(3, targetLayout.paneCount || 1));
              setPaneCount(targetCount);
              panes.update((list) => {
                return list.map((p, i) => {
                  if (i < targetCount && targetLayout.panes?.[i]) {
                    return {
                      ...p,
                      repoId: targetLayout.panes[i].repoId,
                      tab: targetLayout.panes[i].tab || "history",
                    };
                  }
                  return p;
                });
              });
              addToast({ message: `Loaded layout "${name}"`, kind: "success" });
            }
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
                const fIdx = get(focusedPaneIndex);
                setPaneTab(fIdx, "files");
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
                  const fIdx = get(focusedPaneIndex);
                  setPaneTab(fIdx, "files");
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
                const fIdx = get(focusedPaneIndex);
                setPaneTab(fIdx, "history");
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
                  const fIdx = get(focusedPaneIndex);
                  setPaneTab(fIdx, "history");
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
      query = "";
      selectedIndex = 0;
      setTimeout(() => inputEl?.focus(), 50);
    }
  });

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
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
      <div class="results-list" role="listbox">
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
              onclick={() => item.action(false)}
              onkeydown={(e) => {
                if (e.key === 'Enter') item.action(e.metaKey || e.ctrlKey);
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
                  {formatKey(item.shortcut)}
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
