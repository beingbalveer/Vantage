<script lang="ts">
  import type { DirtyAction } from "../../platform/types";
  import * as api from "../../platform/api";
  import { addToast, checkoutRequest, clearCheckoutRequest } from "../../app/ui";
  import { refreshSingleStatus, loadRepos } from "../../domain/stores/repos";
  import { Modal, Button, Input, Checkbox } from "../common";
  import { formatStashIndex, summarizeStashSubject } from "../../domain/stash";

  let isOpen = $derived($checkoutRequest !== null);
  let request = $derived($checkoutRequest);

  let action = $state<DirtyAction>("bring");
  let stashMessage = $state("");
  let includeUntracked = $state(true);
  let isCheckingOut = $state(false);
  let errorMessage = $state<string | null>(null);

  $effect(() => {
    if ($checkoutRequest) {
      action = "bring";
      stashMessage = "";
      includeUntracked = true;
      errorMessage = null;
      isCheckingOut = false;
    }
  });

  function handleClose() {
    if (isCheckingOut) return;
    errorMessage = null;
    clearCheckoutRequest();
  }

  async function handleContinue() {
    const req = request;
    if (!req || isCheckingOut) return;
    isCheckingOut = true;
    errorMessage = null;

    try {
      const result = await api.checkout(
        req.repoId,
        req.target,
        action,
        action === "stash" ? stashMessage.trim() || undefined : undefined,
        includeUntracked
      );

      if (action === "stash") {
        const entry = result.stash;
        const label = entry
          ? `${formatStashIndex(entry.index)} · ${summarizeStashSubject(entry.subject)}`
          : "your changes";
        addToast({
          message: `Stashed as ${label}, switched to ${req.target}`,
          kind: "success",
        });
      } else if (action === "bring") {
        addToast({ message: `Switched to ${req.target} with your changes`, kind: "success" });
      } else {
        addToast({ message: `Checked out ${req.target}`, kind: "success" });
      }

      await refreshSingleStatus(req.repoId);
      await loadRepos();
      clearCheckoutRequest();
    } catch (e: any) {
      // Stay open so the user can pick another option — never a dead end.
      errorMessage = e?.message || "Checkout failed";
      addToast({ message: errorMessage || "Checkout failed", kind: "error" });
    } finally {
      isCheckingOut = false;
    }
  }

  const options: { value: DirtyAction; title: string; hint: string }[] = [
    {
      value: "bring",
      title: "Bring my changes along",
      hint: "Stash them, switch, then re-apply on the new branch.",
    },
    {
      value: "leave",
      title: "Leave my changes here",
      hint: "Plain checkout. Git refuses if the changes would be overwritten.",
    },
    {
      value: "stash",
      title: "Stash them first",
      hint: "Park the changes and switch to a clean tree.",
    },
  ];
</script>

{#if isOpen && request}
  <Modal
    open={isOpen}
    title={`Switch to ${request.target}?`}
    description="You have uncommitted changes. Choose what happens to them."
    width="520px"
    closeOnEscape={!isCheckingOut}
    closeOnOutsideClick={!isCheckingOut}
    onclose={handleClose}
  >
    <div class="switch-content">
      <p class="text-sm text-muted">
        {request.dirtyCount} file{request.dirtyCount === 1 ? "" : "s"} changed in
        <span class="text-strong">{request.repoName}</span>.
      </p>

      <div class="options" role="radiogroup" aria-label="What to do with uncommitted changes">
        {#each options as option (option.value)}
          <button
            type="button"
            class="option {action === option.value ? 'selected' : ''}"
            role="radio"
            aria-checked={action === option.value}
            disabled={isCheckingOut}
            onclick={() => (action = option.value)}
          >
            <span class="option-radio" aria-hidden="true"></span>
            <span class="option-text">
              <span class="option-title text-strong">{option.title}</span>
              <span class="option-hint text-xs text-muted">{option.hint}</span>
            </span>
          </button>
        {/each}
      </div>

      {#if action === "stash"}
        <div class="stash-options">
          <Input
            bind:value={stashMessage}
            placeholder="Stash message (optional)"
            disabled={isCheckingOut}
          />
        </div>
      {/if}

      {#if action !== "leave"}
        <Checkbox
          bind:checked={includeUntracked}
          label="Include untracked files"
          disabled={isCheckingOut}
        />
      {/if}

      {#if errorMessage}
        <div class="error-banner text-sm" role="alert">{errorMessage}</div>
      {/if}
    </div>

    {#snippet footer()}
      <Button variant="default" onclick={handleClose} disabled={isCheckingOut}>
        Cancel
      </Button>
      <Button variant="primary" onclick={handleContinue} loading={isCheckingOut}>
        {action === "stash" ? "Stash and switch" : "Continue"}
      </Button>
    {/snippet}
  </Modal>
{/if}

<style>
  .switch-content {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .options {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .option {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    background-color: var(--bg);
    cursor: pointer;
    text-align: left;
    transition: border-color 80ms ease, background-color 80ms ease;
  }
  .option:hover:not(:disabled) {
    background-color: var(--bg-subtle);
  }
  .option.selected {
    border-color: var(--accent);
    background-color: var(--accent-bg);
  }
  .option:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .option-radio {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 1.5px solid var(--border);
    flex-shrink: 0;
    margin-top: 2px;
  }
  .option.selected .option-radio {
    border-color: var(--accent);
    background:
      radial-gradient(circle, var(--accent) 0 3px, transparent 3px);
  }

  .option-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .option-title {
    color: var(--text);
  }

  .option-hint {
    line-height: 16px;
  }

  .stash-options {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .error-banner {
    padding: 8px 12px;
    background-color: var(--danger-subtle, rgba(218, 54, 51, 0.1));
    border: 1px solid var(--danger);
    border-radius: var(--r-md);
    color: var(--danger);
    white-space: pre-wrap;
  }
</style>
