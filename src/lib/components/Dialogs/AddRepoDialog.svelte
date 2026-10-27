<script lang="ts">
  import { get } from "svelte/store";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { addRepoDialogOpen, addRepoInitialTab, addToast } from "../../app/ui";
  import { loadRepos, activeRepoId } from "../../domain/stores/repos";
  import { openRepoInFocusedPane } from "../../domain/stores/panes";
  import { saveRepoIcon, type RepoIcon } from "../../domain/stores/repoIcons";
  import { isSafeFolderName, suggestFolderName } from "../../domain/clone";
  import * as api from "../../platform/api";
  import { Modal, Icon } from "../common";
  import RepoIconPicker from "./RepoIconPicker.svelte";

  let tab = $state<"existing" | "clone">("existing");

  // ---- Add existing ----
  let path = $state("");
  let branch = $state("");
  let choice = $state<RepoIcon>({ kind: "app", value: "code" });
  let isAdding = $state(false);

  // ---- Clone ----
  let cloneUrl = $state("");
  let cloneDest = $state("");
  let cloneName = $state("");
  let cloneBranch = $state("");
  let cloneChoice = $state<RepoIcon>({ kind: "app", value: "code" });
  let cloneNameTouched = $state(false);
  let isCloning = $state(false);
  let cloneProgress = $state("");
  let cloneError = $state<string | null>(null);

  let isBrowsing = $state(false);

  let isBusy = $derived(isAdding || isBrowsing || isCloning);
  let canSubmit = $derived(!!path.trim() && !isBusy);

  let cloneSuggestedName = $derived(suggestFolderName(cloneUrl) ?? "");
  let cloneNameError = $derived.by(() => {
    const name = cloneName.trim();
    if (!name) return null;
    return isSafeFolderName(name) ? null : "Folder name can't contain path separators.";
  });
  let cloneHasName = $derived(
    cloneName.trim() ? true : cloneSuggestedName.length > 0
  );
  let canClone = $derived(
    !!cloneUrl.trim() && !!cloneDest.trim() && cloneHasName && !cloneNameError && !isBusy
  );
  let resolvedDest = $derived.by(() => {
    const dest = cloneDest.trim().replace(/[\\/]+$/, "");
    const name = cloneName.trim() || cloneSuggestedName;
    if (!dest || !name) return "";
    return `${dest}/${name}`;
  });

  let effectiveExistingName = $derived.by(() => {
    const p = path.trim().replace(/[\\/]+$/, "");
    if (!p) return "R";
    const segments = p.split(/[\\/]/);
    return segments[segments.length - 1] || "R";
  });

  let effectiveCloneName = $derived.by(() => {
    return cloneName.trim() || cloneSuggestedName || "R";
  });

  // Auto-fill the folder name from the URL until the user edits it.
  $effect(() => {
    if (!cloneNameTouched) {
      cloneName = suggestFolderName(cloneUrl) ?? "";
    }
  });

  // When the dialog opens, honor a deep-link from the palette and clear state.
  $effect(() => {
    if ($addRepoDialogOpen) {
      tab = get(addRepoInitialTab);
      addRepoInitialTab.set("existing");
      cloneError = null;
      cloneProgress = "";
    }
  });

  function close() {
    if (isBusy) return;
    $addRepoDialogOpen = false;
  }

  function reset() {
    path = "";
    branch = "";
    choice = { kind: "app", value: "code" };
  }

  function resetClone() {
    cloneUrl = "";
    cloneDest = "";
    cloneName = "";
    cloneBranch = "";
    cloneChoice = { kind: "app", value: "code" };
    cloneNameTouched = false;
    cloneProgress = "";
    cloneError = null;
  }

  async function browse() {
    if (isBusy) return;
    isBrowsing = true;
    try {
      // Native folder picker: returns the full path, or null when cancelled.
      const selected = await api.pickDirectory();
      if (selected) path = selected;
    } catch (e: any) {
      addToast({ message: e.message || "Failed to open folder picker", kind: "error" });
    } finally {
      isBrowsing = false;
    }
  }

  async function browseDestination() {
    if (isBusy) return;
    isBrowsing = true;
    try {
      const selected = await api.pickDirectory();
      if (selected) cloneDest = selected;
    } catch (e: any) {
      cloneError = e.message || "Failed to open folder picker";
    } finally {
      isBrowsing = false;
    }
  }

  async function handleSubmit(e: SubmitEvent) {
    e.preventDefault();
    if (!canSubmit) return;
    isAdding = true;
    try {
      // Duplicate-path/branch rules and invalid-repository errors come from the
      // backend and remain authoritative.
      const entry = await api.repoAdd(path.trim(), branch.trim() || undefined);
      if (!saveRepoIcon(entry.id, choice)) {
        addToast({ message: "Repository added, but its icon could not be saved.", kind: "error" });
      } else {
        addToast({ message: `Added repository ${entry.name}`, kind: "success" });
      }
      await loadRepos();
      activeRepoId.set(entry.id);
      openRepoInFocusedPane(entry.id);
      reset();
      $addRepoDialogOpen = false;
    } catch (err: any) {
      addToast({ message: err.message || "Failed to add repository", kind: "error" });
    } finally {
      isAdding = false;
    }
  }

  async function handleCloneSubmit(e: SubmitEvent) {
    e.preventDefault();
    if (!canClone) return;
    isCloning = true;
    cloneError = null;
    cloneProgress = "Starting clone…";

    // Scope progress events to this clone so concurrent attempts don't mix.
    const cloneId = `clone-${Math.random().toString(36).slice(2, 10)}`;
    let unlisten: UnlistenFn | null = null;
    try {
      unlisten = await api.onCloneProgress((p) => {
        if (p.cloneId === cloneId) cloneProgress = p.line;
      });

      const entry = await api.repoClone(
        cloneId,
        cloneUrl.trim(),
        cloneDest.trim(),
        cloneName.trim() || undefined,
        cloneBranch.trim() || undefined
      );

      if (!saveRepoIcon(entry.id, cloneChoice)) {
        addToast({ message: "Repository cloned, but its icon could not be saved.", kind: "error" });
      }

      addToast({ message: `Cloned ${entry.name} into ${entry.path}`, kind: "success" });
      await loadRepos();
      activeRepoId.set(entry.id);
      openRepoInFocusedPane(entry.id);
      resetClone();
      $addRepoDialogOpen = false;
    } catch (err: any) {
      // Bad URL, destination conflict, missing credentials, and network errors
      // all surface here with a clear message (never a silent console error).
      const message = err?.message || "Clone failed";
      cloneError = message;
      addToast({ message, kind: "error", details: err?.stderr || undefined });
    } finally {
      unlisten?.();
      isCloning = false;
    }
  }
</script>

{#if $addRepoDialogOpen}
    <Modal
    open={$addRepoDialogOpen}
    title="Add repository"
    width="480px"
    showCloseButton={!isBusy}
    closeOnEscape={!isBusy}
    closeOnOutsideClick={!isBusy}
    onclose={close}
  >
    <div class="mode-tabs" role="tablist" aria-label="Repository source">
      <button
        type="button"
        role="tab"
        aria-selected={tab === "existing"}
        class="mode-tab"
        class:active={tab === "existing"}
        disabled={isBusy}
        onclick={() => (tab = "existing")}
      >
        Add existing
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={tab === "clone"}
        class="mode-tab"
        class:active={tab === "clone"}
        disabled={isBusy}
        onclick={() => (tab = "clone")}
      >
        Clone
      </button>
    </div>

    {#if tab === "existing"}
      <form class="add-repo-form" onsubmit={handleSubmit}>
        <label for="repo-path-input" class="field-label">Repository</label>
        <div class="path-control">
          <input
            id="repo-path-input"
            type="text"
            bind:value={path}
            placeholder="~/projects/my-app"
            disabled={isAdding}
            spellcheck="false"
            autocomplete="off"
          />
          <button
            type="button"
            class="browse-btn"
            onclick={browse}
            disabled={isBusy}
            title="Choose a folder"
          >
            <Icon name="folder" size={14} />
            {isBrowsing ? "Browsing…" : "Browse"}
          </button>
        </div>

        <label for="repo-branch-input" class="field-label">Branch</label>
        <input
          id="repo-branch-input"
          class="branch-input"
          type="text"
          bind:value={branch}
          placeholder="main"
          disabled={isAdding}
          spellcheck="false"
          autocomplete="off"
        />

        <div class="picker-label">Repository Icon</div>
        <RepoIconPicker
          value={choice}
          defaultLetter={effectiveExistingName}
          onchange={(newIcon) => (choice = newIcon)}
        />

        <div class="add-actions">
          <button type="button" class="cancel-btn" onclick={close} disabled={isBusy}>
            Cancel
          </button>
          <button type="submit" class="primary-btn" disabled={!canSubmit}>
            {isAdding ? "Adding…" : "Add repository"}
          </button>
        </div>
      </form>
    {:else}
      <form class="clone-form" onsubmit={handleCloneSubmit}>
        <label for="clone-url-input" class="field-label">Repository URL or local path</label>
        <input
          id="clone-url-input"
          class="clone-input"
          type="text"
          bind:value={cloneUrl}
          placeholder="https://github.com/owner/repo.git"
          disabled={isCloning}
          spellcheck="false"
          autocomplete="off"
        />

        <label for="clone-dest-input" class="field-label">Clone into</label>
        <div class="path-control">
          <input
            id="clone-dest-input"
            type="text"
            bind:value={cloneDest}
            placeholder="Choose a destination folder"
            disabled={isCloning}
            spellcheck="false"
            autocomplete="off"
          />
          <button
            type="button"
            class="browse-btn"
            onclick={browseDestination}
            disabled={isBusy}
            title="Choose a destination folder"
          >
            <Icon name="folder" size={14} />
            {isBrowsing ? "Browsing…" : "Browse"}
          </button>
        </div>

        <label for="clone-name-input" class="field-label">Folder name</label>
        <input
          id="clone-name-input"
          class="clone-input"
          type="text"
          bind:value={cloneName}
          oninput={() => (cloneNameTouched = cloneName.trim().length > 0)}
          placeholder={cloneSuggestedName || "repo"}
          disabled={isCloning}
          spellcheck="false"
          autocomplete="off"
          aria-invalid={!!cloneNameError}
        />
        {#if cloneNameError}
          <p class="field-error">{cloneNameError}</p>
        {/if}

        <label for="clone-branch-input" class="field-label">Branch (optional)</label>
        <input
          id="clone-branch-input"
          class="clone-input"
          type="text"
          bind:value={cloneBranch}
          placeholder="Default branch"
          disabled={isCloning}
          spellcheck="false"
          autocomplete="off"
        />

        {#if resolvedDest}
          <p class="clone-resolved">Will clone into <code>{resolvedDest}</code></p>
        {/if}

        <div class="picker-label">Repository Icon</div>
        <RepoIconPicker
          value={cloneChoice}
          defaultLetter={effectiveCloneName}
          onchange={(newIcon) => (cloneChoice = newIcon)}
        />

        {#if isCloning}
          <div class="clone-progress" role="status" aria-live="polite">
            <span class="spinner" aria-hidden="true"></span>
            <span class="clone-progress-text">{cloneProgress || "Cloning…"}</span>
          </div>
        {/if}

        {#if cloneError}
          <div class="clone-error" role="alert">{cloneError}</div>
        {/if}

        <div class="add-actions">
          <button type="button" class="cancel-btn" onclick={close} disabled={isBusy}>
            Cancel
          </button>
          <button type="submit" class="primary-btn" disabled={!canClone}>
            {isCloning ? "Cloning…" : "Clone repository"}
          </button>
        </div>
      </form>
    {/if}
  </Modal>
{/if}

<style>
  .add-repo-form {
    color: #383c41;
  }

  .field-label,
  .picker-label {
    display: block;
    color: #646b73;
    font-size: 12px;
    font-weight: 500;
    margin: 0 0 5px;
  }

  .path-control {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 2px;
    margin-bottom: 12px;
    border: 1px solid #dedfe2;
    border-radius: 7px;
  }
  .path-control:focus-within {
    border-color: #a1adbb;
  }

  .path-control input {
    width: 100%;
    min-width: 0;
    height: 27px;
    padding: 4px 7px;
    border: 0;
    background: transparent;
    color: #3e454d;
    font-size: 12px;
    outline: none;
  }
  .path-control input:disabled {
    opacity: 0.6;
  }

  .browse-btn {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 5px 8px;
    background: #f0f1f3;
    border: 0;
    border-radius: 5px;
    font-size: 11px;
    color: #505963;
    cursor: pointer;
    flex-shrink: 0;
  }
  .browse-btn:hover:not(:disabled) {
    background: #e7e9ec;
  }
  .browse-btn:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .branch-input {
    display: block;
    width: 100%;
    height: 31px;
    padding: 6px 9px;
    border: 1px solid #dedfe2;
    border-radius: 7px;
    font-size: 12px;
    color: #454545;
    background: transparent;
    outline: none;
  }
  .branch-input:focus {
    border-color: #a1adbb;
  }

  .picker-label {
    margin: 15px 0 7px;
  }

  .add-actions {
    display: flex;
    justify-content: flex-end;
    gap: 7px;
    margin-top: 17px;
    padding-top: 13px;
    border-top: 1px solid #eee;
  }
  .add-actions button {
    border: 0;
    border-radius: 7px;
    padding: 7px 11px;
    font-size: 12px;
    cursor: pointer;
    background: none;
  }
  .add-actions .cancel-btn {
    color: var(--text-muted);
    background: none;
  }
  .add-actions .cancel-btn:hover:not(:disabled) {
    background: #f0f1f3;
  }
  .add-actions .primary-btn {
    background: #363d45;
    color: #fff;
  }
  .add-actions .primary-btn:hover:not(:disabled) {
    background: #2b323a;
  }
  .add-actions button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  /* ---- Clone tab (token-based, per DESIGN.md) ---- */
  .mode-tabs {
    display: flex;
    gap: 4px;
    padding: 3px;
    margin-bottom: 16px;
    background-color: var(--bg-subtle);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
  }
  .mode-tab {
    flex: 1;
    height: 26px;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--text-muted);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }
  .mode-tab:hover:not(:disabled) {
    color: var(--text);
  }
  .mode-tab.active {
    background-color: var(--bg);
    color: var(--text);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.08);
  }
  .mode-tab:disabled {
    opacity: 0.55;
    cursor: default;
  }

  .clone-form {
    display: flex;
    flex-direction: column;
  }
  .clone-form .field-label {
    color: var(--text-muted);
  }

  .clone-input {
    display: block;
    width: 100%;
    height: 31px;
    padding: 6px 9px;
    margin-bottom: 12px;
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    font-size: 12px;
    color: var(--text);
    background-color: var(--bg);
    outline: none;
  }
  .clone-input:focus {
    border-color: var(--accent);
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .clone-input:disabled {
    opacity: 0.6;
  }
  .clone-input[aria-invalid="true"] {
    border-color: var(--danger);
    margin-bottom: 4px;
  }

  .clone-form .path-control {
    border-color: var(--border);
    background-color: var(--bg);
  }
  .clone-form .path-control:focus-within {
    border-color: var(--accent);
  }
  .clone-form .path-control input {
    color: var(--text);
  }
  .clone-form .browse-btn {
    background-color: var(--bg-muted);
    color: var(--text-muted);
  }
  .clone-form .browse-btn:hover:not(:disabled) {
    background-color: var(--border-subtle);
  }

  .field-error {
    margin: 0 0 12px;
    font-size: 11px;
    color: var(--danger);
  }

  .clone-resolved {
    margin: 0 0 12px;
    font-size: 11px;
    color: var(--text-muted);
    overflow-wrap: anywhere;
  }
  .clone-resolved code {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text);
  }

  .clone-progress {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    margin-bottom: 12px;
    background-color: var(--accent-bg);
    border-radius: var(--r-md);
    font-size: 11px;
    color: var(--text);
  }
  .clone-progress-text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .spinner {
    width: 12px;
    height: 12px;
    flex-shrink: 0;
    border: 2px solid var(--accent);
    border-top-color: transparent;
    border-radius: 50%;
    animation: clone-spin 0.7s linear infinite;
  }
  @keyframes clone-spin {
    to {
      transform: rotate(360deg);
    }
  }

  .clone-error {
    padding: 8px 10px;
    margin-bottom: 12px;
    background-color: var(--warning-bg);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    font-size: 12px;
    color: var(--text);
    overflow-wrap: anywhere;
  }

  @media (prefers-reduced-motion: reduce) {
    .spinner {
      animation: none;
    }
  }
</style>
