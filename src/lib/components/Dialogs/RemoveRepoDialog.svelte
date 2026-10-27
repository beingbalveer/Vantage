<script lang="ts">
  import { activeDialog, closeDialog, addToast } from "../../app/ui";
  import { repos, removeRepo } from "../../domain/stores/repos";
  import { panes } from "../../domain/stores/panes";
  import { get } from "svelte/store";
  import { Modal, Button } from "../common";

  let isStoreActive = $derived($activeDialog?.type === "remove_repo");
  let repoId = $derived(isStoreActive ? $activeDialog?.repoId || "" : "");
  let repo = $derived($repos.find((r) => r.id === repoId) || $activeDialog?.data?.repo || null);

  let isRemoving = $state(false);

  function handleClose() {
    if (isRemoving) return;
    closeDialog();
  }

  async function handleConfirmRemove() {
    if (!repoId || isRemoving) return;
    isRemoving = true;

    try {
      const repoNameToRemove = repo?.name || "repository";
      const targetId = repoId;

      panes.update((list) => {
        const remaining = get(repos).filter((r) => r.id !== targetId);
        const nextId = remaining.length > 0 ? remaining[0].id : null;
        return list.map((p) => (p.repoId === targetId ? { ...p, repoId: nextId } : { ...p }));
      });

      await removeRepo(targetId);
      addToast({ message: `Removed repository ${repoNameToRemove}`, kind: "info" });
      closeDialog();
    } catch (err: any) {
      addToast({ message: err.message || "Failed to remove repository", kind: "error" });
    } finally {
      isRemoving = false;
    }
  }
</script>

<Modal
  open={isStoreActive && !!repo}
  title="Remove from Vantage"
  onclose={handleClose}
>
  <div class="remove-dialog-content">
    <p class="text-base text-strong">
      Remove "{repo?.name}{repo?.branch ? ` (${repo.branch})` : ""}" from Vantage?
    </p>
    <p class="text-sm text-muted">
      Your repository files on disk will not be touched.
    </p>
  </div>

  {#snippet footer()}
    <Button
      variant="default"
      onclick={handleClose}
      disabled={isRemoving}
    >
      Cancel
    </Button>
    <Button
      variant="danger"
      onclick={handleConfirmRemove}
      disabled={isRemoving}
      loading={isRemoving}
    >
      {isRemoving ? "Removing…" : "Remove from Vantage"}
    </Button>
  {/snippet}
</Modal>

<style>
  .remove-dialog-content {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
</style>
