import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  BackupInfo,
  BlameInfo,
  BlobTextResult,
  BranchInfo,
  CheckoutResult,
  CommitMsgResult,
  CommitSummary,
  DefHit,
  DiffFile,
  DiffScope,
  DirtyAction,
  EnvInfo,
  FileChange,
  HeadCommitInfo,
  HistoryPage,
  FileDiffMeta,
  Layout,
  RepoEntry,
  RepoStatus,
  RepoStackInfo,
  Settings,
  StashEntry,
  StashPushResult,
  TagInfo,
  TriageFlag,
  RecentContributor,
  UndoCommitResult,
} from "./types";

// Repos
export async function repoAdd(
  path: string,
  branch?: string | null,
  unreviewedLastN?: number
): Promise<RepoEntry> {
  return invoke("repo_add", { path, branch: branch || undefined, unreviewedLastN });
}

export async function pickDirectory(): Promise<string | null> {
  return invoke("pick_directory");
}

export async function repoClone(
  cloneId: string,
  url: string,
  destParent: string,
  name?: string,
  branch?: string
): Promise<RepoEntry> {
  return invoke("repo_clone", {
    cloneId,
    url,
    destParent,
    name: name || undefined,
    branch: branch || undefined,
  });
}

export async function repoScan(path: string, depth?: number): Promise<string[]> {
  return invoke("repo_scan", { path, depth });
}

export async function repoRemove(id: string): Promise<void> {
  return invoke("repo_remove", { id });
}

export async function repoUpdate(
  id: string,
  update: { name?: string; branch?: string | null; pinned?: boolean; order?: number }
): Promise<RepoEntry> {
  return invoke("repo_update", { id, update });
}

export async function repoRelocate(id: string, newPath: string): Promise<RepoEntry> {
  return invoke("repo_relocate", { id, newPath });
}

export async function repoList(): Promise<RepoEntry[]> {
  return invoke("repo_list");
}

export async function repoStatus(id: string): Promise<RepoStatus> {
  return invoke("repo_status", { id });
}

export async function repoRefreshAll(): Promise<RepoStatus[]> {
  return invoke("repo_refresh_all");
}

export async function repoDetectStack(path: string): Promise<RepoStackInfo> {
  return invoke("repo_detect_stack", { path });
}

export async function pickIconFile(): Promise<string | null> {
  return invoke("pick_icon_file");
}

// Marker
export async function markerSet(id: string, sha: string): Promise<void> {
  return invoke("marker_set", { id, sha });
}

export async function markerSetToHead(id: string): Promise<void> {
  // NOTE: not used by the review flow. "Mark reviewed" must target the captured
  // snapshot end (see stores/review.ts); calling this at click time marks
  // commits that arrived mid-review as reviewed (ALPH-146).
  return invoke("marker_set_to_head", { id });
}

// History & Diff
export async function logPage(
  id: string,
  skip: number,
  limit: number,
  query?: string
): Promise<CommitSummary[]> {
  return invoke("log_page", { id, skip, limit, query });
}

export async function historyPage(
  id: string,
  skip: number,
  limit: number,
  query?: string
): Promise<HistoryPage> {
  return invoke("history_page", { id, skip, limit, query });
}

export async function commitFlags(
  id: string,
  shas: string[]
): Promise<Record<string, TriageFlag[]>> {
  return invoke("commit_flags", { id, shas });
}

export async function scopeFiles(
  id: string,
  scope: DiffScope,
  ignoreWs: boolean
): Promise<FileChange[]> {
  return invoke("scope_files", { id, scope, ignoreWs });
}

export async function scopeFileDiff(
  id: string,
  scope: DiffScope,
  path: string,
  ctxLines: number,
  ignoreWs: boolean,
  force: boolean
): Promise<DiffFile> {
  return invoke("scope_file_diff", { id, scope, path, ctxLines, ignoreWs, force });
}

export async function blobLines(
  id: string,
  rev: string,
  path: string,
  start: number,
  end: number
): Promise<string[]> {
  return invoke("blob_lines", { id, rev, path, start, end });
}

export async function blobText(
  id: string,
  rev: string,
  path: string
): Promise<BlobTextResult> {
  return invoke("blob_text", { id, rev, path });
}

export async function treeList(id: string, rev: string): Promise<string[]> {
  return invoke("tree_list", { id, rev });
}

export async function fileDiffMeta(
  id: string,
  scope: DiffScope,
  path: string,
  oldPath?: string | null
): Promise<FileDiffMeta> {
  return invoke("file_diff_meta", { id, scope, path, oldPath: oldPath ?? null });
}

export async function blameLine(
  id: string,
  rev: string,
  path: string,
  line: number
): Promise<BlameInfo> {
  return invoke("blame_line", { id, rev, path, line });
}

export async function grepDefs(
  id: string,
  rev: string,
  ident: string,
  extHint?: string
): Promise<DefHit[]> {
  return invoke("grep_defs", { id, rev, ident, extHint });
}

// Writes
export async function commit(
  id: string,
  paths: string[],
  summary: string,
  body?: string,
  coAuthors?: string[]
): Promise<CommitSummary> {
  return invoke("commit", { id, paths, summary, body, coAuthors });
}

export async function recentContributors(
  id: string,
  limit?: number
): Promise<RecentContributor[]> {
  return invoke("recent_contributors", { id, limit });
}

export async function headCommitInfo(id: string): Promise<HeadCommitInfo> {
  return invoke("head_commit_info", { id });
}

export async function amendCommit(
  id: string,
  paths: string[],
  summary: string,
  body?: string
): Promise<CommitSummary> {
  return invoke("amend_commit", { id, paths, summary, body });
}

export async function undoLastCommit(id: string): Promise<UndoCommitResult> {
  return invoke("undo_last_commit", { id });
}

export async function discard(id: string, paths: string[]): Promise<BackupInfo> {
  return invoke("discard", { id, paths });
}

export async function revert(id: string, sha: string): Promise<CommitSummary> {
  return invoke("revert", { id, sha });
}

export async function revertRange(
  id: string,
  from: string,
  to: string
): Promise<CommitSummary[]> {
  return invoke("revert_range", { id, from, to });
}

export async function resetTo(
  id: string,
  sha: string,
  alsoDiscardDirty: boolean
): Promise<BackupInfo> {
  return invoke("reset_to", { id, sha, alsoDiscardDirty });
}

export async function backupList(id: string): Promise<BackupInfo[]> {
  return invoke("backup_list", { id });
}

export async function backupRestore(id: string, ref: string): Promise<void> {
  return invoke("backup_restore", { id, ref });
}

export async function branchList(id: string): Promise<BranchInfo[]> {
  return invoke("branch_list", { id });
}

export async function checkout(
  id: string,
  target: string,
  dirtyAction?: DirtyAction,
  stashMessage?: string,
  includeUntracked?: boolean
): Promise<CheckoutResult> {
  return invoke("checkout", { id, target, dirtyAction, stashMessage, includeUntracked });
}

export async function stashList(id: string): Promise<StashEntry[]> {
  return invoke("stash_list", { id });
}

export async function stashPush(
  id: string,
  message?: string,
  includeUntracked?: boolean
): Promise<StashPushResult> {
  return invoke("stash_push", { id, message, includeUntracked });
}

export async function stashPop(id: string, index: number): Promise<void> {
  return invoke("stash_pop", { id, index });
}

export async function stashApply(id: string, index: number): Promise<void> {
  return invoke("stash_apply", { id, index });
}

export async function stashDrop(id: string, index: number): Promise<void> {
  return invoke("stash_drop", { id, index });
}

export async function branchCreate(
  id: string,
  name: string,
  startPoint?: string | null,
  checkout = false
): Promise<void> {
  return invoke("branch_create", { id, name, startPoint: startPoint || undefined, checkout });
}

export async function branchRename(id: string, from: string | null, to: string): Promise<void> {
  return invoke("branch_rename", { id, from: from || undefined, to });
}

export async function branchDelete(id: string, name: string, force = false): Promise<void> {
  return invoke("branch_delete", { id, name, force });
}

export async function branchIsMerged(id: string, name: string): Promise<boolean> {
  return invoke("branch_is_merged", { id, name });
}

// Tags
export async function tagList(id: string): Promise<TagInfo[]> {
  return invoke("tag_list", { id });
}

export async function tagCreate(
  id: string,
  name: string,
  target: string,
  annotated: boolean,
  message?: string
): Promise<void> {
  return invoke("tag_create", { id, name, target, annotated, message });
}

export async function tagPush(id: string, name: string): Promise<void> {
  return invoke("tag_push", { id, name });
}

export async function tagPushAll(id: string): Promise<void> {
  return invoke("tag_push_all", { id });
}

export async function syncFetch(id: string): Promise<void> {
  return invoke("sync_fetch", { id });
}

export async function syncPull(id: string): Promise<void> {
  return invoke("sync_pull", { id });
}

export async function syncPush(id: string, publish: boolean): Promise<void> {
  return invoke("sync_push", { id, publish });
}

export async function remoteUrl(id: string): Promise<string | null> {
  return invoke("remote_url", { id });
}

export async function checkPushed(id: string, sha: string): Promise<boolean> {
  return invoke("check_pushed", { id, sha });
}

// Conflict / in-progress operation flow (ALPH-156)
export async function operationContinue(id: string): Promise<void> {
  return invoke("operation_continue", { id });
}

export async function operationAbort(id: string): Promise<void> {
  return invoke("operation_abort", { id });
}

// Terminal
export async function termOpen(
  id: string,
  cols: number,
  rows: number
): Promise<{ replay: string }> {
  return invoke("term_open", { id, cols, rows });
}

export async function termInput(id: string, data: string): Promise<void> {
  return invoke("term_input", { id, data });
}

export async function termResize(id: string, cols: number, rows: number): Promise<void> {
  return invoke("term_resize", { id, cols, rows });
}

export async function termRestart(id: string): Promise<{ replay: string }> {
  return invoke("term_restart", { id });
}

export async function termKill(id: string): Promise<void> {
  return invoke("term_kill", { id });
}

export async function termPaste(id: string, text: string): Promise<void> {
  return invoke("term_paste", { id, text });
}

// Settings & Layout
export async function settingsGet(): Promise<Settings> {
  return invoke("settings_get");
}

export async function settingsSet(settings: Settings): Promise<void> {
  return invoke("settings_set", { settings });
}

export async function layoutGet(): Promise<Layout> {
  return invoke("layout_get");
}

export async function layoutSet(layout: Layout): Promise<void> {
  return invoke("layout_set", { layout });
}

export async function openExternalEditor(
  id: string,
  path: string,
  line?: number
): Promise<void> {
  return invoke("open_external_editor", { id, path, line });
}

export async function revealInFileManager(path: string): Promise<void> {
  return invoke("reveal_in_file_manager", { path });
}

export async function runCommitMessageCmd(
  id: string,
  paths: string[]
): Promise<CommitMsgResult> {
  return invoke("run_commit_message_cmd", { id, paths });
}

export async function envCheck(): Promise<EnvInfo> {
  return invoke("env_check");
}

// Event Listeners
export function onRepoStatus(cb: (status: RepoStatus) => void): Promise<UnlistenFn> {
  return listen<RepoStatus>("repo://status", (event) => cb(event.payload));
}

export function onHeadChanged(
  cb: (payload: { repoId: string; sha: string }) => void
): Promise<UnlistenFn> {
  return listen<{ repoId: string; sha: string }>("repo://head-changed", (event) =>
    cb(event.payload)
  );
}

export function onTermOutput(
  cb: (payload: { repoId: string; data: string }) => void
): Promise<UnlistenFn> {
  return listen<{ repoId: string; data: string }>("term://output", (event) =>
    cb(event.payload)
  );
}

export function onTermExit(
  cb: (payload: { repoId: string }) => void
): Promise<UnlistenFn> {
  return listen<{ repoId: string }>("term://exit", (event) => cb(event.payload));
}

export interface CloneProgressPayload {
  cloneId: string;
  line: string;
}

export function onCloneProgress(
  cb: (payload: CloneProgressPayload) => void
): Promise<UnlistenFn> {
  return listen<CloneProgressPayload>("repo://clone-progress", (event) =>
    cb(event.payload)
  );
}

// Native menu dispatch (see src-tauri/src/menu.rs and src/lib/menu.ts)
export function onMenuCommand(cb: (command: string) => void): Promise<UnlistenFn> {
  return listen<string>("menu://command", (event) => cb(event.payload));
}
