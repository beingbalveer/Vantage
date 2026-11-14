//! Seed data — a 1:1 transcription of `src/data/staticRepos.ts`.

use crate::model::{
    BranchInfo, ChangedFile, CommitEntry, FileChangeStatus, RepoSyncState, Repository,
};

macro_rules! cf {
    ($path:expr, $status:ident, $add:expr, $del:expr, $staged:expr) => {
        ChangedFile {
            path: $path.to_string(),
            status: FileChangeStatus::$status,
            additions: $add,
            deletions: $del,
            staged: $staged,
            diff_preview: Vec::new(),
        }
    };
    ($path:expr, $status:ident, $add:expr, $del:expr, $staged:expr, $diff:expr) => {
        ChangedFile {
            path: $path.to_string(),
            status: FileChangeStatus::$status,
            additions: $add,
            deletions: $del,
            staged: $staged,
            diff_preview: $diff.iter().map(|l: &&str| l.to_string()).collect(),
        }
    };
}

macro_rules! branch {
    ($name:expr, $current:expr, $default:expr, $ahead:expr, $behind:expr, $msg:expr, $time:expr) => {
        BranchInfo {
            name: $name.to_string(),
            is_current: $current,
            is_default: $default,
            ahead: $ahead,
            behind: $behind,
            last_commit_msg: $msg.to_string(),
            last_commit_time: $time.to_string(),
            author: "balveer".to_string(),
        }
    };
}

macro_rules! commit {
    ($sha:expr, $msg:expr, $time:expr, $branch:expr) => {
        CommitEntry {
            sha: $sha.to_string(),
            message: $msg.to_string(),
            author: "balveer".to_string(),
            relative_time: $time.to_string(),
            branch: $branch.to_string(),
        }
    };
}

/// `DEFAULT_GROUPS` in App.tsx
pub fn default_groups() -> Vec<String> {
    vec![
        "AI Tools".to_string(),
        "Web Apps".to_string(),
        "Packages & CLI".to_string(),
    ]
}

/// `STATIC_REPOSITORIES` in staticRepos.ts
pub fn static_repositories() -> Vec<Repository> {
    vec![
        Repository {
            id: "repo-prompt-workbench".into(),
            name: "prompt-workbench".into(),
            workspace: "AI Tools".into(),
            local_path: "~/Documents/GitHub/prompt-workbench".into(),
            remote_repo: "github.com/balveer/prompt-workbench".into(),
            current_branch: "feat/streaming-diff".into(),
            default_branch: "main".into(),
            sync_state: RepoSyncState::Modified,
            ahead_count: 3,
            behind_count: 0,
            stash_count: 1,
            last_activity_label: "14m ago".into(),
            last_activity_minutes: 14,
            primary_language: "TypeScript".into(),
            session_note: "Iterating on side-by-side token diff viewer".into(),
            uncommitted_files: vec![
                cf!(
                    "src/components/DiffViewer.tsx",
                    Modified,
                    84,
                    19,
                    true,
                    [
                        "@@ -42,8 +42,14 @@ export function DiffViewer({ left, right }: DiffViewerProps) {",
                        "-  const lines = computeBasicLines(left, right);",
                        "+  const lines = useMemo(() => computeTokenDiff(left, right), [left, right]);",
                        "+  const [wrapLines, setWrapLines] = useState(false);",
                    ]
                ),
                cf!(
                    "src/hooks/useStreamBuffer.ts",
                    Modified,
                    31,
                    7,
                    true,
                    [
                        "@@ -18,5 +18,9 @@ export function useStreamBuffer() {",
                        "+  const flushTimerRef = useRef<number | null>(null);",
                        "-  setChunks(prev => [...prev, chunk]);",
                        "+  queueChunk(chunk);",
                    ]
                ),
                cf!(
                    "src/utils/tokenCounter.ts",
                    Added,
                    62,
                    0,
                    false,
                    [
                        "@@ -0,0 +1,12 @@",
                        "+export function estimateTokenCount(input: string): number {",
                        "+  if (!input.trim()) return 0;",
                        "+  return Math.ceil(input.length / 3.8);",
                        "+}",
                    ]
                ),
                cf!("src/styles/editor.css", Modified, 11, 4, false),
                cf!("tests/diff-viewer.spec.ts", Untracked, 48, 0, false),
            ],
            branches: vec![
                branch!(
                    "feat/streaming-diff",
                    true,
                    false,
                    3,
                    0,
                    "Add buffered chunk flushing for large outputs",
                    "42m ago"
                ),
                branch!(
                    "main",
                    false,
                    true,
                    0,
                    0,
                    "Merge pull request #28 from feat/saved-presets",
                    "2d ago"
                ),
                branch!(
                    "exp/json-schema-validator",
                    false,
                    false,
                    2,
                    4,
                    "Prototype strict output validation panel",
                    "5d ago"
                ),
            ],
            recent_commits: vec![
                commit!(
                    "a4f91c2",
                    "Add buffered chunk flushing for large outputs",
                    "42m ago",
                    "feat/streaming-diff"
                ),
                commit!(
                    "81d02e9",
                    "Wire keyboard shortcuts for run comparison",
                    "2h ago",
                    "feat/streaming-diff"
                ),
                commit!(
                    "5e2b77a",
                    "Scaffold token diff utility module",
                    "4h ago",
                    "feat/streaming-diff"
                ),
            ],
        },
        Repository {
            id: "repo-meeting-notes-cli".into(),
            name: "meeting-notes-cli".into(),
            workspace: "Packages & CLI".into(),
            local_path: "~/Documents/GitHub/meeting-notes-cli".into(),
            remote_repo: "github.com/balveer/meeting-notes-cli".into(),
            current_branch: "feat/whisper-local".into(),
            default_branch: "main".into(),
            sync_state: RepoSyncState::Conflict,
            ahead_count: 1,
            behind_count: 2,
            stash_count: 0,
            last_activity_label: "38m ago".into(),
            last_activity_minutes: 38,
            primary_language: "TypeScript".into(),
            session_note: "Rebase conflict in config loader after upstream update".into(),
            uncommitted_files: vec![
                cf!(
                    "src/config/loadConfig.ts",
                    Conflicted,
                    18,
                    12,
                    false,
                    [
                        "<<<<<<< HEAD",
                        "  modelPath: \"~/.cache/whisper/base.en.bin\",",
                        "=======",
                        "  modelPath: process.env.WHISPER_MODEL ?? \"./models/base.bin\",",
                        ">>>>>>> origin/feat/whisper-local",
                    ]
                ),
                cf!("src/commands/transcribe.ts", Modified, 24, 9, false),
            ],
            branches: vec![
                branch!(
                    "feat/whisper-local",
                    true,
                    false,
                    1,
                    2,
                    "Add local audio chunking flag",
                    "38m ago"
                ),
                branch!(
                    "main",
                    false,
                    true,
                    0,
                    0,
                    "Release v0.8.2 markdown formatter fix",
                    "4d ago"
                ),
            ],
            recent_commits: vec![
                commit!(
                    "c91d3b4",
                    "Add local audio chunking flag",
                    "38m ago",
                    "feat/whisper-local"
                ),
                commit!(
                    "30f8a12",
                    "Support wav to 16kHz ffmpeg pipe",
                    "1d ago",
                    "feat/whisper-local"
                ),
            ],
        },
        Repository {
            id: "repo-invoice-ocr-api".into(),
            name: "invoice-ocr-api".into(),
            workspace: "AI Tools".into(),
            local_path: "~/Documents/GitHub/invoice-ocr-api".into(),
            remote_repo: "github.com/balveer/invoice-ocr-api".into(),
            current_branch: "fix/pdf-table-parser".into(),
            default_branch: "main".into(),
            sync_state: RepoSyncState::Behind,
            ahead_count: 1,
            behind_count: 3,
            stash_count: 2,
            last_activity_label: "1h ago".into(),
            last_activity_minutes: 60,
            primary_language: "Python".into(),
            session_note: "Fixing multi-page line item extraction edge cases".into(),
            uncommitted_files: vec![
                cf!(
                    "app/parsers/table_extractor.py",
                    Modified,
                    39,
                    15,
                    true,
                    [
                        "@@ -91,6 +91,11 @@ def extract_rows(page_blocks):",
                        "+    merged = merge_wrapped_descriptions(rows, y_tolerance=4.5)",
                        "-    return rows",
                        "+    return merged",
                    ]
                ),
                cf!("fixtures/multi_page_invoice.json", Added, 112, 0, false),
            ],
            branches: vec![
                branch!(
                    "fix/pdf-table-parser",
                    true,
                    false,
                    1,
                    3,
                    "Normalize currency symbols before total check",
                    "1h ago"
                ),
                branch!(
                    "main",
                    false,
                    true,
                    0,
                    2,
                    "Update FastAPI dependency pins",
                    "2d ago"
                ),
                branch!(
                    "feat/batch-webhook",
                    false,
                    false,
                    4,
                    0,
                    "Add HMAC signature header to webhook calls",
                    "6d ago"
                ),
            ],
            recent_commits: vec![
                commit!(
                    "7b44e01",
                    "Normalize currency symbols before total check",
                    "1h ago",
                    "fix/pdf-table-parser"
                ),
                commit!(
                    "19ab410",
                    "Add bounding box tolerance configuration",
                    "3h ago",
                    "fix/pdf-table-parser"
                ),
            ],
        },
        Repository {
            id: "repo-agent-eval-runner".into(),
            name: "agent-eval-runner".into(),
            workspace: "AI Tools".into(),
            local_path: "~/Documents/GitHub/agent-eval-runner".into(),
            remote_repo: "github.com/balveer/agent-eval-runner".into(),
            current_branch: "exp/multi-turn-bench".into(),
            default_branch: "main".into(),
            sync_state: RepoSyncState::Modified,
            ahead_count: 5,
            behind_count: 0,
            stash_count: 0,
            last_activity_label: "2h ago".into(),
            last_activity_minutes: 120,
            primary_language: "TypeScript".into(),
            session_note: "Building deterministic replay harness for tool-call suites".into(),
            uncommitted_files: vec![
                cf!("src/runner/replayHarness.ts", Modified, 105, 28, false),
                cf!("src/reporters/markdownSummary.ts", Added, 74, 0, false),
                cf!("src/types/trace.ts", Modified, 16, 3, true),
                cf!("suites/booking-flow.yaml", Untracked, 53, 0, false),
            ],
            branches: vec![
                branch!(
                    "exp/multi-turn-bench",
                    true,
                    false,
                    5,
                    0,
                    "Record tool latency distribution per step",
                    "2h ago"
                ),
                branch!(
                    "main",
                    false,
                    true,
                    0,
                    0,
                    "Initial CLI runner and JSON exporter",
                    "1w ago"
                ),
            ],
            recent_commits: vec![
                commit!(
                    "f03c91a",
                    "Record tool latency distribution per step",
                    "2h ago",
                    "exp/multi-turn-bench"
                ),
                commit!(
                    "d8821b0",
                    "Add assertion matcher for structured args",
                    "5h ago",
                    "exp/multi-turn-bench"
                ),
            ],
        },
        Repository {
            id: "repo-analytics-portal".into(),
            name: "client-analytics-portal".into(),
            workspace: "Web Apps".into(),
            local_path: "~/Documents/GitHub/client-analytics-portal".into(),
            remote_repo: "github.com/balveer/client-analytics-portal".into(),
            current_branch: "chore/tailwind-v4".into(),
            default_branch: "main".into(),
            sync_state: RepoSyncState::Modified,
            ahead_count: 0,
            behind_count: 0,
            stash_count: 1,
            last_activity_label: "3h ago".into(),
            last_activity_minutes: 180,
            primary_language: "React / TS".into(),
            session_note: "Migrating dashboard tokens and chart grid spacing".into(),
            uncommitted_files: vec![
                cf!("src/index.css", Modified, 22, 41, false),
                cf!("src/components/CohortTable.tsx", Modified, 14, 14, false),
                cf!("tailwind.config.js", Deleted, 0, 68, true),
            ],
            branches: vec![
                branch!(
                    "chore/tailwind-v4",
                    true,
                    false,
                    0,
                    0,
                    "Remove legacy PostCSS plugins",
                    "3h ago"
                ),
                branch!(
                    "main",
                    false,
                    true,
                    0,
                    0,
                    "Add CSV export for weekly retention",
                    "3d ago"
                ),
                branch!(
                    "feat/date-compare",
                    false,
                    false,
                    2,
                    1,
                    "Support custom comparison windows",
                    "6d ago"
                ),
            ],
            recent_commits: vec![
                commit!(
                    "4a12ef8",
                    "Remove legacy PostCSS plugins",
                    "3h ago",
                    "chore/tailwind-v4"
                ),
                commit!(
                    "99c10ab",
                    "Switch CSS entry to @import tailwindcss",
                    "4h ago",
                    "chore/tailwind-v4"
                ),
            ],
        },
        Repository {
            id: "repo-design-tokens".into(),
            name: "ui-design-tokens".into(),
            workspace: "Packages & CLI".into(),
            local_path: "~/Documents/GitHub/ui-design-tokens".into(),
            remote_repo: "github.com/balveer/ui-design-tokens".into(),
            current_branch: "release/v1.4".into(),
            default_branch: "main".into(),
            sync_state: RepoSyncState::Ahead,
            ahead_count: 2,
            behind_count: 0,
            stash_count: 0,
            last_activity_label: "5h ago".into(),
            last_activity_minutes: 300,
            primary_language: "JSON / TS".into(),
            session_note: "Ready to push release tag and publish npm package".into(),
            uncommitted_files: vec![],
            branches: vec![
                branch!(
                    "release/v1.4",
                    true,
                    false,
                    2,
                    0,
                    "Bump version to 1.4.0 and update changelog",
                    "5h ago"
                ),
                branch!(
                    "main",
                    false,
                    true,
                    0,
                    0,
                    "Merge neutral slate surface scale",
                    "2d ago"
                ),
            ],
            recent_commits: vec![
                commit!(
                    "e21b79c",
                    "Bump version to 1.4.0 and update changelog",
                    "5h ago",
                    "release/v1.4"
                ),
                commit!(
                    "b60811e",
                    "Regenerate CSS custom properties build",
                    "5h ago",
                    "release/v1.4"
                ),
            ],
        },
        Repository {
            id: "repo-chrome-tab-organizer".into(),
            name: "tab-cluster-ext".into(),
            workspace: "Web Apps".into(),
            local_path: "~/Documents/GitHub/tab-cluster-ext".into(),
            remote_repo: "github.com/balveer/tab-cluster-ext".into(),
            current_branch: "feat/keyboard-nav".into(),
            default_branch: "main".into(),
            sync_state: RepoSyncState::Ahead,
            ahead_count: 2,
            behind_count: 0,
            stash_count: 0,
            last_activity_label: "Yesterday".into(),
            last_activity_minutes: 1320,
            primary_language: "TypeScript".into(),
            session_note: "Command palette arrow-key navigation tested in Chrome".into(),
            uncommitted_files: vec![cf!(
                "src/popup/CommandList.tsx",
                Modified,
                19,
                6,
                true
            )],
            branches: vec![
                branch!(
                    "feat/keyboard-nav",
                    true,
                    false,
                    2,
                    0,
                    "Add vim-style j/k selection in popup",
                    "Yesterday"
                ),
                branch!(
                    "main",
                    false,
                    true,
                    0,
                    0,
                    "Fix manifest v3 service worker sleep",
                    "5d ago"
                ),
            ],
            recent_commits: vec![commit!(
                "2f190ac",
                "Add vim-style j/k selection in popup",
                "Yesterday",
                "feat/keyboard-nav"
            )],
        },
        Repository {
            id: "repo-commerce-storefront".into(),
            name: "atelier-storefront".into(),
            workspace: "Web Apps".into(),
            local_path: "~/Documents/GitHub/atelier-storefront".into(),
            remote_repo: "github.com/balveer/atelier-storefront".into(),
            current_branch: "main".into(),
            default_branch: "main".into(),
            sync_state: RepoSyncState::Clean,
            ahead_count: 0,
            behind_count: 0,
            stash_count: 0,
            last_activity_label: "2d ago".into(),
            last_activity_minutes: 2880,
            primary_language: "Next.js".into(),
            session_note: "Production deploy verified; working tree clean".into(),
            uncommitted_files: vec![],
            branches: vec![
                branch!(
                    "main",
                    true,
                    true,
                    0,
                    0,
                    "Optimize product gallery responsive srcset",
                    "2d ago"
                ),
                branch!(
                    "feat/wishlist-sync",
                    false,
                    false,
                    1,
                    3,
                    "Persist wishlist items in local storage",
                    "1w ago"
                ),
            ],
            recent_commits: vec![commit!(
                "77c43d1",
                "Optimize product gallery responsive srcset",
                "2d ago",
                "main"
            )],
        },
        Repository {
            id: "repo-docs-kb".into(),
            name: "engineering-notes-kb".into(),
            workspace: "Packages & CLI".into(),
            local_path: "~/Documents/GitHub/engineering-notes-kb".into(),
            remote_repo: "github.com/balveer/engineering-notes-kb".into(),
            current_branch: "main".into(),
            default_branch: "main".into(),
            sync_state: RepoSyncState::Clean,
            ahead_count: 0,
            behind_count: 0,
            stash_count: 0,
            last_activity_label: "4d ago".into(),
            last_activity_minutes: 5760,
            primary_language: "Markdown".into(),
            session_note: "Synced across devices via GitHub Desktop".into(),
            uncommitted_files: vec![],
            branches: vec![branch!(
                "main",
                true,
                true,
                0,
                0,
                "Add architecture decision log for queue worker",
                "4d ago"
            )],
            recent_commits: vec![commit!(
                "91bc042",
                "Add architecture decision log for queue worker",
                "4d ago",
                "main"
            )],
        },
    ]
}
