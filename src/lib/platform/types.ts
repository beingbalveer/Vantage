// Data models mirroring Rust backend structs (PRODUCT_SPEC.md Section 5)

export interface RepoEntry {
  id: string;
  path: string;
  name: string;
  branch?: string | null;
  pinned: boolean;
  order: number;
  marker: Marker | null;
  notes: ReviewNote[];
}

export interface Marker {
  sha: string;
  setAt: string; // ISO-8601
  branch: string | null;
}

export type MarkerState = "ok" | "missing" | "diverged" | "none";

/** In-progress repository operation that must be finished or aborted. */
export type OperationKind = "merge" | "rebase" | "cherryPick" | "revert";

export interface RepoStatus {
  repoId: string;
  branch: string | null;
  detachedSha: string | null;
  upstream: string | null;
  ahead: number;
  behind: number;
  dirtyCount: number;
  operation?: OperationKind | null;
  conflictedCount?: number;
  conflictedPaths?: string[];
  newCommitCount: number;
  markerState: MarkerState;
  lastActivityAt: string | null;
  lastFetchedAt: string | null;
  error: string | null;
  isCheckedOut?: boolean;
}

export interface CommitSummary {
  sha: string;
  short: string;
  authorName: string;
  authorEmail: string;
  date: string; // ISO-8601
  subject: string;
  body: string;
  parents: string[];
  filesChanged: number;
  additions: number;
  deletions: number;
  flags: TriageFlag[];
  isNew: boolean;
}

export interface HistoryPage {
  commits: CommitSummary[];
  total: number;
  capped: boolean;
}

export type FileStatus =
  | "added"
  | "modified"
  | "deleted"
  | "renamed"
  | "copied"
  | "typeChanged"
  | "untracked"
  | "conflicted";

export interface FileChange {
  path: string;
  oldPath: string | null;
  status: FileStatus;
  additions: number;
  deletions: number;
  binary: boolean;
  oldMode: string | null;
  newMode: string | null;
}

export interface DiffFile {
  change: FileChange;
  hunks: Hunk[];
  truncated: boolean;
}

export interface LineSelection {
  kind: LineKind;
  oldNo: number | null;
  newNo: number | null;
  text: string;
}

export interface Hunk {
  oldStart: number;
  oldLines: number;
  newStart: number;
  newLines: number;
  header: string;
  lines: DiffLine[];
}

export type LineKind = "context" | "add" | "del" | "noNewline";

export interface DiffLine {
  kind: LineKind;
  oldNo: number | null;
  newNo: number | null;
  text: string;
  wordRanges: [number, number][] | null;
}

export type DiffScope =
  | { type: "commit"; sha: string }
  | { type: "range"; from: string; to: string }
  | { type: "working" };

export type FlagKind =
  | "large"
  | "deletionHeavy"
  | "dependencies"
  | "config"
  | "testsRemoved"
  | "secrets"
  | "bigFile"
  | "binary";

export type Severity = "info" | "caution" | "warn";

export interface TriageFlag {
  kind: FlagKind;
  severity: Severity;
  detail: string;
}

export type NoteSide = "new" | "old";

export interface ReviewNote {
  id: string;
  file: string;
  lineStart: number;
  lineEnd: number;
  side: NoteSide;
  commitSha: string | null;
  text: string;
  createdAt: string;
  snippet: string;
}

export type Theme = "system" | "light" | "dark";
export type DiffMode = "unified" | "split";

export interface Settings {
  theme: Theme;
  diffMode: DiffMode;
  ignoreWhitespace: boolean;
  contextLines: number;
  autoFetchMinutes: number;
  shell: string | null;
  externalEditorCmd: string | null;
  commitMessageCmd: string | null;
  terminalFontSize: number;
  codeFontSize: number;
  treeFontSize: number;
  uiScale: number;
  defaultBranch: string;
  tabSize: number;
  confirmTerminalRestart: boolean;
}

export interface PaneLayout {
  repoId: string | null;
  tab: "history" | "files";
  widths: number[] | null;
}

export interface WindowBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Layout {
  paneCount: number;
  panes: PaneLayout[];
  terminalHeight: number;
  terminalOpen: boolean;
  windowBounds: WindowBounds | null;
}

export interface BranchInfo {
  name: string;
  upstream: string | null;
  isHead: boolean;
  /** Committer date of the branch tip (ISO-8601), or null for an unborn branch. */
  updatedAt: string | null;
}

export interface StashEntry {
  index: number;
  sha: string;
  subject: string;
  date: string; // ISO-8601
}

export interface StashPushResult {
  stashed: boolean;
  entry: StashEntry | null;
}

export interface CheckoutResult {
  stash: StashEntry | null;
}

export type DirtyAction = "leave" | "bring" | "stash";


export interface TagInfo {
  name: string;
  /** Peeled commit/object SHA the tag points at. */
  target: string;
  annotated: boolean;
  /** Annotation message subject line; null for lightweight tags. */
  message: string | null;
  date: string | null;
}


export interface BackupInfo {
  ref: string;
  createdAt: string;
  kind: string;
  description: string;
}

export interface DefHit {
  path: string;
  line: number;
  lineText: string;
  contextLines: string[];
}

export interface BlameInfo {
  sha: string;
  author: string;
  date: string;
  subject: string;
}

export interface BlobTextResult {
  text: string;
  tooLarge: boolean;
  binary: boolean;
}

/** Single-file preview for the Files viewer (text, image, or binary state). */
export interface BlobPreview {
  size: number;
  binary: boolean;
  tooLarge: boolean;
  /** Image MIME type when the path is a recognised image. */
  mime: string | null;
  /** `data:<mime>;base64,...` for images within the preview cap. */
  image: string | null;
  /** Source text (ordinary files, and SVG so the viewer can toggle source). */
  text: string;
}

/** Old/new metadata for a single changed file (sizes + bounded image previews). */
export interface FileDiffMeta {
  oldSize: number | null;
  newSize: number | null;
  oldExists: boolean;
  newExists: boolean;
  oldImage: string | null;
  newImage: string | null;
  oldTooLarge: boolean;
  newTooLarge: boolean;
  capBytes: number;
}

export interface EnvInfo {
  gitVersion: string;
  shell: string;
  startupWarning?: string | null;
  defaultShell: string;
  shells: DetectedApp[];
  editors: DetectedApp[];
}

export interface DetectedApp {
  id: string;
  label: string;
  command: string;
}

export interface GitIdentity {
  globalName: string | null;
  globalEmail: string | null;
  localName: string | null;
  localEmail: string | null;
  effectiveName: string | null;
  effectiveEmail: string | null;
  nameSource: "repo" | "global" | "system" | "unset";
  emailSource: "repo" | "global" | "system" | "unset";
}

/** A one-off author for a single commit. */
export interface CommitAuthor {
  name: string;
  email: string;
}

export interface GitHubAccount {
  host: string;
  login: string;
  state: string;
  active: boolean;
  tokenSource?: string | null;
  scopes: string[];
  gitProtocol?: string | null;
}

export interface GitHubStatus {
  ghInstalled: boolean;
  ghVersion: string | null;
  remoteUrl: string | null;
  host: string | null;
  owner: string | null;
  repo: string | null;
  isGithub: boolean;
  authenticated: boolean;
  accounts: GitHubAccount[];
  activeAccount: string | null;
  canPush: boolean | null;
  permission: string | null;
  detail: string | null;
}

export interface HeadCommitInfo {
  sha: string;
  short: string;
  subject: string;
  body: string;
  date: string;
  isPushed: boolean;
  hasParent: boolean;
  operationInProgress: string | null;
}

export interface UndoCommitResult {
  undone: CommitSummary;
  backupRef: string;
  message: string;
}

export interface CommitMsgResult {
  summary: string;
  body: string | null;
}

export interface RecentContributor {
  name: string;
  email: string;
}

export interface AppError {
  code: string;
  message: string;
  stderr: string | null;
}

export interface NavigationTarget {
  repoId?: string;
  tab?: "history" | "files";
  commitSha?: string;
  path?: string;
  line?: number;
  rev?: string;
}

export interface RepoStackInfo {
  detected?: string | null;
  label?: string | null;
  matchedFile?: string | null;
}

export type RepoIconType = "tech" | "role" | "emoji" | "custom" | "letter";

export interface RepoIconChoice {
  type: RepoIconType;
  value: string;
  color?: string;
}
