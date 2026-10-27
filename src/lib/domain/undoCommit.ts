import * as api from "../platform/api";
import type { UndoCommitResult } from "../platform/types";
import { addToast } from "../util/toast";
import { pendingCommitMessage } from "./stores/composer";
import { refreshSingleStatus, loadRepos } from "./stores/repos";

/**
 * Undo the last commit (`reset --soft HEAD~1`) and surface the outcome.
 *
 * Shared by the commit composer and the command palette so both paths behave
 * identically: the undone message is stashed for re-commit, the removed commit
 * stays reachable via its backup ref, and the sidebar/history refresh.
 */
export async function runUndoLastCommit(repoId: string): Promise<UndoCommitResult> {
  const result = await api.undoLastCommit(repoId);

  // Preserve the message so the composer can pre-fill it for re-commit.
  pendingCommitMessage.set({
    repoId,
    summary: result.undone.subject,
    body: result.undone.body || "",
  });

  addToast({
    message: result.message,
    kind: "info",
    actionLabel: "Restore commit",
    action: async () => {
      try {
        await api.backupRestore(repoId, result.backupRef);
        addToast({
          message: `Restored ${result.undone.short} from backup`,
          kind: "success",
        });
        await refreshSingleStatus(repoId);
        await loadRepos();
      } catch (e: any) {
        addToast({ message: e.message || "Failed to restore commit", kind: "error" });
      }
    },
  });

  await refreshSingleStatus(repoId);
  await loadRepos();
  return result;
}
