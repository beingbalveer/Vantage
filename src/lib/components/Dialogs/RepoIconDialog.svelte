<script lang="ts">
  import { activeDialog, closeDialog, addToast } from "../../stores/ui";
  import {
    repoIcons,
    detectedStacks,
    saveRepoIcon,
    clearRepoIcon,
    readIconFile,
    techIcons,
    roleIcons,
    presetEmojis,
    type RepoIcon,
  } from "../../stores/repoIcons";
  import * as api from "../../api";
  import type { RepoEntry } from "../../types";
  import { Modal, Button, Icon, TechIcon } from "../common";
  import RepoIconView from "../Sidebar/RepoIcon.svelte";

  let isOpen = $derived($activeDialog?.type === "change_icon");
  let repo = $derived<RepoEntry | null>($activeDialog?.data?.repo ?? null);
  let repoId = $derived(repo?.id ?? $activeDialog?.repoId ?? "");

  let currentOverride = $derived(repoId ? $repoIcons[repoId] : undefined);
  let detectedInfo = $derived(repoId ? $detectedStacks[repoId] : undefined);

  type TabId = "tech" | "role" | "emoji" | "letter" | "custom";
  let activeTab = $state<TabId>("tech");

  // Letter tab state
  let customLetter = $state("");
  let selectedColor = $state<string | undefined>(undefined);

  const colorPalette = [
    { label: "Default", value: undefined },
    { label: "Blue", value: "#2563eb" },
    { label: "Indigo", value: "#6366f1" },
    { label: "Purple", value: "#9333ea" },
    { label: "Pink", value: "#ec4899" },
    { label: "Red", value: "#ef4444" },
    { label: "Orange", value: "#f97316" },
    { label: "Green", value: "#10b981" },
    { label: "Teal", value: "#14b8a6" },
  ];

  $effect(() => {
    if (isOpen && repo) {
      if (currentOverride?.kind === "letter") {
        customLetter = currentOverride.value;
        selectedColor = currentOverride.color;
      } else {
        customLetter = (repo.name || "R").charAt(0).toUpperCase();
        selectedColor = undefined;
      }
    }
  });

  function selectTech(id: string) {
    if (!repoId) return;
    saveRepoIcon(repoId, { kind: "tech", value: id });
    addToast({ message: `Repository icon set to ${id}`, kind: "success" });
  }

  function selectRole(id: string) {
    if (!repoId) return;
    saveRepoIcon(repoId, { kind: "role", value: id });
    addToast({ message: `Repository icon set to ${id}`, kind: "success" });
  }

  function selectEmoji(char: string) {
    if (!repoId) return;
    saveRepoIcon(repoId, { kind: "emoji", value: char });
    addToast({ message: `Repository icon set to ${char}`, kind: "success" });
  }

  function applyLetter() {
    if (!repoId) return;
    const letter = customLetter.trim().slice(0, 3).toUpperCase() || (repo?.name || "R").charAt(0).toUpperCase();
    saveRepoIcon(repoId, { kind: "letter", value: letter, color: selectedColor });
    addToast({ message: `Repository icon set to "${letter}"`, kind: "success" });
  }

  async function handlePickFile() {
    if (!repoId) return;
    try {
      // First try native Tauri picker if available
      const dataUrl = await api.pickIconFile();
      if (dataUrl) {
        saveRepoIcon(repoId, { kind: "image", value: dataUrl });
        addToast({ message: "Custom icon applied", kind: "success" });
      }
    } catch (e: any) {
      addToast({ message: e.message || "Failed to pick icon file", kind: "error" });
    }
  }

  async function handleHtmlFileInput(e: Event) {
    const target = e.target as HTMLInputElement;
    const file = target.files?.[0];
    if (!file || !repoId) return;
    try {
      const dataUrl = await readIconFile(file);
      saveRepoIcon(repoId, { kind: "image", value: dataUrl });
      addToast({ message: "Custom icon uploaded", kind: "success" });
    } catch (err: any) {
      addToast({ message: err.message || "Failed to read image", kind: "error" });
    }
    target.value = "";
  }

  function handleResetAuto() {
    if (!repoId) return;
    clearRepoIcon(repoId);
    addToast({ message: "Reset to auto-detected icon", kind: "info" });
  }

  function handleClose() {
    closeDialog();
  }
</script>

<Modal
  open={isOpen}
  title="Change Repository Icon"
  description="Select a technology logo, category symbol, emoji, or custom image for this repository."
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

    <!-- Category Tabs -->
    <div class="tab-bar" role="tablist">
      <button
        type="button"
        role="tab"
        aria-selected={activeTab === "tech"}
        class="tab-btn"
        class:active={activeTab === "tech"}
        onclick={() => (activeTab = "tech")}
      >
        Tech Stack
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={activeTab === "role"}
        class="tab-btn"
        class:active={activeTab === "role"}
        onclick={() => (activeTab = "role")}
      >
        Roles
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={activeTab === "emoji"}
        class="tab-btn"
        class:active={activeTab === "emoji"}
        onclick={() => (activeTab = "emoji")}
      >
        Emojis
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={activeTab === "letter"}
        class="tab-btn"
        class:active={activeTab === "letter"}
        onclick={() => (activeTab = "letter")}
      >
        Letter
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={activeTab === "custom"}
        class="tab-btn"
        class:active={activeTab === "custom"}
        onclick={() => (activeTab = "custom")}
      >
        Custom File
      </button>
    </div>

    <!-- Tab Contents -->
    <div class="tab-panel">
      {#if activeTab === "tech"}
        <div class="icon-grid">
          {#each techIcons as item (item.id)}
            {@const isSelected = currentOverride?.kind === "tech" && currentOverride.value === item.id}
            {@const isDetected = !currentOverride && detectedInfo?.detected === item.id}
            <button
              type="button"
              class="icon-card"
              class:selected={isSelected}
              class:detected={isDetected}
              onclick={() => selectTech(item.id)}
              title={item.label}
            >
              <div class="icon-wrapper">
                <TechIcon name={item.id} size={24} />
              </div>
              <span class="icon-label">{item.label}</span>
              {#if isDetected}
                <span class="detected-dot" title="Auto-detected"></span>
              {/if}
            </button>
          {/each}
        </div>
      {:else if activeTab === "role"}
        <div class="icon-grid">
          {#each roleIcons as item (item.id)}
            {@const isSelected = currentOverride?.kind === "role" && currentOverride.value === item.id}
            <button
              type="button"
              class="icon-card"
              class:selected={isSelected}
              onclick={() => selectRole(item.id)}
              title={item.label}
            >
              <div class="icon-wrapper role">
                <TechIcon name={item.id} size={22} />
              </div>
              <span class="icon-label">{item.label}</span>
            </button>
          {/each}
        </div>
      {:else if activeTab === "emoji"}
        <div class="emoji-grid">
          {#each presetEmojis as emoji (emoji)}
            {@const isSelected = currentOverride?.kind === "emoji" && currentOverride.value === emoji}
            <button
              type="button"
              class="emoji-card"
              class:selected={isSelected}
              onclick={() => selectEmoji(emoji)}
            >
              <span class="emoji-char">{emoji}</span>
            </button>
          {/each}
        </div>
      {:else if activeTab === "letter"}
        <div class="letter-customizer">
          <div class="form-row">
            <label for="letter-input" class="field-label">Initial Letter(s):</label>
            <input
              id="letter-input"
              type="text"
              class="letter-input"
              maxlength="3"
              bind:value={customLetter}
              oninput={applyLetter}
              placeholder="R"
            />
          </div>
          <div class="color-palette-section">
            <span class="field-label">Color Accent:</span>
            <div class="palette-options">
              {#each colorPalette as c}
                <button
                  type="button"
                  class="color-chip"
                  class:selected={selectedColor === c.value}
                  style={c.value ? `background-color: ${c.value};` : "background: #1a1c1f;"}
                  onclick={() => {
                    selectedColor = c.value;
                    applyLetter();
                  }}
                  title={c.label}
                  aria-label={c.label}
                ></button>
              {/each}
            </div>
          </div>
        </div>
      {:else if activeTab === "custom"}
        <div class="custom-upload-section">
          <p class="upload-hint">
            Upload any PNG, SVG, ICO, JPG, or WebP image under 512 KB. It will be stored locally with your repository settings.
          </p>
          <div class="upload-buttons">
            <Button variant="default" onclick={handlePickFile}>
              <Icon name="folder" size={14} class="mr-1 inline-block" />
              Choose file from system…
            </Button>

            <label class="native-file-label">
              <input
                type="file"
                accept="image/png,image/svg+xml,image/x-icon,image/jpeg,image/webp"
                onchange={handleHtmlFileInput}
                hidden
              />
              <span class="native-file-btn">Browse file…</span>
            </label>
          </div>
        </div>
      {/if}
    </div>
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

  .tab-bar {
    display: flex;
    border-bottom: 1px solid var(--border-subtle, #e5e7eb);
    gap: 4px;
    margin-bottom: 12px;
  }

  .tab-btn {
    padding: 6px 12px;
    font-size: 12px;
    font-weight: 500;
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    color: var(--text-muted);
    cursor: pointer;
    transition: color 120ms ease, border-color 120ms ease;
  }
  .tab-btn:hover {
    color: var(--text);
  }
  .tab-btn.active {
    color: var(--accent, #2563eb);
    border-bottom-color: var(--accent, #2563eb);
    font-weight: 600;
  }

  .tab-panel {
    min-height: 220px;
    max-height: 280px;
    overflow-y: auto;
    padding-right: 4px;
  }

  /* Tech and Role Grids */
  .icon-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 8px;
  }

  .icon-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 10px 6px;
    border-radius: 10px;
    border: 1px solid var(--border-subtle, #e5e7eb);
    background-color: var(--bg);
    cursor: pointer;
    position: relative;
    transition: transform 80ms ease, border-color 120ms ease, background-color 120ms ease;
  }
  .icon-card:hover {
    border-color: var(--accent, #2563eb);
    background-color: var(--bg-subtle, #f9fafb);
  }
  .icon-card.selected {
    border-color: var(--accent, #2563eb);
    background-color: rgba(37, 99, 235, 0.08);
    box-shadow: 0 0 0 1px var(--accent, #2563eb);
  }

  .icon-wrapper {
    width: 32px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .icon-wrapper.role {
    color: var(--text);
  }

  .icon-label {
    font-size: 11px;
    color: var(--text);
    font-weight: 500;
    text-align: center;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 100%;
  }

  .detected-dot {
    position: absolute;
    top: 5px;
    right: 5px;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background-color: #10b981;
  }

  /* Emoji Grid */
  .emoji-grid {
    display: grid;
    grid-template-columns: repeat(6, 1fr);
    gap: 8px;
  }

  .emoji-card {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 48px;
    border-radius: 10px;
    border: 1px solid var(--border-subtle, #e5e7eb);
    background-color: var(--bg);
    cursor: pointer;
    transition: transform 80ms ease, border-color 120ms ease;
  }
  .emoji-card:hover {
    border-color: var(--accent, #2563eb);
    background-color: var(--bg-subtle, #f9fafb);
  }
  .emoji-card.selected {
    border-color: var(--accent, #2563eb);
    background-color: rgba(37, 99, 235, 0.08);
    box-shadow: 0 0 0 1px var(--accent, #2563eb);
  }

  .emoji-char {
    font-size: 22px;
    line-height: 1;
  }

  /* Letter Customizer */
  .letter-customizer {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 8px 0;
  }

  .form-row {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .field-label {
    font-size: 12px;
    font-weight: 500;
    color: var(--text);
    min-width: 110px;
  }

  .letter-input {
    width: 60px;
    height: 36px;
    text-align: center;
    font-size: 18px;
    font-weight: 700;
    border: 1px solid var(--border);
    border-radius: 8px;
    background-color: var(--bg-inset);
    color: var(--text);
    outline: none;
    text-transform: uppercase;
  }
  .letter-input:focus {
    border-color: var(--accent);
  }

  .color-palette-section {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .palette-options {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .color-chip {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    border: 2px solid transparent;
    cursor: pointer;
    transition: transform 80ms ease, box-shadow 80ms ease;
  }
  .color-chip:hover {
    transform: scale(1.15);
  }
  .color-chip.selected {
    box-shadow: 0 0 0 2px var(--bg), 0 0 0 4px var(--accent, #2563eb);
  }

  /* Custom Upload */
  .custom-upload-section {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 12px 0;
  }

  .upload-hint {
    font-size: 12px;
    color: var(--text-muted);
    line-height: 1.5;
    margin: 0;
  }

  .upload-buttons {
    display: flex;
    gap: 10px;
    align-items: center;
    flex-wrap: wrap;
  }

  .native-file-label {
    cursor: pointer;
  }

  .native-file-btn {
    display: inline-flex;
    align-items: center;
    height: 32px;
    padding: 0 12px;
    font-size: 12px;
    font-weight: 500;
    border-radius: var(--r-sm, 6px);
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text);
    cursor: pointer;
  }
  .native-file-btn:hover {
    background-color: var(--bg-subtle);
  }

  .dialog-footer {
    display: flex;
    justify-content: flex-end;
    width: 100%;
  }
</style>
