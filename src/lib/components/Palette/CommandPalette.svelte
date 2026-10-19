<script lang="ts">
  import { get } from "svelte/store";
  import { paletteOpen, addToast } from "../../stores/ui";
  import { repos, activeRepoId } from "../../stores/repos";
  import {
    openRepoInFocusedPane,
    openRepoInNextPane,
    panes,
    focusedPaneIndex,
    setPaneTab,
  } from "../../stores/panes";
  import { formatKey } from "../../keys";
  import { paletteCommands, runCommand, getBaseContexts, applyNamedLayout } from "../../commands";
  import { fuzzyScore } from "../../fuzzy";
  import { formatRelativeTime } from "../../format";
  import type { CommitSummary } from "../../types";
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

    // Commands come from the single shortcut registry so labels and
    // keybindings cannot drift between the palette, Help and the router.
    const commands: Omit<PaletteItem, "type" | "score">[] = paletteCommands(getBaseContexts()).map((c) => ({
      id: c.id,
      title: c.label,
      shortcut: c.keys.length > 0 ? c.keys.map((k) => formatKey(k)).join(" / ") : undefined,
      action: () => {
        $paletteOpen = false;
        runCommand(c.id);
      },
    }));

    // Add named layouts if present. These are user data, not registry
    // commands; they still route through the layout command's applier.
    try {
      const raw = localStorage.getItem("vantage_saved_layouts") || localStorage.getItem("relay_saved_layouts") || "{}";
      const saved = JSON.parse(raw);
      for (const name of Object.keys(saved)) {
        commands.push({
          id: `load_layout_${name}`,
          title: `Load layout: ${name}`,
          action: () => {
            $paletteOpen = false;
            applyNamedLayout(name, saved[name]);
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
