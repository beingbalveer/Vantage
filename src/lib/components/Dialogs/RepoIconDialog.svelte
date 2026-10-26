<script lang="ts">
  import { activeDialog, closeDialog, addToast } from "../../stores/ui";
  import {
    repoIcons,
    detectedStacks,
    saveRepoIcon,
    clearRepoIcon,
    type RepoIcon,
  } from "../../stores/repoIcons";
  import type { RepoEntry } from "../../types";
  import { Modal, Button, Icon } from "../common";
  import RepoIconView from "../Sidebar/RepoIcon.svelte";
  import RepoIconPicker from "./RepoIconPicker.svelte";

  let isOpen = $derived($activeDialog?.type === "change_icon");
  let repo = $derived<RepoEntry | null>($activeDialog?.data?.repo ?? null);
  let repoId = $derived(repo?.id ?? $activeDialog?.repoId ?? "");

  let currentOverride = $derived(repoId ? $repoIcons[repoId] : undefined);
  let detectedInfo = $derived(repoId ? $detectedStacks[repoId] : undefined);

  let pickerValue = $state<RepoIcon>({ kind: "auto" });

  $effect(() => {
    if (isOpen && repo) {
      if (currentOverride) {
        pickerValue = currentOverride;
      } else {
        pickerValue = {
          kind: "letter",
          value: (repo.name || "R").charAt(0).toUpperCase(),
        };
      }
    }
  });

  function handleIconChange(newIcon: RepoIcon) {
    if (!repoId) return;
    pickerValue = newIcon;
    saveRepoIcon(repoId, newIcon);
    addToast({ message: "Repository icon updated", kind: "success" });
  }

  function handleResetAuto() {
    if (!repoId) return;
    clearRepoIcon(repoId);
    pickerValue = {
      kind: "letter",
      value: (repo?.name || "R").charAt(0).toUpperCase(),
    };
    addToast({ message: "Reset to auto-detected icon", kind: "info" });
  }

  function handleClose() {
    closeDialog();
  }
</script>

<Modal
  open={isOpen}
  title="Change Repository Icon"
  description="Select a technology logo, category symbol, emoji, letter initial, or custom file for this repository."
  width={480}
  onclose={handleClose}
>
  {#if repo}
    <!-- Current Preview Banner -->
    <div class="preview-banner">
      <div class="preview-box">
        <RepoIconView id={repo.id} repoName={repo.name} repoPath={repo.path} size={24} />
      </div>
      <div class="preview-meta">
        <div class="repo-title">{repo.name}</div>
        <div class="repo-sub">
          {#if currentOverride}
            <span class="custom-badge">Custom icon active</span>
          {:else if detectedInfo?.detected}
            <span class="auto-badge">Auto-detected: <strong>{detectedInfo.label || detectedInfo.detected}</strong> ({detectedInfo.matchedFile})</span>
          {:else}
            <span class="fallback-badge">Using default letter initial</span>
          {/if}
        </div>
      </div>
      {#if currentOverride}
        <button
          type="button"
          class="reset-auto-btn"
          onclick={handleResetAuto}
          title="Remove custom icon and use auto-detection"
        >
          <Icon name="refresh" size={13} />
          Reset to Auto
        </button>
      {/if}
    </div>

    <!-- Reusable Icon Picker -->
    <RepoIconPicker
      value={pickerValue}
      defaultLetter={repo.name || "R"}
      onchange={handleIconChange}
    />
  {/if}

  {#snippet footer()}
    <div class="dialog-footer">
      <Button variant="primary" onclick={handleClose}>
        Done
      </Button>
    </div>
  {/snippet}
</Modal>

<style>
  .preview-banner {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    background-color: var(--bg-muted, #f3f4f6);
    border: 1px solid var(--border-subtle, #e5e7eb);
    border-radius: 12px;
    margin-bottom: 14px;
  }
  :global([data-theme="dark"]) .preview-banner {
    background-color: #1c2128;
    border-color: #30363d;
  }

  .preview-box {
    width: 36px;
    height: 36px;
    border-radius: 10px;
    display: flex;
    align-items: center;
    justify-content: center;
    background-color: #ffffff;
    border: 1px solid #e1e4e8;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.04);
    flex-shrink: 0;
  }
  :global([data-theme="dark"]) .preview-box {
    background-color: #21262d;
    border-color: #30363d;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.3);
  }

  .preview-meta {
    flex: 1;
    min-width: 0;
  }

  .repo-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .repo-sub {
    font-size: 11px;
    color: var(--text-muted);
    margin-top: 1px;
  }

  .custom-badge {
    color: var(--accent, #2563eb);
    font-weight: 500;
  }

  .auto-badge {
    color: var(--success);
  }

  .auto-badge strong {
    font-weight: 600;
  }

  .fallback-badge {
    color: var(--text-muted);
  }

  .reset-auto-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    font-weight: 500;
    padding: 5px 9px;
    border-radius: 6px;
    border: 1px solid var(--border, #d1d5db);
    background: var(--bg);
    color: var(--text);
    cursor: pointer;
    white-space: nowrap;
    transition: background 120ms ease;
  }
  .reset-auto-btn:hover {
    background-color: var(--bg-subtle, #e5e7eb);
  }

  .dialog-footer {
    display: flex;
    justify-content: flex-end;
    width: 100%;
  }
</style>
