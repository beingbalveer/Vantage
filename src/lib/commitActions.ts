// Pure helpers for the amend / undo-commit flow (ALPH-154).
//
// Kept free of `api`/store imports so they stay trivially unit-testable. The
// backend is the source of truth for git behaviour; these only shape the UI
// (guards, warnings and toast copy).

export interface HeadCommitRef {
  short: string;
  subject: string;
}

export type CommitGuard = { ok: true } | { ok: false; reason: string };

function operationReason(operation: string, verb: string): string {
  return `A ${operation} is in progress. Finish or abort it in the terminal before ${verb}.`;
}

/** Whether "Amend last commit" can run for the current HEAD. */
export function canAmendLastCommit(
  head: { sha: string } | null | undefined,
  operationInProgress: string | null | undefined
): CommitGuard {
  if (operationInProgress) {
    return { ok: false, reason: operationReason(operationInProgress, "amending") };
  }
  if (!head || !head.sha) {
    return { ok: false, reason: "There are no commits to amend yet." };
  }
  return { ok: true };
}

/** Whether "Undo last commit" can run (needs HEAD and a parent commit). */
export function canUndoLastCommit(
  hasHead: boolean,
  hasParent: boolean,
  operationInProgress: string | null | undefined
): CommitGuard {
  if (operationInProgress) {
    return { ok: false, reason: operationReason(operationInProgress, "undoing a commit") };
  }
  if (!hasHead) {
    return { ok: false, reason: "There are no commits to undo yet." };
  }
  if (!hasParent) {
    return {
      ok: false,
      reason: "Cannot undo the initial commit — there is no earlier commit to reset to.",
    };
  }
  return { ok: true };
}

/** Toast copy after a successful undo (mirrors the Rust `undo_success_message`). */
export function undoCommitMessage(commit: HeadCommitRef): string {
  return `Undid commit ${commit.short} (${commit.subject}). Its changes are staged and the message is ready to re-commit.`;
}

/** Warning shown when amending a commit that is already on a remote. */
export function amendWarning(isPushed: boolean): string | null {
  return isPushed
    ? "This commit is already on the remote. Amending rewrites the branch — you will need to force push."
    : null;
}
