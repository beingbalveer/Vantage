<script lang="ts">
  import { settings } from "../../domain/stores/settings";
  import * as api from "../../platform/api";
  import { addToast, openSettingsAt } from "../../app/ui";
  import { pendingCommitMessage } from "../../domain/stores/composer";
  import { registerHandler } from "../../app/commands";
  import type {
    CommitAuthor,
    GitIdentity,
    GitHubStatus,
    HeadCommitInfo,
  } from "../../platform/types";
  import { canAmendLastCommit, amendWarning } from "../../domain/commitActions";
  import { runUndoLastCommit } from "../../domain/undoCommit";
  import { validateIdentityEmail, validateIdentityName } from "../../domain/settings";
  import Modal from "../common/Modal.svelte";
  import Icon from "../common/Icon.svelte";
  import Badge from "../common/Badge.svelte";
  import Button from "../common/Button.svelte";
  import {
    addCoAuthor,
    coAuthorEmail,
    formatCoAuthor,
    parseCoAuthor,
    type CoAuthor,
  } from "../../domain/coauthors";

  interface Props {
    repoId: string;
    repoName: string;
    branch: string;
    selectedPaths: string[];
    /** Paths with unmerged index entries; commit is blocked until resolved. */
    conflictedPaths?: string[];
    /** Bumped by the palette to open this dialog already in amend mode. */
    amendToken?: number;
    onclose?: () => void;
    oncommit?: (preserveSelection?: boolean) => void;
  }

  let {
    repoId,
    repoName,
    branch,
    selectedPaths,
    conflictedPaths = [],
    amendToken = 0,
    onclose,
    oncommit,
  }: Props = $props();

  let dialogOpen = $state(true);
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
  let seenAmendToken = $state(0);

  // Author selection. "repo" means "let Git resolve the configured identity";
  // the other two are one-off overrides applied with `git -c` for this commit.
  let identity = $state<GitIdentity | null>(null);
  let identityLoading = $state(false);
  let identityError = $state<string | null>(null);
  let authorMode = $state<"repo" | "global" | "custom">("repo");
  let customName = $state("");
  let customEmail = $state("");
  let identityTouched = $state(false);
  let authorOpen = $state(false);

  // GitHub account / permission snapshot (read-only, via the gh CLI).
  let github = $state<GitHubStatus | null>(null);
  let githubLoading = $state(false);
  let githubError = $state<string | null>(null);
  let githubOpen = $state(false);

  let hasAiCmd = $derived(!!$settings.commitMessageCmd);
  let summaryLen = $derived(summary.length);
  let hasConflicts = $derived(conflictedPaths.length > 0);
  let repoIdentityReady = $derived(
    !!identity?.localName?.trim() && !!identity?.localEmail?.trim()
  );
  let globalIdentityReady = $derived(
    !!identity?.globalName?.trim() && !!identity?.globalEmail?.trim()
  );

  let customNameError = $derived(
    customName.trim() ? validateIdentityName(customName) : "Name is required."
  );
  let customEmailError = $derived(
    customEmail.trim() ? validateIdentityEmail(customEmail) : "Email is required."
  );

  let authorReady = $derived(
    authorMode === "repo"
      ? !!identity?.effectiveName?.trim() && !!identity?.effectiveEmail?.trim()
      : authorMode === "global"
      ? globalIdentityReady
      : !customNameError && !customEmailError
  );

  /** `undefined` means "no override: Git uses the repository config". */
  let authorOverride = $derived<CommitAuthor | undefined>(
    authorMode === "global" && globalIdentityReady
      ? { name: identity!.globalName!.trim(), email: identity!.globalEmail!.trim() }
      : authorMode === "custom"
      ? { name: customName.trim(), email: customEmail.trim() }
      : undefined
  );

  let displayName = $derived(
    authorMode === "repo"
      ? identity?.effectiveName ?? ""
      : authorMode === "global"
      ? identity?.globalName ?? ""
      : customName
  );
  let displayEmail = $derived(
    authorMode === "repo"
      ? identity?.effectiveEmail ?? ""
      : authorMode === "global"
      ? identity?.globalEmail ?? ""
      : customEmail
  );
  let authorInitials = $derived(
    displayName
      .trim()
      .split(/\s+/)
      .slice(0, 2)
      .map((part) => part[0]?.toUpperCase() ?? "")
      .join("") || "?"
  );
  let authorModeLabel = $derived(
    authorMode === "repo" ? "Repository" : authorMode === "global" ? "Global" : "One-off"
  );
  let githubPermissionDetail = $derived(
    [github?.permission, github?.activeAccount && `@${github.activeAccount}`]
      .filter(Boolean)
      .join(" · ")
  );

  let githubChipLabel = $derived.by(() => {
    if (githubLoading) return "Checking GitHub…";
    if (githubError || !github) return "GitHub unavailable";
    if (!github.remoteUrl) return "No remote";
    if (!github.isGithub) return "Not GitHub";
    if (!github.ghInstalled) return "gh not installed";
    if (!github.authenticated) return "GitHub not signed in";
    const who = github.activeAccount ? `@${github.activeAccount}` : "GitHub";
    const access =
      github.canPush === true
        ? "push"
        : github.canPush === false
        ? "read-only"
        : "unknown access";
    return `${who} · ${access}`;
  });

  let githubState = $derived<"ok" | "warn" | "muted">(
    github?.authenticated
      ? github.canPush === true
        ? "ok"
        : "warn"
      : "muted"
  );

  let canCommit = $derived(
    summary.trim().length > 0 &&
      !isCommitting &&
      !hasConflicts &&
      authorReady &&
      (amendMode || selectedPaths.length > 0)
  );
  let pushedWarning = $derived(headInfo ? amendWarning(headInfo.isPushed) : null);

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

  function applyIdentityDefaults() {
    if (!identity || identityTouched) return;
    if (repoIdentityReady) {
      authorMode = "repo";
    } else if (globalIdentityReady) {
      authorMode = "global";
    } else {
      authorMode = "custom";
      customName = identity.effectiveName ?? "";
      customEmail = identity.effectiveEmail ?? "";
    }
  }

  async function loadIdentity() {
    identityLoading = true;
    identityError = null;
    try {
      identity = await api.gitIdentityGet(repoId);
      applyIdentityDefaults();
      // Surface the fields when the user has to act on them.
      if (!identity.effectiveName?.trim() || !identity.effectiveEmail?.trim()) {
        authorOpen = true;
      }
    } catch (e: any) {
      identityError = e?.message || "Could not read the Git identity.";
      identity = null;
    } finally {
      identityLoading = false;
    }
  }

  async function loadGitHub() {
    githubLoading = true;
    githubError = null;
    try {
      github = await api.githubStatus(repoId);
    } catch (e: any) {
      githubError = e?.message || "Could not read GitHub status.";
      github = null;
    } finally {
      githubLoading = false;
    }
  }

  function selectAuthorMode(mode: "repo" | "global" | "custom") {
    identityTouched = true;
    authorMode = mode;
  }

  function openGitSettings() {
    dialogOpen = false;
    onclose?.();
    openSettingsAt("git", repoId);
  }

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

  $effect(() => {
    // Runs once per mounted dialog. The component is unmounted when the pane
    // closes the dialog, so reopening always re-reads fresh state.
    void loadIdentity();
    void loadGitHub();
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
    const author = authorOverride;
    try {
      const coAuthorTrailers = coAuthors.length > 0 ? coAuthors.map(formatCoAuthor) : undefined;
      if (amendMode) {
        const c = await api.amendCommit(repoId, selectedPaths, summary.trim(), body, author);
        addToast({ message: `Amended ${c.short}`, kind: "success" });
      } else {
        const c = await api.commit(
          repoId,
          selectedPaths,
          summary.trim(),
          body,
          coAuthorTrailers,
          author
        );
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
      dialogOpen = false;
      onclose?.();
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
      // Preserve selection so the dialog stays open with the restored message.
      oncommit?.(true);
    } catch (e: any) {
      addToast({ message: e.message || "Failed to undo last commit", kind: "error" });
    } finally {
      isUndoing = false;
    }
  }

  function handleClose() {
    dialogOpen = false;
    onclose?.();
  }

  $effect(() => {
    const unregister = registerHandler(
      "commit",
      () => {
        const active = document.activeElement as HTMLElement | null;
        if (!active?.closest(".commit-dialog")) return false;
        if (!canCommit) return false;
        void handleCommit();
        return true;
      },
      "history"
    );
    return unregister;
  });
</script>

<Modal
  bind:open={dialogOpen}
  title={amendMode ? "Amend last commit" : "Commit changes"}
  description={`${repoName} · ${branch}`}
  width={520}
  onclose={handleClose}
  bodyPadding={false}
  closeOnBackdrop={false}
>
  <div class="commit-dialog">
    {#if amendMode && headInfo}
      <div class="amend-banner">
        <div class="amend-body">
          <Icon name="pencil" size={12} />
          <span class="amend-label">
            Rewriting <span class="font-mono">{headInfo.short}</span>
            <span class="amend-subject" title={headInfo.subject}>{headInfo.subject}</span>
          </span>
        </div>
        <button
          type="button"
          class="icon-btn"
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
          <Icon name="alert" size={13} />
          <span class="text-sm">{pushedWarning}</span>
        </div>
      {/if}
    {/if}

    <div class="input-wrap">
      <input
        id="commit-summary"
        type="text"
        placeholder="Commit summary"
        aria-label="Commit summary"
        bind:value={summary}
        class="summary-input"
      />
      {#if hasAiCmd}
        <button
          type="button"
          class="icon-btn"
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

    {#if showDescription}
      <textarea
        placeholder="Add an optional extended description…"
        aria-label="Commit extended description"
        bind:value={description}
        rows="4"
        class="desc-input"
      ></textarea>
    {/if}

    <div class="actions-row">
      <button
        type="button"
        class="text-btn"
        aria-expanded={showDescription}
        onclick={() => (showDescription = !showDescription)}
      >
        <Icon name={showDescription ? "chevron-up" : "plus"} size={11} />
        <span>{showDescription ? "Hide description" : "Description"}</span>
      </button>
      <button
        type="button"
        class="text-btn"
        aria-expanded={showCoAuthorPicker}
        onclick={toggleCoAuthorPicker}
      >
        <Icon name={showCoAuthorPicker ? "chevron-up" : "plus"} size={11} />
        <span>Co-authors{coAuthors.length > 0 ? ` (${coAuthors.length})` : ""}</span>
      </button>
    </div>

    {#if coAuthors.length > 0}
      <ul class="coauthor-chips" aria-label="Selected co-authors">
        {#each coAuthors as co, i (coAuthorEmail(co))}
          <li class="coauthor-chip">
            <span class="coauthor-chip-name" title={formatCoAuthor(co)}>{co.name}</span>
            <button
              type="button"
              class="chip-remove"
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
      <div class="subpanel">
        <div class="coauthor-input-wrap">
          <input
            type="text"
            class="field text-sm"
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
            class="icon-btn bordered"
            disabled={!coAuthorInput.trim()}
            onclick={addTypedCoAuthor}
            title="Add co-author"
            aria-label="Add co-author"
          >
            <Icon name="plus" size={13} />
          </button>
        </div>

        {#if loadingSuggestions}
          <div class="panel-note text-xs text-muted">Loading recent contributors…</div>
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
                  <span class="coauthor-suggestion-email text-xs text-muted">{s.email}</span>
                </button>
              </li>
            {/each}
          </ul>
        {:else}
          <div class="panel-note text-xs text-muted">
            No recent contributors. Type one above.
          </div>
        {/if}
      </div>
    {/if}

    {#if hasConflicts}
      <div class="conflict-warning" role="alert">
        <span class="text-strong">Resolve conflicts before committing.</span>
        <ul>
          {#each conflictedPaths as path (path)}
            <li class="font-mono text-xs">{path}</li>
          {/each}
        </ul>
      </div>
    {/if}

    <!-- Compact identity strip. Everything is a chip; details expand on demand. -->
    <div class="chips">
      <button
        type="button"
        class="chip"
        class:chip-warn={!authorReady && !identityLoading}
        aria-expanded={authorOpen}
        title="Change the commit author"
        onclick={() => (authorOpen = !authorOpen)}
      >
        <span class="avatar">{authorInitials}</span>
        <span class="chip-label">
          {identityLoading ? "Reading identity…" : displayName || "Author not set"}
        </span>
        <Icon name={authorOpen ? "chevron-up" : "chevron-down"} size={11} />
      </button>

      <button
        type="button"
        class="chip"
        aria-expanded={githubOpen}
        title="GitHub account and push access"
        onclick={() => (githubOpen = !githubOpen)}
      >
        <Icon name="git-branch" size={12} />
        <span class="chip-label">{githubChipLabel}</span>
        <span class="dot {githubState}"></span>
        <Icon name={githubOpen ? "chevron-up" : "chevron-down"} size={11} />
      </button>
    </div>

    {#if authorOpen}
      <div class="subpanel">
        {#if identityError}
          <div class="panel-note text-xs text-muted">{identityError}</div>
        {:else if identity}
          <div class="author-line">
            <span class="text-strong">{displayName || "No name set"}</span>
            <span class="text-xs text-muted font-mono">{displayEmail || "No email set"}</span>
            <Badge variant="neutral">{authorModeLabel}</Badge>
          </div>

          <div class="segmented" role="group" aria-label="Commit author">
            <button
              type="button"
              class:active={authorMode === "repo"}
              disabled={!repoIdentityReady}
              title={repoIdentityReady
                ? "Use the repository Git identity"
                : "No repository-specific identity is set"}
              onclick={() => selectAuthorMode("repo")}
            >
              Repository
            </button>
            <button
              type="button"
              class:active={authorMode === "global"}
              disabled={!globalIdentityReady}
              title={globalIdentityReady
                ? "Use the global Git identity for this commit"
                : "No global identity is set"}
              onclick={() => selectAuthorMode("global")}
            >
              Global
            </button>
            <button
              type="button"
              class:active={authorMode === "custom"}
              onclick={() => selectAuthorMode("custom")}
            >
              Custom
            </button>
          </div>

          {#if authorMode === "custom"}
            <div class="custom-author">
              <input
                type="text"
                class="field text-sm"
                class:has-error={!!customNameError}
                placeholder="Author name"
                aria-label="Author name"
                bind:value={customName}
                oninput={() => (identityTouched = true)}
              />
              <input
                type="email"
                class="field text-sm"
                class:has-error={!!customEmailError}
                placeholder="author@example.com"
                aria-label="Author email"
                bind:value={customEmail}
                oninput={() => (identityTouched = true)}
              />
              {#if customNameError && customName.trim()}
                <span class="field-error text-xs">{customNameError}</span>
              {/if}
              {#if customEmailError && customEmail.trim()}
                <span class="field-error text-xs">{customEmailError}</span>
              {/if}
            </div>
          {:else if authorMode === "global"}
            <p class="panel-note text-xs text-muted">
              Uses the global identity for this commit. Git config is not changed.
            </p>
          {:else}
            <p class="panel-note text-xs text-muted">
              Uses this repository's Git identity.
            </p>
          {/if}

          {#if amendMode}
            <p class="panel-note text-xs text-muted">
              Amend keeps the original author unless you choose an override.
            </p>
          {/if}

          {#if !authorReady}
            <div class="inline-warning">
              <span class="text-xs">Git author identity is incomplete.</span>
              <button type="button" class="text-btn" onclick={openGitSettings}>
                Open Git settings
              </button>
            </div>
          {/if}
        {:else}
          <div class="panel-note text-xs text-muted">No Git identity available.</div>
        {/if}
      </div>
    {/if}

    {#if githubOpen}
      <div class="subpanel">
        <div class="subpanel-head">
          <span class="text-xs text-muted">GitHub account</span>
          <button
            type="button"
            class="icon-btn"
            onclick={loadGitHub}
            disabled={githubLoading}
            title="Refresh GitHub status"
            aria-label="Refresh GitHub status"
          >
            <Icon name="refresh" size={12} />
          </button>
        </div>

        {#if githubLoading}
          <div class="panel-note text-xs text-muted">Checking GitHub CLI…</div>
        {:else if githubError}
          <div class="panel-note text-xs text-muted">{githubError}</div>
        {:else if github}
          {#if github.remoteUrl}
            <div class="remote-line font-mono text-xs" title={github.remoteUrl}>
              {#if github.host}{github.host}/{/if}{github.owner ?? ""}{github.repo
                ? `/${github.repo}`
                : ""}
            </div>
          {:else}
            <div class="panel-note text-xs text-muted">
              No origin remote. Commits stay local.
            </div>
          {/if}

          {#if github.ghInstalled}
            {#if github.accounts.length > 0}
              <ul class="account-list">
                {#each github.accounts as account (account.host + ":" + account.login)}
                  <li class="account">
                    <span class="account-login text-sm">{account.login}</span>
                    <span class="text-xs text-muted">{account.host}</span>
                    {#if account.active}
                      <Badge variant="accent">Active</Badge>
                    {/if}
                    {#if account.state !== "success"}
                      <Badge variant="danger">{account.state}</Badge>
                    {/if}
                  </li>
                {/each}
              </ul>
            {:else}
              <div class="panel-note text-xs text-muted">
                Run <span class="font-mono">gh auth login</span> to connect an account.
              </div>
            {/if}
          {:else}
            <div class="panel-note text-xs text-muted">
              GitHub CLI (gh) is not installed. Commits use your system Git credentials.
            </div>
          {/if}

          {#if github.isGithub && github.authenticated}
            <div class="permission-line">
              <span class="text-xs text-muted">Push access</span>
              {#if github.canPush === true}
                <Badge variant="success">Allowed</Badge>
              {:else if github.canPush === false}
                <Badge variant="warning">Read-only</Badge>
              {:else}
                <Badge variant="neutral">Unknown</Badge>
              {/if}
              {#if githubPermissionDetail}
                <span class="text-xs text-muted">{githubPermissionDetail}</span>
              {/if}
            </div>
            {#if github.canPush !== null}
              <p class="panel-note text-xs text-muted">
                SSH keys and other credential helpers may differ.
              </p>
            {/if}
          {/if}

          {#if github.detail}
            <p class="panel-note text-xs text-muted">{github.detail}</p>
          {/if}
        {/if}
      </div>
    {/if}
  </div>

  {#snippet footer()}
    <div class="dialog-footer">
      <div class="footer-left">
        {#if !amendMode}
          <button
            type="button"
            class="icon-btn"
            onclick={enterAmendMode}
            disabled={isLoadingHead || isCommitting}
            title="Amend last commit"
            aria-label="Amend last commit"
          >
            <Icon name="pencil" size={13} />
          </button>
        {/if}
        <button
          type="button"
          class="icon-btn"
          onclick={handleUndo}
          disabled={isUndoing || isCommitting}
          title="Undo last commit"
          aria-label="Undo last commit"
        >
          <Icon name="undo" size={13} />
        </button>
      </div>
      <div class="footer-right">
        <Button variant="ghost" onclick={handleClose} disabled={isCommitting}>Cancel</Button>
        <Button variant="primary" disabled={!canCommit} onclick={handleCommit}>
          {#if isCommitting}
            {amendMode ? "Amending…" : "Committing…"}
          {:else if amendMode}
            {selectedPaths.length > 0
              ? `Amend ${selectedPaths.length} ${selectedPaths.length === 1 ? "file" : "files"}`
              : "Amend message"}
          {:else}
            Commit {selectedPaths.length} {selectedPaths.length === 1 ? "file" : "files"}
          {/if}
        </Button>
      </div>
    </div>
  {/snippet}
</Modal>

<style>
  .commit-dialog {
    padding: 16px 20px 20px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .input-wrap {
    display: flex;
    align-items: center;
    gap: 6px;
    border: 1px solid var(--border);
    background-color: var(--bg);
    padding: 0 8px;
  }

  .input-wrap:focus-within {
    border-color: var(--accent);
  }

  .summary-input {
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    color: var(--text);
    padding: 9px 0;
    outline: none;
  }

  .summary-input::placeholder,
  .desc-input::placeholder,
  .field::placeholder {
    color: var(--text-faint);
  }

  .desc-input {
    width: 100%;
    resize: vertical;
    border: 1px solid var(--border);
    background-color: var(--bg);
    color: var(--text);
    padding: 8px;
    outline: none;
  }

  .desc-input:focus {
    border-color: var(--accent);
  }

  .counter {
    align-self: flex-end;
    margin-top: -6px;
    color: var(--text-muted);
  }

  .counter-amber {
    color: var(--warning);
  }

  .actions-row {
    display: flex;
    align-items: center;
    gap: 16px;
  }

  .text-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 0;
    border: 0;
    background: none;
    color: var(--text-muted);
    cursor: pointer;
  }

  .text-btn:hover:not(:disabled) {
    color: var(--text);
  }

  .text-btn:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    flex-shrink: 0;
  }

  .icon-btn:hover:not(:disabled) {
    background-color: var(--bg-muted);
    color: var(--text);
  }

  .icon-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .icon-btn.bordered {
    border: 1px solid var(--border);
    background-color: var(--bg-subtle);
  }

  .amend-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 8px 10px;
    background-color: var(--accent-bg);
    color: var(--text);
  }

  .amend-body {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .amend-label {
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .amend-subject {
    color: var(--text);
    margin-left: 6px;
  }

  .amend-warning {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    padding: 8px 10px;
    border: 1px solid var(--danger);
    background-color: var(--danger-bg);
    color: var(--text);
  }

  .coauthor-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .coauthor-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 6px;
    background-color: var(--bg-muted);
    color: var(--text);
  }

  .coauthor-chip-name {
    max-width: 180px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chip-remove {
    display: inline-flex;
    align-items: center;
    border: 0;
    background: none;
    color: var(--text-muted);
    cursor: pointer;
    padding: 0;
  }

  .chip-remove:hover {
    color: var(--text);
  }

  .coauthor-input-wrap {
    display: flex;
    gap: 6px;
  }

  .coauthor-suggestions {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
    max-height: 140px;
    overflow-y: auto;
  }

  .coauthor-suggestion {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
    width: 100%;
    padding: 4px 6px;
    border: 0;
    background: transparent;
    color: var(--text);
    cursor: pointer;
    text-align: left;
  }

  .coauthor-suggestion:hover:not(:disabled) {
    background-color: var(--bg-muted);
  }

  .coauthor-suggestion:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .coauthor-suggestion-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .conflict-warning {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 8px 10px;
    border: 1px solid var(--danger);
    background-color: var(--danger-bg);
    color: var(--text);
  }

  .conflict-warning ul {
    margin: 0;
    padding-left: 18px;
  }

  .chips {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    padding-top: 2px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    max-width: 100%;
    padding: 4px 8px;
    border: 1px solid var(--border-subtle);
    background-color: var(--bg-subtle);
    color: var(--text);
    cursor: pointer;
  }

  .chip:hover {
    border-color: var(--border);
    background-color: var(--bg-muted);
  }

  .chip-warn {
    border-color: var(--warning);
  }

  .chip-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .avatar {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background-color: var(--accent-bg);
    color: var(--accent);
    font-size: 0.7rem;
    font-weight: 600;
    flex-shrink: 0;
  }

  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background-color: var(--text-faint);
    flex-shrink: 0;
  }

  .dot.ok {
    background-color: var(--success);
  }

  .dot.warn {
    background-color: var(--warning);
  }

  .subpanel {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
    border: 1px solid var(--border-subtle);
    background-color: var(--bg-subtle);
  }

  .subpanel-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .panel-note {
    margin: 0;
    color: var(--text-muted);
    line-height: 16px;
  }

  .author-line {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .segmented {
    display: flex;
    border: 1px solid var(--border);
    background-color: var(--bg);
  }

  .segmented button {
    flex: 1;
    padding: 4px 6px;
    border: 0;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
  }

  .segmented button + button {
    border-left: 1px solid var(--border-subtle);
  }

  .segmented button:hover:not(:disabled) {
    background-color: var(--bg-muted);
    color: var(--text);
  }

  .segmented button.active {
    background-color: var(--bg-muted);
    color: var(--text);
  }

  .segmented button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .custom-author {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .field {
    width: 100%;
    border: 1px solid var(--border);
    background-color: var(--bg);
    color: var(--text);
    padding: 5px 8px;
    outline: none;
  }

  .field:focus {
    border-color: var(--accent);
  }

  .field.has-error {
    border-color: var(--danger);
  }

  .field-error {
    color: var(--danger);
  }

  .inline-warning {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px 8px;
    border: 1px solid var(--warning);
    background-color: var(--warning-bg);
    color: var(--text);
  }

  .remote-line {
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .permission-line {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .account-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .account {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .account-login {
    color: var(--text);
    font-weight: 500;
  }

  .dialog-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    width: 100%;
  }

  .footer-left,
  .footer-right {
    display: flex;
    align-items: center;
    gap: 8px;
  }
</style>
