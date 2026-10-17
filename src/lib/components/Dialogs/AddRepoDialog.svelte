<script lang="ts">
  import { addRepoDialogOpen, addToast } from "../../stores/ui";
  import { loadRepos, activeRepoId } from "../../stores/repos";
  import { openRepoInFocusedPane } from "../../stores/panes";
  import { appIcons, saveRepoIcon, readIconFile, type RepoIcon } from "../../stores/repoIcons";
  import * as api from "../../api";
  import { Modal, Icon } from "../common";
  import RepoIconView from "../Sidebar/RepoIcon.svelte";

  let path = $state("");
  let branch = $state("");
  let choice = $state<RepoIcon>({ kind: "app", value: "code" });
  let uploadName = $state("");
  let isAdding = $state(false);
  let isBrowsing = $state(false);

  let isBusy = $derived(isAdding || isBrowsing);
  let canSubmit = $derived(!!path.trim() && !isBusy);

  function close() {
    if (isBusy) return;
    $addRepoDialogOpen = false;
  }

  function reset() {
    path = "";
    branch = "";
    choice = { kind: "app", value: "code" };
    uploadName = "";
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

  async function handleUpload(e: Event) {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    try {
      // Read as a data URL and render it through <img> only — SVG is never
      // injected as raw HTML (RELAY-UI · UI-07).
      const data = await readIconFile(file);
      choice = { kind: "image", value: data };
      uploadName = file.name;
    } catch (err: any) {
      addToast({ message: err?.message || "Could not read that image.", kind: "error" });
    } finally {
      // Allow re-selecting the same file.
      input.value = "";
    }
  }

  function selectPreset(value: (typeof appIcons)[number]) {
    choice = { kind: "app", value };
    uploadName = "";
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
</script>

{#if $addRepoDialogOpen}
  <Modal
    open={$addRepoDialogOpen}
    title="Add repository"
    width="390px"
    showCloseButton={!isBusy}
    closeOnEscape={!isBusy}
    closeOnOutsideClick={!isBusy}
    onclose={close}
  >
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

      <div class="picker-label">Icon</div>
      <div class="icon-picker" role="group" aria-label="Repository icon">
        {#each appIcons as value (value)}
          <button
            type="button"
            class="preset"
            class:selected={choice.kind === "app" && choice.value === value}
            aria-label="{value} icon"
            aria-pressed={choice.kind === "app" && choice.value === value}
            disabled={isAdding}
            onclick={() => selectPreset(value)}
          >
            <RepoIconView choice={{ kind: "app", value }} size={20} />
          </button>
        {/each}
      </div>

      <label class="upload-icon">
        <Icon name="plus" size={15} />
        Upload icon
        <input
          type="file"
          accept="image/png,image/jpeg,image/gif,image/webp,image/svg+xml,image/x-icon"
          onchange={handleUpload}
          disabled={isAdding}
          hidden
        />
      </label>

      {#if uploadName && choice.kind === "image"}
        <div class="upload-preview">
          <RepoIconView {choice} size={20} />
          <span class="upload-name">{uploadName}</span>
        </div>
      {/if}

      <div class="add-actions">
        <button type="button" class="cancel-btn" onclick={close} disabled={isBusy}>
          Cancel
        </button>
        <button type="submit" class="primary-btn" disabled={!canSubmit}>
          {isAdding ? "Adding…" : "Add repository"}
        </button>
      </div>
    </form>
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

  .icon-picker {
    display: flex;
    gap: 6px;
  }
  .preset {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    background: #f7f7f8;
    border: 0;
    border-radius: 7px;
    cursor: pointer;
  }
  .preset:hover:not(:disabled) {
    background: #eef0f2;
  }
  .preset.selected {
    background: #e5e9ee;
    box-shadow: inset 0 0 0 1px #aeb9c6;
  }
  .preset:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .upload-icon {
    display: flex;
    align-items: center;
    gap: 7px;
    margin-top: 10px;
    font-size: 11px;
    color: #737c86;
    cursor: pointer;
  }
  .upload-icon:hover {
    color: #454b53;
  }

  .upload-preview {
    display: flex;
    align-items: center;
    gap: 9px;
    margin-top: 10px;
    font-size: 11px;
    color: #777;
    overflow: hidden;
  }
  .upload-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
    color: #737b84;
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
</style>
