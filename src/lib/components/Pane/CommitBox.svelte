<script lang="ts">
  import { isFeatureEnabled } from "../../config/featureFlags.svelte";
  import { settings } from "../../domain/stores/settings";
  import * as api from "../../platform/api";
  import { addToast } from "../../app/ui";
  import { pendingCommitMessage } from "../../domain/stores/composer";
  import { registerHandler } from "../../app/commands";
  import type { HeadCommitInfo } from "../../platform/types";
  import { canAmendLastCommit, amendWarning } from "../../domain/commitActions";
  import { runUndoLastCommit } from "../../domain/undoCommit";
  import Icon from "../common/Icon.svelte";
  import {
    addCoAuthor,
    coAuthorEmail,
    formatCoAuthor,
    parseCoAuthor,
    type CoAuthor,
  } from "../../domain/coauthors";

  interface Props {
    repoId: string;
    branch: string;
    selectedPaths: string[];
    /** Paths with unmerged index entries; commit is blocked until resolved (ALPH-156). */
    conflictedPaths?: string[];
    /** Bumped by the palette to open this composer already in amend mode. */
    amendToken?: number;
    oncommit?: (preserveSelection?: boolean) => void;
  }

  let {
    repoId,
    branch,
    selectedPaths,
    conflictedPaths = [],
    amendToken = 0,
    oncommit,
  }: Props = $props();

  let summary = $state("");
  let description = $state("");
  let showDescription = $state(false);
  let isCommitting = $state(false);
  let isDrafting = $state(false);

  let coAuthors = $state<CoAuthor[]>([]);
  let showCoAuthorPicker = $state(false);
  let coAuthorInput = $state("");
  let suggestions = $state<CoAuthor[]>([]);
  let loadingSuggestions = $state(false);

  // Amend / undo state (ALPH-154)
  let amendMode = $state(false);
  let headInfo = $state<HeadCommitInfo | null>(null);
  let isLoadingHead = $state(false);
  let isUndoing = $state(false);
  // Starts at 0 (the token's initial value) so the effect below can detect the
  // first real request without reading the prop during initialization.
  let seenAmendToken = $state(0);

  let hasAiCmd = $derived(!!$settings.commitMessageCmd);
  let summaryLen = $derived(summary.length);
  let hasConflicts = $derived(conflictedPaths.length > 0);
  let canCommit = $derived(
    summary.trim().length > 0 &&
      !isCommitting &&
      !hasConflicts &&
      (amendMode || selectedPaths.length > 0)
  );
  let pushedWarning = $derived(headInfo ? amendWarning(headInfo.isPushed) : null);

  // Pick up a message preserved by an undo (from the palette or composer).
  $effect(() => {
    const pending = $pendingCommitMessage;
    if (pending && pending.repoId === repoId) {
      summary = pending.summary;
      description = pending.body;
      showDescription = !!pending.body;
      pendingCommitMessage.set(null);
    }
  });

  // The palette bumps amendToken to request amend mode.
  $effect(() => {
    if (amendToken !== seenAmendToken) {
      seenAmendToken = amendToken;
      if (amendToken > 0) enterAmendMode();
    }
  });

  async function enterAmendMode() {
    if (isCommitting || isLoadingHead) return;
    isLoadingHead = true;
    try {
      const info = await api.headCommitInfo(repoId);
      const guard = canAmendLastCommit(info, info.operationInProgress);
      if (!guard.ok) {
        addToast({ message: guard.reason, kind: "error" });
        return;
      }
      headInfo = info;
      amendMode = true;
      summary = info.subject;
      description = info.body || "";
      showDescription = !!info.body;
    } catch (e: any) {
      addToast({ message: e.message || "Cannot amend last commit", kind: "error" });
    } finally {
      isLoadingHead = false;
    }
  }

  function exitAmendMode() {
    amendMode = false;
    headInfo = null;
    summary = "";
    description = "";
    showDescription = false;
  }

  async function handleDraftMessage() {
    if (!hasAiCmd || isDrafting || selectedPaths.length === 0) return;
    isDrafting = true;
    try {
      const res = await api.runCommitMessageCmd(repoId, selectedPaths);
      summary = res.summary;
      if (res.body) {
        description = res.body;
        showDescription = true;
      }
    } catch (e: any) {
      addToast({ message: e.message || "Failed to draft commit message", kind: "error" });
    } finally {
      isDrafting = false;
    }
  }

  async function toggleCoAuthorPicker() {
    showCoAuthorPicker = !showCoAuthorPicker;
    if (showCoAuthorPicker && suggestions.length === 0) {
      loadingSuggestions = true;
      try {
        suggestions = await api.recentContributors(repoId, 50);
      } catch {
        // Best-effort: the picker still allows typing a co-author by hand.
        suggestions = [];
      } finally {
        loadingSuggestions = false;
      }
    }
  }

  function tryAddCoAuthor(raw: string): boolean {
    const parsed = parseCoAuthor(raw);
    if (!parsed.ok) {
      addToast({ message: parsed.error, kind: "error" });
      return false;
    }
    const key = coAuthorEmail(parsed.coAuthor);
    if (coAuthors.some((c) => coAuthorEmail(c) === key)) {
      addToast({ message: `${parsed.coAuthor.name} is already a co-author`, kind: "info" });
      return false;
    }
    coAuthors = addCoAuthor(coAuthors, parsed.coAuthor);
    return true;
  }

  function addTypedCoAuthor() {
    if (tryAddCoAuthor(coAuthorInput)) {
      coAuthorInput = "";
    }
  }

  function addSuggestedCoAuthor(contributor: CoAuthor) {
    if (tryAddCoAuthor(formatCoAuthor(contributor))) {
      showCoAuthorPicker = false;
    }
  }

  function removeCoAuthorAt(index: number) {
    coAuthors = coAuthors.filter((_, i) => i !== index);
  }

  async function handleCommit() {
    if (!canCommit || hasConflicts) return;
    isCommitting = true;
    const body = showDescription && description.trim() ? description.trim() : undefined;
    try {
      const coAuthorTrailers = coAuthors.length > 0 ? coAuthors.map(formatCoAuthor) : undefined;
      if (amendMode) {
        const c = await api.amendCommit(repoId, selectedPaths, summary.trim(), body);
        addToast({ message: `Amended ${c.short}`, kind: "success" });
      } else {
        const c = await api.commit(repoId, selectedPaths, summary.trim(), body, coAuthorTrailers);
        addToast({ message: `Committed ${c.short}`, kind: "success" });
      }
      summary = "";
      description = "";
      showDescription = false;
      coAuthors = [];
      coAuthorInput = "";
      showCoAuthorPicker = false;
      amendMode = false;
      headInfo = null;
      // Refresh the pane exactly once after a successful commit.
      oncommit?.();
    } catch (e: any) {
      addToast({ message: e.message || (amendMode ? "Amend failed" : "Commit failed"), kind: "error" });
    } finally {
      isCommitting = false;
    }
  }

  async function handleUndo() {
    if (isUndoing || isCommitting) return;
    isUndoing = true;
    try {
      await runUndoLastCommit(repoId);
      exitAmendMode();
      // Preserve selection so the composer stays open with the restored message.
      oncommit?.(true);
    } catch (e: any) {
      addToast({ message: e.message || "Failed to undo last commit", kind: "error" });
    } finally {
      isUndoing = false;
    }
  }

  $effect(() => {
    const unregister = registerHandler(
      "commit",
      () => {
        const active = document.activeElement as HTMLElement | null;
        if (!active?.closest(".commit-box")) return false;
        if (!canCommit) return false;
        void handleCommit();
        return true;
      },
      "history"
    );
    return unregister;
  });
</script>

<div class="commit-box">
  <!-- Amend affordance: always visible; the previous commit is echoed so the
       rewrite is never accidental. -->
  {#if amendMode && headInfo}
    <div class="amend-banner">
      <div class="amend-body">
        <Icon name="pencil" size={12} />
        <span class="amend-label">
          Amending <span class="font-mono">{headInfo.short}</span>
          <span class="amend-subject" title={headInfo.subject}>{headInfo.subject}</span>
        </span>
      </div>
      <button
        type="button"
        class="amend-exit"
        aria-label="Cancel amend"
        title="Cancel amend"
        onclick={exitAmendMode}
        disabled={isCommitting}
      >
        <Icon name="close" size={12} />
      </button>
    </div>
    {#if pushedWarning}
      <div class="amend-warning" role="alert">
        <span class="warning-icon">⚠</span>
        <span>{pushedWarning}</span>
      </div>
    {/if}
  {:else}
    <button
      type="button"
      class="amend-toggle"
      onclick={enterAmendMode}
      disabled={isLoadingHead || isCommitting}
      title="Rewrite the last commit instead of creating a new one"
    >
      <Icon name="pencil" size={12} />
      <span>{isLoadingHead ? "Loading last commit…" : "Amend last commit"}</span>
    </button>
  {/if}

  <!-- Summary Input with optional AI draft button and counter -->
  <div class="input-wrap">
    <input
      type="text"
      placeholder="Summary (required)"
      aria-label="Commit summary"
      bind:value={summary}
      class="summary-input"
    />
    {#if hasAiCmd}
      <button
        type="button"
        class="draft-btn"
        onclick={handleDraftMessage}
        disabled={isDrafting || selectedPaths.length === 0}
        title="Draft message with AI"
        aria-label="Draft commit message with AI"
      >
        <Icon name="sparkles" size={13} />
      </button>
    {/if}
  </div>

  {#if summaryLen >= 60}
    <div class="counter text-xs {summaryLen >= 73 ? 'counter-amber' : ''}">
      {summaryLen} / 72
    </div>
  {/if}

  <!-- Collapsible Description -->
  <button
    type="button"
    class="desc-toggle"
    aria-expanded={showDescription}
    onclick={() => (showDescription = !showDescription)}
  >
    {showDescription ? "▾" : "▸"} Description
  </button>

  {#if showDescription}
    <textarea
      placeholder="Add an optional extended description…"
      aria-label="Commit extended description"
      bind:value={description}
      rows="4"
      class="desc-input"
    ></textarea>
  {/if}

  <!-- Co-author picker (ALPH-155) -->
  <div class="coauthor-section">
    <button
      type="button"
      class="coauthor-toggle"
      aria-expanded={showCoAuthorPicker}
      onclick={toggleCoAuthorPicker}
    >
      <Icon name="plus" size={11} />
      <span>Co-authors{coAuthors.length > 0 ? ` (${coAuthors.length})` : ""}</span>
    </button>

    {#if coAuthors.length > 0}
      <ul class="coauthor-chips" aria-label="Selected co-authors">
        {#each coAuthors as co, i (coAuthorEmail(co))}
          <li class="coauthor-chip">
            <span class="coauthor-chip-name" title={formatCoAuthor(co)}>{co.name}</span>
            <button
              type="button"
              class="coauthor-chip-remove"
              aria-label={`Remove co-author ${co.name}`}
              onclick={() => removeCoAuthorAt(i)}
            >
              <Icon name="close" size={10} />
            </button>
          </li>
        {/each}
      </ul>
    {/if}

    {#if showCoAuthorPicker}
      <div class="coauthor-picker">
        <div class="coauthor-input-wrap">
          <input
            type="text"
            class="coauthor-input"
            placeholder="Name <email>"
            aria-label="Add co-author"
            bind:value={coAuthorInput}
            onkeydown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                addTypedCoAuthor();
              }
            }}
          />
          <button
            type="button"
            class="coauthor-add-btn"
            disabled={!coAuthorInput.trim()}
            onclick={addTypedCoAuthor}
          >
            Add
          </button>
        </div>

        {#if loadingSuggestions}
          <div class="coauthor-empty text-xs">Loading recent contributors…</div>
        {:else if suggestions.length > 0}
          <ul class="coauthor-suggestions" aria-label="Recent contributors">
            {#each suggestions as s (coAuthorEmail(s))}
              {@const alreadyAdded = coAuthors.some(
                (c) => coAuthorEmail(c) === coAuthorEmail(s)
              )}
              <li>
                <button
                  type="button"
                  class="coauthor-suggestion"
                  disabled={alreadyAdded}
                  onclick={() => addSuggestedCoAuthor(s)}
                >
                  <span class="coauthor-suggestion-name">{s.name}</span>
                  <span class="coauthor-suggestion-email text-xs">{s.email}</span>
                </button>
              </li>
            {/each}
          </ul>
        {:else}
          <div class="coauthor-empty text-xs">No recent contributors. Type one above.</div>
        {/if}
      </div>
    {/if}
  </div>

  <!-- Branch display above commit button (R28) -->
  <div class="commit-target-branch text-xs font-mono text-muted">
    <Icon name="git-branch" size={11} class="mr-1 inline-block" />
    <span>{branch || "main"}</span>
  </div>

  {#if hasConflicts}
    <div class="commit-conflict-warning" role="alert">
      <span class="text-strong">Resolve conflicts before committing.</span>
      <ul>
        {#each conflictedPaths as path (path)}
          <li class="font-mono">{path}</li>
        {/each}
      </ul>
    </div>
  {/if}

  <!-- Commit Button with concise label (R28) -->
  <button
    type="button"
    class="commit-btn"
    disabled={!canCommit}
    onclick={handleCommit}
  >
    {#if isCommitting}
      {amendMode ? "Amending…" : "Committing…"}
    {:else if amendMode}
      {selectedPaths.length > 0
        ? `Amend commit (${selectedPaths.length} ${selectedPaths.length === 1 ? "file" : "files"})`
        : "Amend commit message"}
    {:else}
      Commit {selectedPaths.length} {selectedPaths.length === 1 ? "file" : "files"}
    {/if}
  </button>

  <!-- Undo last commit: soft reset, changes come back staged. -->
  <button
    type="button"
    class="undo-btn"
    onclick={handleUndo}
    disabled={isUndoing || isCommitting}
    title="Move HEAD back one commit, keeping the changes staged"
  >
    <Icon name="undo" size={12} />
    <span>{isUndoing ? "Undoing…" : "Undo last commit"}</span>
  </button>
</div>

<style>
  .commit-box {
    padding: 12px;
    background-color: var(--bg-muted);
    border-top: none;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .amend-toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    align-self: flex-start;
    background: none;
    border: none;
    color: var(--text-muted);
    font-size: 12px;
    cursor: pointer;
    padding: 2px 0;
  }
  .amend-toggle:hover:not(:disabled) {
    color: var(--text);
  }
  .amend-toggle:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .amend-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 6px 8px;
    border-radius: var(--r-md);
    background-color: var(--accent-bg, rgba(64, 120, 200, 0.12));
    color: var(--text);
  }
  .amend-body {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    color: var(--text-muted);
  }
  .amend-label {
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .amend-subject {
    color: var(--text);
    margin-left: 6px;
  }
  .amend-exit {
    display: flex;
    align-items: center;
    background: none;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    padding: 2px;
    flex-shrink: 0;
  }
  .amend-exit:hover:not(:disabled) {
    color: var(--text);
  }

  .amend-warning {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    padding: 8px 10px;
    border-radius: var(--r-md);
    border: 1px solid var(--danger);
    background-color: var(--danger-subtle, rgba(218, 54, 51, 0.1));
    color: var(--text);
    font-size: 12px;
    line-height: 16px;
  }
  .warning-icon {
    color: var(--danger);
    line-height: 16px;
  }

  .input-wrap {
    display: flex;
    align-items: center;
    position: relative;
  }

  .summary-input {
    width: 100%;
    height: 28px;
    padding: 0 8px;
    border: none;
    border-radius: var(--r-md);
    background-color: var(--bg);
    color: var(--text);
    font-size: 13px;
    outline: none;
  }
  .summary-input:focus {
    box-shadow: 0 0 0 1.5px var(--accent);
  }

  .draft-btn {
    position: absolute;
    right: 4px;
    background: none;
    border: none;
    font-size: 14px;
    cursor: pointer;
    padding: 2px 4px;
  }
  .draft-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .counter {
    text-align: right;
    color: var(--text-faint);
  }
  .counter-amber {
    color: var(--warning);
    font-weight: 600;
  }

  .desc-toggle {
    background: none;
    border: none;
    color: var(--text-muted);
    font-size: 12px;
    text-align: left;
    cursor: pointer;
    padding: 2px 0;
  }
  .desc-toggle:hover {
    color: var(--text);
  }

  .desc-input {
    width: 100%;
    padding: 6px 8px;
    border: none;
    border-radius: var(--r-md);
    background-color: var(--bg);
    color: var(--text);
    font-size: 12px;
    font-family: var(--font-ui);
    resize: vertical;
    outline: none;
  }
  .desc-input:focus {
    box-shadow: 0 0 0 1.5px var(--accent);
  }

  .coauthor-section {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .coauthor-toggle {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    align-self: flex-start;
    background: none;
    border: none;
    color: var(--text-muted);
    font-size: 12px;
    cursor: pointer;
    padding: 2px 0;
  }
  .coauthor-toggle:hover {
    color: var(--text);
  }

  .coauthor-chips {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .coauthor-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    max-width: 100%;
    padding: 2px 4px 2px 8px;
    background-color: var(--bg-muted);
    border: 1px solid var(--border);
    border-radius: var(--r-sm, 4px);
    font-size: 12px;
    color: var(--text);
  }

  .coauthor-chip-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .coauthor-chip-remove {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    padding: 0;
    background: none;
    border: none;
    color: var(--text-faint);
    cursor: pointer;
    border-radius: var(--r-sm, 4px);
  }
  .coauthor-chip-remove:hover {
    color: var(--danger);
    background-color: var(--bg);
  }

  .coauthor-picker {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px;
    background-color: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
  }

  .coauthor-input-wrap {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .coauthor-input {
    flex: 1;
    min-width: 0;
    height: 26px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    background-color: var(--bg);
    color: var(--text);
    font-size: 12px;
    outline: none;
  }
  .coauthor-input:focus {
    border-color: var(--accent);
    box-shadow: 0 0 0 1.5px var(--accent);
  }

  .coauthor-add-btn {
    height: 26px;
    padding: 0 10px;
    border: none;
    border-radius: var(--r-md);
    background-color: var(--accent);
    color: var(--accent-fg);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }
  .coauthor-add-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .coauthor-suggestions {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 160px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }

  .coauthor-suggestion {
    display: flex;
    align-items: baseline;
    gap: 8px;
    width: 100%;
    padding: 4px 6px;
    background: none;
    border: none;
    border-radius: var(--r-sm, 4px);
    color: var(--text);
    font-size: 12px;
    text-align: left;
    cursor: pointer;
  }
  .coauthor-suggestion:hover:not(:disabled) {
    background-color: var(--bg-muted);
  }
  .coauthor-suggestion:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .coauthor-suggestion-name {
    flex-shrink: 0;
  }

  .coauthor-suggestion-email {
    color: var(--text-faint);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .coauthor-empty {
    color: var(--text-faint);
    padding: 2px 0;
  }

  .commit-conflict-warning {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 8px 10px;
    background-color: var(--danger-subtle, rgba(218, 54, 51, 0.1));
    border: 1px solid var(--danger);
    border-radius: var(--r-md);
    color: var(--danger);
    font-size: 12px;
  }
  .commit-conflict-warning ul {
    margin: 0;
    padding-left: 16px;
    max-height: 76px;
    overflow-y: auto;
    font-size: 11.5px;

  }

  .commit-btn {
    height: 32px;
    width: 100%;
    border-radius: var(--r-md);
    border: none;
    background-color: var(--accent);
    color: var(--accent-fg);
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    transition: filter 80ms linear;
  }
  .commit-btn:hover:not(:disabled) {
    filter: brightness(0.95);
  }
  .commit-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .undo-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    width: 100%;
    height: 26px;
    border-radius: var(--r-md);
    border: 1px solid var(--border);
    background-color: transparent;
    color: var(--text-muted);
    font-size: 12px;
    cursor: pointer;
  }
  .undo-btn:hover:not(:disabled) {
    color: var(--text);
    background-color: var(--bg-subtle);
  }
  .undo-btn:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
</style>
