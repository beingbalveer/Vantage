<script lang="ts">
  import { settings } from "../../stores/settings";
  import * as api from "../../api";
  import { addToast } from "../../stores/ui";
  import { registerHandler } from "../../commands";
  import Icon from "../common/Icon.svelte";

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

  async function handleCommit() {
    if (!canCommit) return;
    isCommitting = true;
    try {
      const c = await api.commit(
        repoId,
        selectedPaths,
        summary.trim(),
        showDescription && description.trim() ? description.trim() : undefined
      );
      addToast({ message: `Committed ${c.short}`, kind: "success" });
      summary = "";
      description = "";
      showDescription = false;
      oncommit?.();
    } catch (e: any) {
      addToast({ message: e.message || "Commit failed", kind: "error" });
    } finally {
      isCommitting = false;
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
