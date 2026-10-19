<script lang="ts">
  import { settings } from "../../stores/settings";
  import * as api from "../../api";
  import { addToast } from "../../stores/ui";
  import Icon from "../common/Icon.svelte";
  import {
    addCoAuthor,
    coAuthorEmail,
    formatCoAuthor,
    parseCoAuthor,
    type CoAuthor,
  } from "../../coauthors";

  interface Props {
    repoId: string;
    branch: string;
    selectedPaths: string[];
    oncommit?: () => void;
  }

  let { repoId, branch, selectedPaths, oncommit }: Props = $props();

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

  let hasAiCmd = $derived(!!$settings.commitMessageCmd);
  let summaryLen = $derived(summary.length);
  let canCommit = $derived(
    summary.trim().length > 0 && selectedPaths.length > 0 && !isCommitting
  );

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
    if (!canCommit) return;
    isCommitting = true;
    try {
      const c = await api.commit(
        repoId,
        selectedPaths,
        summary.trim(),
        showDescription && description.trim() ? description.trim() : undefined,
        coAuthors.length > 0 ? coAuthors.map(formatCoAuthor) : undefined
      );
      addToast({ message: `Committed ${c.short}`, kind: "success" });
      summary = "";
      description = "";
      showDescription = false;
      coAuthors = [];
      coAuthorInput = "";
      showCoAuthorPicker = false;
      oncommit?.();
    } catch (e: any) {
      addToast({ message: e.message || "Commit failed", kind: "error" });
    } finally {
      isCommitting = false;
    }
  }
</script>

<div class="commit-box">
  <!-- Summary Input with optional AI draft button and counter -->
  <div class="input-wrap">
    <input
      type="text"
      placeholder="Summary (required)"
      aria-label="Commit summary"
      bind:value={summary}
      class="summary-input"
      onkeydown={(e) => {
        if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
          handleCommit();
        }
      }}
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

  <!-- Commit Button with concise label (R28) -->
  <button
    type="button"
    class="commit-btn"
    disabled={!canCommit}
    onclick={handleCommit}
  >
    {#if isCommitting}
      Committing…
    {:else}
      Commit {selectedPaths.length} {selectedPaths.length === 1 ? "file" : "files"}
    {/if}
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
    user-select: none;
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
</style>
