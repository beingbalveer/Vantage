import { writable } from "svelte/store";

/** ALPH-154: request the commit composer to open in amend mode for a repo. */
export interface ComposerRequest {
  repoId: string;
  action: "amend";
}
export const composerRequest = writable<ComposerRequest | null>(null);

/**
 * ALPH-154: after undo/reset the commit message is stashed here so the composer
 * can pre-fill it the next time it opens for that repo.
 */
export interface PendingCommitMessage {
  repoId: string;
  summary: string;
  body: string;
}
export const pendingCommitMessage = writable<PendingCommitMessage | null>(null);
