<script lang="ts">
  import { settingsOpen, addToast, settingsSection, settingsRecoveryRepoId, type SettingsSection } from "../../app/ui";
  import { settings, updateSettings } from "../../domain/stores/settings";
  import { FEATURE_METADATA, isFeatureEnabled, setFeatureFlag, resetFeatureFlags, type FeatureFlagKey } from "../../config/featureFlags.svelte";
  import { repos, activeRepoId, refreshSingleStatus, loadRepos } from "../../domain/stores/repos";
  import * as api from "../../platform/api";
  import * as settingsLib from "../../domain/settings";
  import type { Settings, BackupInfo, EnvInfo, GitIdentity } from "../../platform/types";
  import { formatRelativeTime, formatFullTimestamp } from "../../domain/format";
  import { Modal, Button, Checkbox, Badge, Input, Icon } from "../common";
  import { checkForAppUpdates, downloadAndInstallUpdate } from "../../platform/updater";
  import type { Update } from "@tauri-apps/plugin-updater";

  let activeSection = $state<SettingsSection>("general");
  let flagsTick = $state(0);
  let appVersion = $state("0.4.0");

  $effect(() => {
    api
      .appVersion()
      .then((version) => (appVersion = version))
      .catch(() => {});
  });

  let isCheckingUpdates = $state(false);
  let availableUpdate = $state<Update | null>(null);
  let isInstallingUpdate = $state(false);

  let currentSettings = $derived($settings);
  let activeRepo = $derived($repos.find((r) => r.id === $activeRepoId) || $repos[0] || null);
  // Preview while dragging the UI-scale slider; the setting is only persisted
  // on release so a drag doesn't trigger a write (and a theme reflow) per step.
  let uiScalePreview = $state<number | null>(null);

  // Environment detection (real editors / shells)
  let envInfo = $state<EnvInfo | null>(null);
  let envError = $state<string | null>(null);
  let detectedShells = $derived(envInfo?.shells ?? []);
  let detectedEditors = $derived(envInfo?.editors ?? []);

  // Git identity
  let identity = $state<GitIdentity | null>(null);
  let identityLoading = $state(false);
  let identityError = $state<string | null>(null);
  let savingIdentity = $state<"repo" | "global" | null>(null);
  let repoNameInput = $state("");
  let repoEmailInput = $state("");
  let globalNameInput = $state("");
  let globalEmailInput = $state("");
  let repoNameError = $state<string | null>(null);
  let repoEmailError = $state<string | null>(null);
  let globalNameError = $state<string | null>(null);
  let globalEmailError = $state<string | null>(null);

  // Default branch draft so invalid text can be shown without saving
  let defaultBranchDraft = $state("main");
  let defaultBranchError = $state<string | null>(null);

  // Recovery state
  let selectedRepoId = $state<string | null>(null);
  let backups = $state<BackupInfo[]>([]);
  let isLoadingBackups = $state(false);
  let restoringRef = $state<string | null>(null);
  let recoveryError = $state<string | null>(null);

  $effect(() => {
    // Reveal the section the caller asked for when the dialog opens.
    if ($settingsOpen) {
      activeSection = $settingsSection;
    }
  });

  $effect(() => {
    const preferred = $settingsRecoveryRepoId;
    if (preferred && $repos.some((r) => r.id === preferred)) {
      selectedRepoId = preferred;
    } else if (!selectedRepoId) {
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

  $effect(() => {
    if ($settingsOpen && !envInfo && !envError) {
      loadEnvironment();
    }
  });

  $effect(() => {
    if ($settingsOpen && activeSection === "git") {
      loadIdentity(activeRepo?.id ?? null);
    }
  });

  $effect(() => {
    defaultBranchDraft = currentSettings.defaultBranch;
  });

  let shellSelectValue = $derived(
    !currentSettings.shell
      ? ""
      : detectedShells.some((s) => s.command === currentSettings.shell)
        ? currentSettings.shell
        : "__custom"
  );

  let editorSelectValue = $derived(
    !currentSettings.externalEditorCmd
      ? ""
      : detectedEditors.find((e) => e.command === currentSettings.externalEditorCmd)?.id || "__custom"
  );

  async function loadEnvironment() {
    try {
      envInfo = await api.envCheck();
    } catch (e: any) {
      envError = e?.message || "Failed to detect installed editors and shells.";
    }
  }

  async function loadIdentity(repoId: string | null) {
    identityLoading = true;
    identityError = null;
    try {
      identity = await api.gitIdentityGet(repoId || undefined);
      repoNameInput = identity.localName || "";
      repoEmailInput = identity.localEmail || "";
      globalNameInput = identity.globalName || "";
      globalEmailInput = identity.globalEmail || "";
      repoNameError = null;
      repoEmailError = null;
      globalNameError = null;
      globalEmailError = null;
    } catch (e: any) {
      identityError = e?.message || "Failed to read Git identity.";
      identity = null;
    } finally {
      identityLoading = false;
    }
  }

  function validateIdentityInputs(scope: "repo" | "global"): boolean {
    if (scope === "repo") {
      repoNameError = settingsLib.validateIdentityName(repoNameInput);
      repoEmailError = settingsLib.validateIdentityEmail(repoEmailInput);
      return !repoNameError && !repoEmailError;
    }
    globalNameError = settingsLib.validateIdentityName(globalNameInput);
    globalEmailError = settingsLib.validateIdentityEmail(globalEmailInput);
    return !globalNameError && !globalEmailError;
  }

  async function saveIdentity(scope: "repo" | "global") {
    if (scope === "repo" && !activeRepo) return;
    if (!validateIdentityInputs(scope)) return;

    savingIdentity = scope;
    try {
      const result = await api.gitIdentitySet(
        scope === "repo" ? activeRepo?.id : undefined,
        scope,
        (scope === "repo" ? repoNameInput : globalNameInput).trim(),
        (scope === "repo" ? repoEmailInput : globalEmailInput).trim()
      );
      identity = result;
      addToast({
        message: scope === "repo" ? "Repository identity saved" : "Global Git identity saved",
        kind: "success",
      });
      await loadIdentity(activeRepo?.id ?? null);
    } catch (e: any) {
      addToast({ message: e?.message || "Failed to save Git identity", kind: "error" });
    } finally {
      savingIdentity = null;
    }
  }

  async function clearRepoIdentity() {
    if (!activeRepo) return;
    savingIdentity = "repo";
    try {
      identity = await api.gitIdentitySet(activeRepo.id, "repo", null, null);
      addToast({ message: "Repository identity cleared — using global identity", kind: "info" });
      await loadIdentity(activeRepo.id);
    } catch (e: any) {
      addToast({ message: e?.message || "Failed to clear repository identity", kind: "error" });
    } finally {
      savingIdentity = null;
    }
  }

  function handleDefaultBranch(value: string) {
    defaultBranchDraft = value;
    const err = settingsLib.validateBranchName(value);
    defaultBranchError = err;
    if (!err) {
      updateSettings({ ...currentSettings, defaultBranch: value.trim() });
    }
  }

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

  function setSection(sec: SettingsSection) {
    activeSection = sec;
  }

  function handleSaveField<K extends keyof Settings>(key: K, value: Settings[K]) {
    updateSettings(settingsLib.normalizeSettings({ ...currentSettings, [key]: value }));
  }

  function sourceLabel(source: GitIdentity["nameSource"]): string {
    switch (source) {
      case "repo":
        return "Repository";
      case "global":
        return "Global";
      case "system":
        return "System";
      default:
        return "Not set";
    }
  }
</script>

{#if $settingsOpen}
  <Modal
    open={$settingsOpen}
    title="Settings"
    width="720px"
    bodyPadding={false}
    onclose={() => {
      $settingsOpen = false;
      $settingsSection = "general";
      $settingsRecoveryRepoId = null;
    }}
  >
    <div class="settings-layout">
      <!-- Left nav -->
      <div class="settings-nav">
        <button
          type="button"
          class="nav-item {activeSection === 'general' ? 'active' : ''}"
          aria-current={activeSection === "general" ? "page" : undefined}
          onclick={() => setSection("general")}
        >
          General
        </button>
        <button
          type="button"
          class="nav-item {activeSection === 'git' ? 'active' : ''}"
          aria-current={activeSection === "git" ? "page" : undefined}
          onclick={() => setSection("git")}
        >
          Git
        </button>
        <button
          type="button"
          class="nav-item {activeSection === 'terminal' ? 'active' : ''}"
          aria-current={activeSection === "terminal" ? "page" : undefined}
          onclick={() => setSection("terminal")}
        >
          Terminal
        </button>
        <button
          type="button"
          class="nav-item {activeSection === 'integrations' ? 'active' : ''}"
          aria-current={activeSection === "integrations" ? "page" : undefined}
          onclick={() => setSection("integrations")}
        >
          Integrations
        </button>
        <button
          type="button"
          class="nav-item {activeSection === 'recovery' ? 'active' : ''}"
          aria-current={activeSection === "recovery" ? "page" : undefined}
          onclick={() => setSection("recovery")}
        >
          Recovery
        </button>
        <button
          type="button"
          class="nav-item {activeSection === 'about' ? 'active' : ''}"
          aria-current={activeSection === "about" ? "page" : undefined}
          onclick={() => setSection("about")}
        >
          About
        </button>
        <button
          type="button"
          class="nav-item {activeSection === 'flags' ? 'active' : ''}"
          onclick={() => setSection("flags")}
        >
          Feature Flags
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
                aria-pressed={currentSettings.theme === "system"}
                onclick={() => handleSaveField("theme", "system")}
              >
                System
              </button>
              <button
                type="button"
                class="theme-btn {currentSettings.theme === 'light' ? 'active' : ''}"
                aria-pressed={currentSettings.theme === "light"}
                onclick={() => handleSaveField("theme", "light")}
              >
                Light
              </button>
              <button
                type="button"
                class="theme-btn {currentSettings.theme === 'dark' ? 'active' : ''}"
                aria-pressed={currentSettings.theme === "dark"}
                onclick={() => handleSaveField("theme", "dark")}
              >
                Dark
              </button>
            </div>
          </div>

          <div class="setting-row">
            <span class="setting-title text-strong">UI scale</span>
            <div class="scale-wrap">
              <input
                type="range"
                min="75"
                max="150"
                step="5"
                value={Math.round((uiScalePreview ?? currentSettings.uiScale) * 100)}
                oninput={(e) => {
                  uiScalePreview =
                    parseInt((e.target as HTMLInputElement).value, 10) / 100;
                }}
                onchange={(e) => {
                  const value =
                    parseInt((e.target as HTMLInputElement).value, 10) / 100;
                  uiScalePreview = null;
                  handleSaveField("uiScale", value);
                }}
                aria-label="UI scale"
              />
              <span class="text-xs text-muted font-mono">
                {Math.round((uiScalePreview ?? currentSettings.uiScale) * 100)}%
              </span>
            </div>
          </div>

          <div class="setting-row">
            <span class="setting-title text-strong">Tab size</span>
            <div class="setting-num-wrap">
              <Input
                type="number"
                min="1"
                max="8"
                size="sm"
                value={String(currentSettings.tabSize)}
                onchange={(e: Event) =>
                  handleSaveField(
                    "tabSize",
                    parseInt((e.target as HTMLInputElement).value, 10) || 4
                  )}
              />
            </div>
          </div>
          <p class="setting-desc text-xs text-muted">
            Indentation width used by the diff viewer and the read-only code viewer.
          </p>
        {:else if activeSection === "git"}
          <h3 class="text-strong">Git Preferences</h3>

          <div class="identity-card">
            <div class="identity-card-header">
              <span class="setting-title text-strong">Git identity</span>
              {#if identityLoading}
                <span class="text-xs text-muted">Reading git config…</span>
              {/if}
            </div>

            {#if identityError}
              <div class="error-banner text-sm" role="alert">{identityError}</div>
            {:else if identity}
              <div class="identity-effective">
                <span class="identity-name font-mono text-xs">
                  {identity.effectiveName || "Name not set"}
                </span>
                <span class="identity-email font-mono text-xs text-muted">
                  &lt;{identity.effectiveEmail || "email not set"}&gt;
                </span>
                <Badge variant={identity.nameSource === "unset" ? "warning" : "neutral"}>
                  name: {sourceLabel(identity.nameSource)}
                </Badge>
                <Badge variant={identity.emailSource === "unset" ? "warning" : "neutral"}>
                  email: {sourceLabel(identity.emailSource)}
                </Badge>
              </div>
              {#if !identity.effectiveName || !identity.effectiveEmail}
                <p class="text-xs text-muted identity-warning">
                  Commits are blocked until both values are configured. Set them below.
                </p>
              {/if}
            {/if}

            {#if activeRepo}
              <div class="identity-group">
                <div class="identity-group-title text-sm text-strong">
                  Repository override · <span class="font-mono">{activeRepo.name}</span>
                </div>
                <Input
                  type="text"
                  size="sm"
                  placeholder={identity?.globalName ? `Global: ${identity.globalName}` : "Your name"}
                  bind:value={repoNameInput}
                  error={repoNameError || false}
                />
                {#if repoNameError}<span class="field-error">{repoNameError}</span>{/if}
                <Input
                  type="email"
                  size="sm"
                  placeholder={identity?.globalEmail ? `Global: ${identity.globalEmail}` : "you@example.com"}
                  bind:value={repoEmailInput}
                  error={repoEmailError || false}
                />
                {#if repoEmailError}<span class="field-error">{repoEmailError}</span>{/if}
                <div class="identity-actions">
                  <Button
                    variant="default"
                    size="sm"
                    loading={savingIdentity === "repo"}
                    disabled={savingIdentity !== null}
                    onclick={() => saveIdentity("repo")}
                  >
                    Save for this repository
                  </Button>
                  <Button
                    variant="subtle"
                    size="sm"
                    disabled={savingIdentity !== null || (!identity?.localName && !identity?.localEmail)}
                    onclick={clearRepoIdentity}
                  >
                    Use global identity
                  </Button>
                </div>
              </div>
            {/if}

            <div class="identity-group">
              <div class="identity-group-title text-sm text-strong">Global identity</div>
              <Input
                type="text"
                size="sm"
                placeholder="Your name"
                bind:value={globalNameInput}
                error={globalNameError || false}
              />
              {#if globalNameError}<span class="field-error">{globalNameError}</span>{/if}
              <Input
                type="email"
                size="sm"
                placeholder="you@example.com"
                bind:value={globalEmailInput}
                error={globalEmailError || false}
              />
              {#if globalEmailError}<span class="field-error">{globalEmailError}</span>{/if}
              <div class="identity-actions">
                <Button
                  variant="default"
                  size="sm"
                  loading={savingIdentity === "global"}
                  disabled={savingIdentity !== null}
                  onclick={() => saveIdentity("global")}
                >
                  Save globally
                </Button>
              </div>
            </div>
          </div>

          <div class="setting-row col">
            <span class="setting-title text-strong">Default branch</span>
            <span class="setting-desc text-sm text-muted">
              Used when adding an empty repository (no commits yet) so its first branch view has
              the expected name. Normal repositories keep their checked-out branch.
            </span>
            <Input
              type="text"
              size="sm"
              value={defaultBranchDraft}
              error={defaultBranchError || false}
              onchange={(e: Event) => handleDefaultBranch((e.target as HTMLInputElement).value)}
            />
            {#if defaultBranchError}<span class="field-error">{defaultBranchError}</span>{/if}
          </div>

          <div class="setting-row">
            <span class="setting-title text-strong">Context lines</span>
            <div class="setting-num-wrap">
              <Input
                type="number"
                min="0"
                max="20"
                size="sm"
                ariaLabel="Context lines"
                value={String(currentSettings.contextLines)}
                onchange={(e: Event) => handleSaveField("contextLines", parseInt((e.target as HTMLInputElement).value) || 0)}
              />
            </div>
          </div>

          <div class="setting-row">
            <span class="setting-title text-strong">Auto-fetch interval</span>
            <select
              class="setting-select text-sm auto-fetch-select"
              aria-label="Auto-fetch interval"
              value={String(currentSettings.autoFetchMinutes)}
              onchange={(e) =>
                handleSaveField(
                  "autoFetchMinutes",
                  parseInt((e.target as HTMLSelectElement).value, 10) || 0
                )}
            >
              <option value="0">Off</option>
              <option value="1">Every minute</option>
              <option value="5">Every 5 minutes</option>
              <option value="15">Every 15 minutes</option>
              <option value="30">Every 30 minutes</option>
              <option value="60">Every hour</option>
              {#if ![0, 1, 5, 15, 30, 60].includes(currentSettings.autoFetchMinutes)}
                <option value={String(currentSettings.autoFetchMinutes)}>
                  Every {currentSettings.autoFetchMinutes} minutes
                </option>
              {/if}
            </select>
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
            <span class="setting-title text-strong">Shell</span>
            <span class="setting-desc text-sm text-muted">
              Detected from <code>/etc/shells</code> and your PATH. "Default" uses
              {envInfo?.defaultShell || "$SHELL"}.
            </span>
            {#if envError}
              <span class="field-error">{envError}</span>
            {/if}
            <select
              class="setting-select text-sm"
              value={shellSelectValue}
              onchange={(e) => {
                const value = (e.target as HTMLSelectElement).value;
                if (value === "__custom") return;
                handleSaveField("shell", value || null);
              }}
            >
              <option value="">Default ({envInfo?.defaultShell || "detected shell"})</option>
              {#each detectedShells as shell (shell.id)}
                <option value={shell.command}>{shell.label} — {shell.command}</option>
              {/each}
              <option value="__custom">Custom path…</option>
            </select>
            <Input
              type="text"
              ariaLabel="Custom shell path"
              placeholder={envInfo?.defaultShell || "e.g. /bin/zsh"}
              size="sm"
              value={currentSettings.shell || ""}
              onchange={(e: Event) =>
                handleSaveField("shell", (e.target as HTMLInputElement).value || null)}
            />
          </div>

          <div class="setting-row">
            <span class="setting-title text-strong">Font size</span>
            <div class="setting-num-wrap">
              <Input
                type="number"
                min="10"
                max="20"
                size="sm"
                value={String(currentSettings.terminalFontSize)}
                onchange={(e: Event) =>
                  handleSaveField(
                    "terminalFontSize",
                    parseInt((e.target as HTMLInputElement).value) || 13
                  )}
              />
            </div>
          </div>

          <div class="setting-row col">
            <span class="setting-title text-strong">Prompts</span>
            <span class="setting-desc text-sm text-muted">
              Hiding a terminal keeps its session alive. Ask before restarting a shell that is still running.
            </span>
            <Checkbox
              checked={currentSettings.confirmTerminalRestart}
              onchange={(checked: boolean) => handleSaveField("confirmTerminalRestart", checked)}
              label="Confirm before restarting the shell"
            />
          </div>
        {:else if activeSection === "integrations"}
          <h3 class="text-strong">External Integrations</h3>

          <div class="setting-row col">
            <span class="setting-title text-strong">External editor</span>
            <span class="setting-desc text-sm text-muted">
              Detected editors only. "System default app" always works as a fallback.
            </span>
            {#if envError}
              <span class="field-error">{envError}</span>
            {/if}
            <select
              class="setting-select text-sm"
              value={editorSelectValue}
              onchange={(e) => {
                const id = (e.target as HTMLSelectElement).value;
                if (!id) {
                  handleSaveField("externalEditorCmd", null);
                } else if (id !== "__custom") {
                  const editor = detectedEditors.find((candidate) => candidate.id === id);
                  if (editor) handleSaveField("externalEditorCmd", editor.command);
                }
              }}
            >
              <option value="">System default app</option>
              {#each detectedEditors.filter((editor) => editor.id !== "system") as editor (editor.id)}
                <option value={editor.id}>{editor.label}</option>
              {/each}
              <option value="__custom">Custom command…</option>
            </select>
            <Input
              type="text"
              ariaLabel="External editor command"
              placeholder={'code -g {repo}/{file}:{line}'}
              size="sm"
              value={currentSettings.externalEditorCmd || ""}
              onchange={(e: Event) =>
                handleSaveField("externalEditorCmd", (e.target as HTMLInputElement).value || null)}
            />
            <span class="setting-desc text-xs text-muted">
              Placeholders: &#123;repo&#125; &#123;file&#125; &#123;line&#125; &#123;full&#125;. Example: code -g &#123;repo&#125;/&#123;file&#125;:&#123;line&#125;
            </span>
          </div>

          <div class="setting-row col">
            <span class="setting-title text-strong">Commit message command</span>
            <span class="setting-desc text-sm text-muted">Receives diff on stdin and writes summary on stdout.</span>
            <Input
              type="text"
              ariaLabel="Commit message command"
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
              Restore uncommitted changes or dropped commits from automatic backups saved during discard, reset, amend, or undo operations. Backups are stored in <code>refs/vantage/backup/*</code> for 30 days.
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
              class="icon-only"
              onclick={() => selectedRepoId && loadBackupsForRepo(selectedRepoId)}
              disabled={isLoadingBackups || !selectedRepoId}
              title="Refresh backup list"
              ariaLabel="Refresh backup list"
            >
              <Icon name="refresh" size={13} />
            </Button>
          </div>

          {#if recoveryError}
            <div class="error-banner text-sm" role="alert">
              {recoveryError}
            </div>
          {/if}

          {#if isLoadingBackups}
            <div class="loading-state text-sm text-muted" role="status">
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
            <img src="/favicon.png" alt="Vantage" width="36" height="36" />
            <div>
              <h3 class="text-strong" style="margin: 0;">Vantage</h3>
              <p class="text-xs text-muted" style="margin: 0;">v{appVersion}</p>
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

        {#if activeSection === "flags"}
          <div class="flags-header">
            <div>
              <h3 class="text-strong" style="margin: 0 0 4px 0;">Feature Flags</h3>
              <p class="text-xs text-muted" style="margin: 0;">
                Enable or disable individual merged tasks at runtime. Changes persist across restarts.
              </p>
            </div>
            <Button
              variant="default"
              size="sm"
              onclick={() => {
                resetFeatureFlags();
                flagsTick++;
                addToast({ message: "Feature flags reset to defaults", kind: "info" });
              }}
            >
              Reset to Defaults
            </Button>
          </div>

          <div class="flags-list">
            {#key flagsTick}
              {#each FEATURE_METADATA as flag}
                <div class="flag-item">
                  <div class="flag-meta">
                    <div class="flag-top">
                      <span class="flag-name text-strong">{flag.name}</span>
                      <span class="flag-ticket">{flag.ticket}</span>
                    </div>
                    <div class="flag-desc text-xs text-muted">{flag.description}</div>
                  </div>
                  <Checkbox
                    checked={isFeatureEnabled(flag.key)}
                    onchange={(checked) => {
                      setFeatureFlag(flag.key, checked);
                      flagsTick++;
                    }}
                  />
                </div>
              {/each}
            {/key}
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

  .scale-wrap {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 190px;
  }
  .scale-wrap input[type="range"] {
    flex: 1;
    accent-color: var(--accent);
  }

  .identity-card {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 12px;
    background-color: var(--bg-subtle);
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-md);
  }

  .identity-card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .identity-effective {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    padding: 6px 8px;
    background-color: var(--bg);
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-sm);
  }

  .identity-name {
    color: var(--text);
    font-weight: 600;
  }

  .identity-warning {
    margin: -4px 0 0;
    color: var(--warning);
  }

  .identity-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .identity-group-title {
    margin-top: 2px;
  }

  .identity-actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    margin-top: 2px;
  }

  .field-error {
    color: var(--danger);
    font-size: 11px;
    line-height: 15px;
  }

  .setting-select.auto-fetch-select {
    min-width: 160px;
  }

  .flags-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding-bottom: 8px;
    border-bottom: 1px solid var(--border-subtle);
  }

  .flags-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .flag-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 12px;
    background-color: var(--bg-subtle);
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-md);
  }

  .flag-meta {
    display: flex;
    flex-direction: column;
    gap: 3px;
    flex: 1;
  }

  .flag-top {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .flag-name {
    font-size: 13px;
    color: var(--text);
  }

  .flag-ticket {
    font-size: 11px;
    font-weight: 600;
    font-family: var(--font-mono, monospace);
    color: var(--accent);
    background-color: var(--accent-subtle, rgba(88, 166, 255, 0.1));
    padding: 1px 6px;
    border-radius: var(--r-sm);
  }

  .flag-desc {
    color: var(--text-muted);
  }
</style>
