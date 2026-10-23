<script lang="ts">
  import {
    techIcons,
    roleIcons,
    presetEmojis,
    readIconFile,
    type RepoIcon,
  } from "../../stores/repoIcons";
  import * as api from "../../api";
  import { Icon, TechIcon, Button } from "../common";
  import RepoIconView from "../Sidebar/RepoIcon.svelte";

  interface Props {
    value: RepoIcon;
    defaultLetter?: string;
    onchange: (icon: RepoIcon) => void;
  }

  let { value, defaultLetter = "R", onchange }: Props = $props();

  type TabId = "tech" | "role" | "emoji" | "letter" | "custom";
  let activeTab = $state<TabId>("tech");

  let customLetter = $state("");
  let selectedColor = $state<string | undefined>(undefined);
  let uploadFileName = $state("");

  $effect(() => {
    if (value.kind === "letter") {
      customLetter = value.value;
      selectedColor = value.color;
    } else {
      customLetter = (defaultLetter.slice(0, 3).toUpperCase() || "R");
      selectedColor = undefined;
    }
  });

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

  function selectTech(id: string) {
    onchange({ kind: "tech", value: id });
  }

  function selectRole(id: string) {
    onchange({ kind: "role", value: id });
  }

  function selectEmoji(char: string) {
    onchange({ kind: "emoji", value: char });
  }

  function applyLetter() {
    const letter = customLetter.trim().slice(0, 3).toUpperCase() || (defaultLetter || "R").charAt(0).toUpperCase();
    onchange({ kind: "letter", value: letter, color: selectedColor });
  }

  async function handlePickFile() {
    try {
      const dataUrl = await api.pickIconFile();
      if (dataUrl) {
        uploadFileName = "Custom file";
        onchange({ kind: "image", value: dataUrl });
      }
    } catch {
      // ignore or fallback
    }
  }

  async function handleHtmlFileInput(e: Event) {
    const target = e.target as HTMLInputElement;
    const file = target.files?.[0];
    if (!file) return;
    try {
      const dataUrl = await readIconFile(file);
      uploadFileName = file.name;
      onchange({ kind: "image", value: dataUrl });
    } catch {
      // ignore
    }
    target.value = "";
  }
</script>

<div class="repo-icon-selector">
  <!-- Selected Icon Preview Bar -->
  <div class="selection-preview-bar">
    <div class="preview-box">
      <RepoIconView choice={value} size={22} />
    </div>
    <div class="preview-meta">
      <div class="preview-title">
        {#if value.kind === "tech"}
          Tech: {techIcons.find((t) => t.id === value.value)?.label ?? value.value}
        {:else if value.kind === "role"}
          Role: {roleIcons.find((r) => r.id === value.value)?.label ?? value.value}
        {:else if value.kind === "emoji"}
          Emoji: {value.value}
        {:else if value.kind === "letter"}
          Letter: "{value.value}"
        {:else if value.kind === "image"}
          Custom Image {uploadFileName ? `(${uploadFileName})` : ""}
        {:else if value.kind === "app"}
          Default: {value.value}
        {:else}
          Auto-detect
        {/if}
      </div>
      <div class="preview-sub">Selected icon for repository</div>
    </div>
  </div>

  <!-- Tabs -->
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
          {@const isSelected = value.kind === "tech" && value.value === item.id}
          <button
            type="button"
            class="icon-card"
            class:selected={isSelected}
            onclick={() => selectTech(item.id)}
            title={item.label}
          >
            <div class="icon-wrapper">
              <TechIcon name={item.id} size={22} />
            </div>
            <span class="icon-label">{item.label}</span>
          </button>
        {/each}
      </div>
    {:else if activeTab === "role"}
      <div class="icon-grid">
        {#each roleIcons as item (item.id)}
          {@const isSelected = value.kind === "role" && value.value === item.id}
          <button
            type="button"
            class="icon-card"
            class:selected={isSelected}
            onclick={() => selectRole(item.id)}
            title={item.label}
          >
            <div class="icon-wrapper role">
              <TechIcon name={item.id} size={20} />
            </div>
            <span class="icon-label">{item.label}</span>
          </button>
        {/each}
      </div>
    {:else if activeTab === "emoji"}
      <div class="emoji-grid">
        {#each presetEmojis as emoji (emoji)}
          {@const isSelected = value.kind === "emoji" && value.value === emoji}
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
          <label for="selector-letter-input" class="field-label">Initial Letter(s):</label>
          <input
            id="selector-letter-input"
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
          Upload any PNG, SVG, ICO, JPG, or WebP image under 512 KB.
        </p>
        <div class="upload-buttons">
          <Button variant="default" onclick={handlePickFile}>
            <Icon name="folder" size={14} class="mr-1 inline-block" />
            Choose file…
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
</div>

<style>
  .repo-icon-selector {
    display: flex;
    flex-direction: column;
    margin-top: 4px;
    margin-bottom: 12px;
  }

  .selection-preview-bar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    background-color: var(--bg-muted, #f3f4f6);
    border: 1px solid var(--border-subtle, #e5e7eb);
    border-radius: 8px;
    margin-bottom: 10px;
  }
  :global([data-theme="dark"]) .selection-preview-bar {
    background-color: #1c2128;
    border-color: #30363d;
  }

  .preview-box {
    width: 32px;
    height: 32px;
    border-radius: 8px;
    display: flex;
    align-items: center;
    justify-content: center;
    background-color: var(--bg, #ffffff);
    border: 1px solid var(--border-subtle, #e1e4e8);
    flex-shrink: 0;
  }

  .preview-meta {
    flex: 1;
    min-width: 0;
  }

  .preview-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text, #111827);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .preview-sub {
    font-size: 11px;
    color: var(--text-muted, #6b7280);
  }

  .tab-bar {
    display: flex;
    border-bottom: 1px solid var(--border-subtle, #e5e7eb);
    gap: 2px;
    margin-bottom: 10px;
  }

  .tab-btn {
    padding: 5px 8px;
    font-size: 11px;
    font-weight: 500;
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    color: var(--text-muted, #6b7280);
    cursor: pointer;
    transition: color 120ms ease, border-color 120ms ease;
  }
  .tab-btn:hover {
    color: var(--text, #111827);
  }
  .tab-btn.active {
    color: var(--accent, #2563eb);
    border-bottom-color: var(--accent, #2563eb);
    font-weight: 600;
  }

  .tab-panel {
    min-height: 140px;
    max-height: 175px;
    overflow-y: auto;
    padding-right: 2px;
  }

  .icon-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 6px;
  }

  .icon-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    padding: 6px 4px;
    border-radius: 8px;
    border: 1px solid var(--border-subtle, #e5e7eb);
    background-color: var(--bg, #ffffff);
    cursor: pointer;
    transition: transform 80ms ease, border-color 120ms ease;
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
    width: 26px;
    height: 26px;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .icon-wrapper.role {
    color: var(--text, #111827);
  }

  .icon-label {
    font-size: 10px;
    color: var(--text, #374151);
    font-weight: 500;
    text-align: center;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 100%;
  }

  .emoji-grid {
    display: grid;
    grid-template-columns: repeat(6, 1fr);
    gap: 6px;
  }

  .emoji-card {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 40px;
    border-radius: 8px;
    border: 1px solid var(--border-subtle, #e5e7eb);
    background-color: var(--bg, #ffffff);
    cursor: pointer;
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
    font-size: 20px;
    line-height: 1;
  }

  .letter-customizer {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 4px 0;
  }

  .form-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .field-label {
    font-size: 11px;
    font-weight: 500;
    color: var(--text, #374151);
    min-width: 100px;
  }

  .letter-input {
    width: 50px;
    height: 32px;
    text-align: center;
    font-size: 16px;
    font-weight: 700;
    border: 1px solid var(--border, #d1d5db);
    border-radius: 6px;
    background-color: var(--bg, #ffffff);
    color: var(--text, #111827);
    outline: none;
    text-transform: uppercase;
  }

  .color-palette-section {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .palette-options {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .color-chip {
    width: 20px;
    height: 20px;
    border-radius: 50%;
    border: 2px solid transparent;
    cursor: pointer;
  }
  .color-chip:hover {
    transform: scale(1.15);
  }
  .color-chip.selected {
    box-shadow: 0 0 0 2px var(--bg, #ffffff), 0 0 0 4px var(--accent, #2563eb);
  }

  .custom-upload-section {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 8px 0;
  }

  .upload-hint {
    font-size: 11px;
    color: var(--text-muted, #6b7280);
    margin: 0;
    line-height: 1.4;
  }

  .upload-buttons {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .native-file-label {
    cursor: pointer;
  }

  .native-file-btn {
    display: inline-flex;
    align-items: center;
    height: 30px;
    padding: 0 10px;
    font-size: 11px;
    font-weight: 500;
    border-radius: 6px;
    border: 1px solid var(--border, #d1d5db);
    background: var(--bg, #ffffff);
    color: var(--text, #111827);
    cursor: pointer;
  }
  .native-file-btn:hover {
    background-color: var(--bg-subtle, #f9fafb);
  }
</style>
