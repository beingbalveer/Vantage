<script lang="ts">
  import { settingsOpen, addToast } from "../../stores/ui";
  import { settings, updateSettings } from "../../stores/settings";
  import { repos, activeRepoId, refreshSingleStatus, loadRepos } from "../../stores/repos";
  import * as api from "../../api";
  import type { Settings, BackupInfo } from "../../types";
  import { formatRelativeTime, formatFullTimestamp } from "../../format";
  import { Modal, Button, Checkbox, Badge, Input } from "../common";
  import { checkForAppUpdates, downloadAndInstallUpdate } from "../../updater";
  import type { Update } from "@tauri-apps/plugin-updater";

  let activeSection = $state<"general" | "git" | "terminal" | "integrations" | "recovery" | "about">("general");

  let isCheckingUpdates = $state(false);
  let availableUpdate = $state<Update | null>(null);
  let isInstallingUpdate = $state(false);

  let currentSettings = $derived($settings);

  // Recovery state
  let selectedRepoId = $state<string | null>(null);
  let backups = $state<BackupInfo[]>([]);
  let isLoadingBackups = $state(false);
  let restoringRef = $state<string | null>(null);
  let recoveryError = $state<string | null>(null);

  $effect(() => {
    if (!selectedRepoId) {
      if ($activeRepoId) {
        selectedRepoId = $activeRepoId;
      } else if ($repos.length > 0) {
        selectedRepoId = $repos[0].id;
      }
    }
  });

  $effect(() => {
    if (activeSection === "recovery" && selectedRepoId) {
      loadBackupsForRepo(selectedRepoId);
    }
  });

  async function loadBackupsForRepo(repoId: string) {
    isLoadingBackups = true;
    recoveryError = null;
    try {
      backups = await api.backupList(repoId);
    } catch (e: any) {
      recoveryError = e.message || "Failed to load backups";
      backups = [];
    } finally {
      isLoadingBackups = false;
    }
  }

  async function handleRestoreBackup(ref: string) {
    if (!selectedRepoId || restoringRef) return;
    restoringRef = ref;
    try {
      await api.backupRestore(selectedRepoId, ref);
      addToast({
        message: `Restored backup ${ref}`,
        kind: "success",
      });
      await refreshSingleStatus(selectedRepoId);
      await loadRepos();
      await loadBackupsForRepo(selectedRepoId);
    } catch (e: any) {
      const msg = e.message || "Failed to restore backup";
      addToast({
        message: msg,
        kind: "error",
      });
    } finally {
      restoringRef = null;
    }
  }

  function setSection(sec: "general" | "git" | "terminal" | "integrations" | "recovery" | "about") {
    activeSection = sec;
  }

  function handleSaveField<K extends keyof Settings>(key: K, value: Settings[K]) {
    updateSettings({ ...currentSettings, [key]: value });
  }
</script>

{#if $settingsOpen}
  <Modal
    open={$settingsOpen}
    title="Settings"
    width="720px"
    bodyPadding={false}
    onclose={() => ($settingsOpen = false)}
  >
    <div class="settings-layout">
      <!-- Left nav -->
      <div class="settings-nav">
        <button
          type="button"
          class="nav-item {activeSection === 'general' ? 'active' : ''}"
          onclick={() => setSection("general")}
        >
          General
        </button>
        <button
          type="button"
          class="nav-item {activeSection === 'git' ? 'active' : ''}"
          onclick={() => setSection("git")}
        >
          Git
        </button>
        <button
          type="button"
          class="nav-item {activeSection === 'terminal' ? 'active' : ''}"
          onclick={() => setSection("terminal")}
        >
          Terminal
        </button>
        <button
          type="button"
          class="nav-item {activeSection === 'integrations' ? 'active' : ''}"
          onclick={() => setSection("integrations")}
        >
          Integrations
        </button>
        <button
          type="button"
          class="nav-item {activeSection === 'recovery' ? 'active' : ''}"
          onclick={() => setSection("recovery")}
        >
          Recovery
        </button>
        <button
          type="button"
          class="nav-item {activeSection === 'about' ? 'active' : ''}"
          onclick={() => setSection("about")}
        >
          About
        </button>
      </div>

      <!-- Right content -->
      <div class="settings-content">
        {#if activeSection === "general"}
          <h3 class="text-strong">General Settings</h3>
          <div class="setting-row">
            <span class="setting-title text-strong">Theme</span>
            <div class="theme-select">
              <button
                type="button"
                class="theme-btn {currentSettings.theme === 'system' ? 'active' : ''}"
                onclick={() => handleSaveField("theme", "system")}
              >
                System
              </button>
              <button
                type="button"
                class="theme-btn {currentSettings.theme === 'light' ? 'active' : ''}"
                onclick={() => handleSaveField("theme", "light")}
              >
                Light
              </button>
              <button
                type="button"
                class="theme-btn {currentSettings.theme === 'dark' ? 'active' : ''}"
                onclick={() => handleSaveField("theme", "dark")}
              >
                Dark
              </button>
            </div>
          </div>

        {:else if activeSection === "git"}
          <h3 class="text-strong">Git Preferences</h3>
          <div class="setting-row">
            <span class="setting-title text-strong">Context lines</span>
            <div class="setting-num-wrap">
              <Input
                type="number"
                min="1"
                max="20"
                size="sm"
                value={String(currentSettings.contextLines)}
                onchange={(e: Event) => handleSaveField("contextLines", parseInt((e.target as HTMLInputElement).value) || 3)}
              />
            </div>
          </div>

          <div class="setting-row">
            <span class="setting-title text-strong">Auto-fetch interval (minutes)</span>
            <div class="setting-num-wrap">
              <Input
                type="number"
                min="0"
                max="60"
                size="sm"
                value={String(currentSettings.autoFetchMinutes)}
                onchange={(e: Event) => handleSaveField("autoFetchMinutes", parseInt((e.target as HTMLInputElement).value) || 0)}
              />
            </div>
          </div>

          <div class="setting-row">
            <Checkbox
              checked={currentSettings.ignoreWhitespace}
              onchange={(checked: boolean) => handleSaveField("ignoreWhitespace", checked)}
              label="Ignore whitespace changes"
            />
          </div>
        {:else if activeSection === "terminal"}
          <h3 class="text-strong">Terminal Preferences</h3>
          <div class="setting-row col">
            <span class="setting-title text-strong">Custom shell path</span>
            <Input
              type="text"
              placeholder="e.g. /bin/zsh or pwsh.exe (leave empty for default)"
              size="sm"
              value={currentSettings.shell || ""}
              onchange={(e: Event) => handleSaveField("shell", (e.target as HTMLInputElement).value || null)}
            />
          </div>
        {:else if activeSection === "integrations"}
          <h3 class="text-strong">External Integrations</h3>
          <div class="setting-row col">
            <span class="setting-title text-strong">External editor command</span>
            <span class="setting-desc text-sm text-muted">Placeholders: &#123;repo&#125; &#123;file&#125; &#123;line&#125;. Example: code -g &#123;repo&#125;/&#123;file&#125;:&#123;line&#125;</span>
            <Input
              type="text"
              placeholder={'code -g {repo}/{file}:{line}'}
              size="sm"
              value={currentSettings.externalEditorCmd || ""}
              onchange={(e: Event) => handleSaveField("externalEditorCmd", (e.target as HTMLInputElement).value || null)}
            />
          </div>

          <div class="setting-row col">
            <span class="setting-title text-strong">Commit message command</span>
            <span class="setting-desc text-sm text-muted">Receives diff on stdin and writes summary on stdout.</span>
            <Input
              type="text"
              placeholder="e.g. aigitcommit"
              size="sm"
              value={currentSettings.commitMessageCmd || ""}
              onchange={(e: Event) => handleSaveField("commitMessageCmd", (e.target as HTMLInputElement).value || null)}
            />
          </div>
        {:else if activeSection === "recovery"}
          <div class="recovery-header">
            <h3 class="text-strong">Recovery & Backups</h3>
            <p class="text-sm text-muted">
              Restore uncommitted changes or dropped commits from automatic backups saved during discard or reset operations. Backups are stored in <code>refs/vantage/backup/*</code> for 30 days.
            </p>
          </div>

          <div class="recovery-controls">
            <div class="repo-picker-group">
              <label for="recovery-repo-select" class="text-sm text-strong">Repository</label>
              <select
                id="recovery-repo-select"
                bind:value={selectedRepoId}
                class="setting-select text-sm"
                onchange={(e) => {
                  const val = (e.target as HTMLSelectElement).value;
                  selectedRepoId = val;
                  loadBackupsForRepo(val);
                }}
              >
                {#each $repos as r (r.id)}
                  <option value={r.id}>{r.name} {r.branch ? `(${r.branch})` : ""}</option>
                {/each}
              </select>
            </div>

            <Button
              variant="default"
              size="sm"
              onclick={() => selectedRepoId && loadBackupsForRepo(selectedRepoId)}
              disabled={isLoadingBackups || !selectedRepoId}
              title="Refresh backup list"
            >
              ↻ Refresh
            </Button>
          </div>

          {#if recoveryError}
            <div class="error-banner text-sm" role="alert">
              {recoveryError}
            </div>
          {/if}

          {#if isLoadingBackups}
            <div class="loading-state text-sm text-muted">
              Loading backup references…
            </div>
          {:else if backups.length === 0}
            <div class="empty-recovery text-sm text-muted">
              Nothing to recover. Backups appear here when you reset or discard.
            </div>
          {:else}
            <div class="backups-list">
              {#each backups as b (b.ref)}
                {@const kindName = b.kind || (b.ref.includes("reset") ? "reset" : "discard")}
                <div class="backup-card">
                  <div class="backup-info">
                    <div class="backup-top">
                      <Badge variant={kindName === "reset" ? "warning" : "accent"} size="sm">
                        {kindName}
                      </Badge>
                      <span class="backup-time text-xs font-mono text-muted" title={formatFullTimestamp(b.createdAt)}>
                        {formatRelativeTime(b.createdAt)}
                      </span>
                    </div>
                    <div class="backup-ref font-mono text-xs">{b.ref}</div>
                    {#if b.description}
                      <div class="backup-desc text-xs text-muted">{b.description}</div>
                    {/if}
                  </div>

                  <Button
                    variant="subtle"
                    size="sm"
                    onclick={() => handleRestoreBackup(b.ref)}
                    loading={restoringRef === b.ref}
                    disabled={restoringRef === b.ref}
                  >
                    {restoringRef === b.ref ? "Restoring…" : "Restore"}
                  </Button>
                </div>
              {/each}
            </div>
          {/if}
        {:else}
          <div style="display: flex; align-items: center; gap: 14px; margin-bottom: 12px;">
            <img src="/favicon.png" alt="Vantage" width="36" height="36" style="border-radius: 8px;" />
            <div>
              <h3 class="text-strong" style="margin: 0;">Vantage</h3>
              <p class="text-xs text-muted" style="margin: 0;">v0.1.1</p>
            </div>
          </div>
          <p class="text-sm text-muted">A fast, local, multi-repo Git desktop client, read-only code viewer, and terminal workspace.</p>

          <div style="margin-top: 24px; padding-top: 16px; border-top: 1px solid var(--border); display: flex; flex-direction: column; gap: 12px; align-items: flex-start;">
            <div style="display: flex; gap: 10px; align-items: center;">
              <Button
                variant="primary"
                size="sm"
                loading={isCheckingUpdates}
                disabled={isCheckingUpdates || isInstallingUpdate}
                onclick={async () => {
                  isCheckingUpdates = true;
                  try {
                    availableUpdate = await checkForAppUpdates(true);
                  } finally {
                    isCheckingUpdates = false;
                  }
                }}
              >
                {isCheckingUpdates ? "Checking for updates…" : "Check for updates"}
              </Button>

              {#if availableUpdate}
                <Button
                  variant="primary"
                  size="sm"
                  loading={isInstallingUpdate}
                  disabled={isInstallingUpdate}
                  onclick={async () => {
                    isInstallingUpdate = true;
                    try {
                      await downloadAndInstallUpdate(availableUpdate);
                    } finally {
                      isInstallingUpdate = false;
                    }
                  }}
                >
                  {isInstallingUpdate ? "Installing & Restarting…" : `Install v${availableUpdate.version}`}
                </Button>
              {/if}
            </div>

            {#if availableUpdate}
              <div class="text-xs text-muted">
                Version <strong>v{availableUpdate.version}</strong> is ready to download and install.
              </div>
            {/if}
          </div>
        {/if}
      </div>
    </div>
  </Modal>
{/if}

<style>
  .settings-layout {
    display: flex;
    height: 480px;
    max-height: 70vh;
    overflow: hidden;
  }

  .settings-nav {
    width: 160px;
    background-color: var(--bg-subtle);
    border-right: 1px solid var(--border);
    padding: 16px 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex-shrink: 0;
  }

  .nav-item {
    height: 32px;
    text-align: left;
    background: transparent;
    border: none;
    border-radius: var(--r-md);
    padding: 0 10px;
    font-size: 13px;
    font-weight: 500;
    color: var(--text-muted);
    cursor: pointer;
  }
  .nav-item:hover {
    background-color: var(--bg-muted);
    color: var(--text);
  }
  .nav-item.active {
    background-color: var(--bg);
    color: var(--text);
    font-weight: 600;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.05);
  }

  .settings-content {
    flex: 1;
    padding: 24px 32px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .setting-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 0;
    border-bottom: 1px solid var(--border-subtle);
  }
  .setting-row.col {
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
  }

  .setting-title {
    color: var(--text);
    font-size: 13px;
  }

  .setting-num-wrap {
    width: 90px;
  }

  .theme-select {
    display: flex;
    background-color: var(--bg-muted);
    border-radius: var(--r-md);
    padding: 2px;
    gap: 2px;
  }

  .theme-btn {
    height: 24px;
    padding: 0 10px;
    border: none;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--text-muted);
    font-size: 12px;
    cursor: pointer;
  }
  .theme-btn.active {
    background-color: var(--bg);
    color: var(--text);
    font-weight: 600;
  }

  /* Recovery section styles */
  .recovery-header {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .recovery-header p {
    margin: 0;
    line-height: 18px;
  }

  .recovery-controls {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 12px;
    padding-bottom: 8px;
    border-bottom: 1px solid var(--border-subtle);
  }

  .repo-picker-group {
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1;
    max-width: 320px;
  }

  .setting-select {
    height: 28px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    background-color: var(--bg);
    color: var(--text);
    outline: none;
  }
  .setting-select:focus {
    border-color: var(--accent);
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .loading-state,
  .empty-recovery {
    padding: 32px 16px;
    text-align: center;
  }

  .backups-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .backup-card {
    background-color: var(--bg-subtle);
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-md);
    padding: 10px 12px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .backup-info {
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1;
    overflow: hidden;
  }

  .backup-top {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .backup-ref {
    color: var(--text);
    word-break: break-all;
  }

  .backup-desc {
    color: var(--text-muted);
  }

  .error-banner {
    padding: 8px 12px;
    background-color: var(--danger-subtle, rgba(218, 54, 51, 0.1));
    border: 1px solid var(--danger);
    border-radius: var(--r-md);
    color: var(--danger);
  }
</style>
