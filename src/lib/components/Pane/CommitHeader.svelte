<script lang="ts">
  import type { CommitSummary, TagInfo } from "../../types";
  import { formatRelativeTime, formatFullTimestamp } from "../../format";
  import { addToast, openTagCreateDialog } from "../../stores/ui";
  import Icon from "../common/Icon.svelte";
  import TagChip from "./TagChip.svelte";

  interface Props {
    commit: CommitSummary;
    repoId?: string;
    tags?: TagInfo[];
  }

  let { commit, repoId = "", tags = [] }: Props = $props();

  let isExpanded = $state(false);
  let copied = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | null = null;

  function handleCreateTag(e: MouseEvent) {
    e.stopPropagation();
    if (!repoId) return;
    openTagCreateDialog({
      repoId,
      target: commit.sha,
      targetLabel: commit.short || commit.sha.slice(0, 7),
    });
  }

  async function handleCopySha(e: MouseEvent) {
    e.stopPropagation();
    try {
      await navigator.clipboard.writeText(commit.sha);
      copied = true;
      if (copyTimer) clearTimeout(copyTimer);
      copyTimer = setTimeout(() => {
        copied = false;
      }, 1500);
      addToast({ message: `Copied ${commit.short} to clipboard`, kind: "info" });
    } catch {
      addToast({ message: "Failed to copy commit SHA", kind: "error" });
    }
  }
</script>

<div class="commit-header-bar">
  <!-- Subject + Stats + Expand details toggle -->
  <div class="subject-row">
    <span class="commit-subject" title={commit.subject}>{commit.subject}</span>

    <div class="header-right">
      <div class="stats font-mono">
        {#if commit.additions > 0 || commit.deletions === 0}
          <span class="stat-add">+{commit.additions}</span>
        {/if}
        {#if commit.deletions > 0}
          <span class="stat-del">−{commit.deletions}</span>
        {/if}
      </div>

      {#if repoId}
        <button
          type="button"
          class="create-tag-btn"
          title="Create tag at this commit…"
          onclick={handleCreateTag}
        >
          <Icon name="tag" size={12} strokeWidth={2} />
          <span>Tag</span>
        </button>
      {/if}

      <button
        type="button"
        class="expand-toggle-btn {isExpanded ? 'active' : ''}"
        aria-expanded={isExpanded}
        title={isExpanded ? "Hide commit details" : "Show commit details"}
        onclick={() => (isExpanded = !isExpanded)}
        aria-label={isExpanded ? "Hide commit details" : "Show commit details"}
      >
        <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor">
          <path d="M8 2.5l2.8 2.8H5.2z" />
          <circle cx="6" cy="8" r="0.9" />
          <circle cx="10" cy="8" r="0.9" />
          <path d="M8 13.5l-2.8-2.8h5.6z" />
        </svg>
      </button>
    </div>
  </div>

  {#if tags.length > 0}
    <div class="tag-strip">
      {#each tags as tag (tag.name)}
        <TagChip {tag} {repoId} />
      {/each}
    </div>
  {/if}

  <!-- Expandable Details: Full commit message body & metadata -->
  {#if isExpanded}
    <div class="expanded-pane">
      {#if commit.body && commit.body.trim().length > 0}
        <div class="commit-body-text">{commit.body.trim()}</div>
      {/if}
      <div class="commit-extra-meta">
        <span>Authored by <strong>{commit.authorName}</strong>{#if commit.authorEmail} &lt;{commit.authorEmail}&gt;{/if}</span>
        <span class="sep">·</span>
        <span title={formatFullTimestamp(commit.date)}>{formatRelativeTime(commit.date)} ({formatFullTimestamp(commit.date)})</span>
        <span class="sep">·</span>
        <button
          type="button"
          class="copy-sha-btn font-mono text-xs"
          onclick={handleCopySha}
          title="Click to copy full commit SHA"
        >
          SHA: {commit.sha}
          {#if copied}
            <span class="copied-badge">copied</span>
          {/if}
        </button>
      </div>
    </div>
  {/if}
</div>

<style>
  .commit-header-bar {
    width: 100%;
    background-color: var(--bg);
    border-bottom: 1px solid var(--border-subtle);
    padding: 7px 14px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    user-select: text;
    flex-shrink: 0;
    box-sizing: border-box;
  }

  .subject-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    min-width: 0;
  }

  .commit-subject {
    font-size: 13px;
    font-weight: 400;
    line-height: 1.35;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    letter-spacing: normal;
    flex: 1;
    min-width: 0;
  }

  .header-right {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-shrink: 0;
  }

  .create-tag-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 20px;
    padding: 0 7px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-md);
    background-color: transparent;
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    flex-shrink: 0;
  }
  .create-tag-btn:hover {
    background-color: var(--accent-bg);
    color: var(--accent);
    border-color: var(--accent);
  }

  .tag-strip {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding-top: 2px;
  }

  .stats {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
    font-weight: 500;
    flex-shrink: 0;
  }

  .stat-add {
    color: var(--success, #2da44e);
  }

  .stat-del {
    color: var(--danger, #cf222e);
  }

  .expand-toggle-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border: none;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    border-radius: var(--r-sm);
    padding: 0;
    transition: all 100ms ease;
    flex-shrink: 0;
  }
  .expand-toggle-btn:hover {
    background-color: var(--bg-hover, rgba(0, 0, 0, 0.06));
    color: var(--text);
  }
  .expand-toggle-btn.active {
    color: var(--accent);
    background-color: var(--bg-subtle);
  }

  .expanded-pane {
    margin-top: 6px;
    padding: 8px 10px;
    background-color: var(--bg-subtle);
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-sm);
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-height: 160px;
    overflow-y: auto;
  }

  .commit-body-text {
    font-size: 12px;
    line-height: 1.45;
    color: var(--text);
    white-space: pre-wrap;
    word-break: break-word;
  }

  .commit-extra-meta {
    font-size: 11px;
    color: var(--text-muted);
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    border-top: 1px solid var(--border-subtle);
    padding-top: 4px;
  }

  .copy-sha-btn {
    background: transparent;
    border: 1px solid var(--border-subtle);
    border-radius: 3px;
    padding: 1px 5px;
    cursor: pointer;
    color: var(--text-muted);
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .copy-sha-btn:hover {
    background-color: var(--bg-muted);
    color: var(--text);
  }

  .copied-badge {
    color: var(--success, #2da44e);
    font-weight: 600;
  }

  .sep {
    opacity: 0.5;
  }
</style>
