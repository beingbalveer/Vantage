use crate::error::AppError;
use crate::git::runner::GitRunner;
use crate::state::{DiffFile, DiffLine, DiffScope, FileChange, FileStatus, Hunk, LineKind};
use similar::{ChangeTag, TextDiff};
use std::collections::HashMap;
use std::path::Path;

pub fn parse_numstat_z(output: &[u8]) -> Vec<FileChange> {
    let mut changes = Vec::new();
    let parts: Vec<&[u8]> = output.split(|&b| b == 0).collect();

    let mut i = 0;
    while i < parts.len() {
        let part = parts[i];
        if part.is_empty() {
            i += 1;
            continue;
        }

        let line = String::from_utf8_lossy(part);
        let tokens: Vec<&str> = line.split('\t').collect();
        if tokens.len() >= 3 {
            let add_str = tokens[0];
            let del_str = tokens[1];
            let path_token = tokens[2];

            let binary = add_str == "-" || del_str == "-";
            let additions = add_str.parse().unwrap_or(0);
            let deletions = del_str.parse().unwrap_or(0);

            // In git diff --numstat -M -z, a rename record has an empty path in tokens[2],
            // followed by the old path and new path as the next two null-separated fields:
            // "<add>\t<del>\t\0<old_path>\0<new_path>\0"
            if path_token.is_empty() && i + 2 < parts.len() {
                let old_p = String::from_utf8_lossy(parts[i + 1]).to_string();
                let new_p = String::from_utf8_lossy(parts[i + 2]).to_string();
                changes.push(FileChange {
                    path: new_p,
                    old_path: Some(old_p),
                    status: FileStatus::Renamed,
                    additions,
                    deletions,
                    binary,
                    old_mode: None,
                    new_mode: None,
                });
                i += 3;
                continue;
            }

            // Do NOT infer Added purely based on additions > 0 && deletions == 0.
            // Edits that only add lines to an existing file must be marked Modified.
            let status = if additions == 0 && deletions > 0 {
                FileStatus::Deleted
            } else {
                FileStatus::Modified
            };

            changes.push(FileChange {
                path: path_token.to_string(),
                old_path: None,
                status,
                additions,
                deletions,
                binary,
                old_mode: None,
                new_mode: None,
            });
        }
        i += 1;
    }

    changes
}

pub fn parse_name_status_z(output: &[u8]) -> HashMap<String, (FileStatus, Option<String>)> {
    let mut map = HashMap::new();
    let parts: Vec<&[u8]> = output.split(|&b| b == 0).collect();
    let mut i = 0;
    while i < parts.len() {
        if parts[i].is_empty() {
            i += 1;
            continue;
        }
        let status_str = String::from_utf8_lossy(parts[i]);
        let first_char = status_str.chars().next().unwrap_or(' ');
        match first_char {
            'M' if i + 1 < parts.len() => {
                let path = String::from_utf8_lossy(parts[i + 1]).to_string();
                map.insert(path, (FileStatus::Modified, None));
                i += 2;
            }
            'A' if i + 1 < parts.len() => {
                let path = String::from_utf8_lossy(parts[i + 1]).to_string();
                map.insert(path, (FileStatus::Added, None));
                i += 2;
            }
            'D' if i + 1 < parts.len() => {
                let path = String::from_utf8_lossy(parts[i + 1]).to_string();
                map.insert(path, (FileStatus::Deleted, None));
                i += 2;
            }
            'R' if i + 2 < parts.len() => {
                let old_p = String::from_utf8_lossy(parts[i + 1]).to_string();
                let new_p = String::from_utf8_lossy(parts[i + 2]).to_string();
                map.insert(new_p, (FileStatus::Renamed, Some(old_p)));
                i += 3;
            }
            'C' if i + 2 < parts.len() => {
                let old_p = String::from_utf8_lossy(parts[i + 1]).to_string();
                let new_p = String::from_utf8_lossy(parts[i + 2]).to_string();
                map.insert(new_p, (FileStatus::Copied, Some(old_p)));
                i += 3;
            }
            'T' if i + 1 < parts.len() => {
                let path = String::from_utf8_lossy(parts[i + 1]).to_string();
                map.insert(path, (FileStatus::TypeChanged, None));
                i += 2;
            }
            'U' if i + 1 < parts.len() => {
                let path = String::from_utf8_lossy(parts[i + 1]).to_string();
                map.insert(path, (FileStatus::Conflicted, None));
                i += 2;
            }
            _ => {
                i += 1;
            }
        }
    }
    map
}

pub fn merge_changes_with_status(
    mut changes: Vec<FileChange>,
    status_map: HashMap<String, (FileStatus, Option<String>)>,
) -> Vec<FileChange> {
    for c in &mut changes {
        if let Some((st, old_p)) = status_map.get(&c.path) {
            c.status = *st;
            if old_p.is_some() {
                c.old_path = old_p.clone();
            }
        }
    }

    let existing_paths: std::collections::HashSet<String> =
        changes.iter().map(|c| c.path.clone()).collect();
    for (path, (status, old_path)) in status_map {
        if !existing_paths.contains(&path) {
            changes.push(FileChange {
                path,
                old_path,
                status,
                additions: 0,
                deletions: 0,
                binary: false,
                old_mode: None,
                new_mode: None,
            });
        }
    }

    changes
}

pub fn parse_unified_diff(diff_text: &str, change: FileChange, force: bool) -> DiffFile {
    let total_lines = diff_text.lines().count();
    let total_bytes = diff_text.len();

    // Check budget thresholds: > 2000 lines or > 1 MB
    if !force && (total_lines > 2000 || total_bytes > 1_000_000) {
        return DiffFile {
            change,
            hunks: Vec::new(),
            truncated: true,
        };
    }

    let mut hunks = Vec::new();
    let mut current_hunk: Option<Hunk> = None;
    let mut old_line_counter = 0;
    let mut new_line_counter = 0;

    for line in diff_text.lines() {
        if line.starts_with("@@ ") {
            if let Some(h) = current_hunk.take() {
                hunks.push(h);
            }

            // Parse @@ -old_start,old_lines +new_start,new_lines @@ header
            let (old_start, old_lines, new_start, new_lines, header) = parse_hunk_header(line);
            old_line_counter = old_start;
            new_line_counter = new_start;

            current_hunk = Some(Hunk {
                old_start,
                old_lines,
                new_start,
                new_lines,
                header,
                lines: Vec::new(),
            });
        } else if let Some(ref mut hunk) = current_hunk {
            if let Some(first_char) = line.chars().next() {
                match first_char {
                    '+' => {
                        let text = line[1..].to_string();
                        hunk.lines.push(DiffLine {
                            kind: LineKind::Add,
                            old_no: None,
                            new_no: Some(new_line_counter),
                            text,
                            word_ranges: None,
                        });
                        new_line_counter += 1;
                    }
                    '-' => {
                        let text = line[1..].to_string();
                        hunk.lines.push(DiffLine {
                            kind: LineKind::Del,
                            old_no: Some(old_line_counter),
                            new_no: None,
                            text,
                            word_ranges: None,
                        });
                        old_line_counter += 1;
                    }
                    ' ' => {
                        let text = line[1..].to_string();
                        hunk.lines.push(DiffLine {
                            kind: LineKind::Context,
                            old_no: Some(old_line_counter),
                            new_no: Some(new_line_counter),
                            text,
                            word_ranges: None,
                        });
                        old_line_counter += 1;
                        new_line_counter += 1;
                    }
                    '\\' => {
                        // \ No newline at end of file
                        hunk.lines.push(DiffLine {
                            kind: LineKind::NoNewline,
                            old_no: None,
                            new_no: None,
                            text: line.to_string(),
                            word_ranges: None,
                        });
                    }
                    _ => {}
                }
            }
        }
    }

    if let Some(h) = current_hunk {
        hunks.push(h);
    }

    // Compute intra-line word diffs
    for hunk in &mut hunks {
        compute_intra_line_diffs(&mut hunk.lines);
    }

    DiffFile {
        change,
        hunks,
        truncated: false,
    }
}

fn parse_hunk_header(line: &str) -> (u32, u32, u32, u32, String) {
    // format: @@ -a,b +c,d @@ context
    let mut old_start = 1;
    let mut old_lines = 1;
    let mut new_start = 1;
    let mut new_lines = 1;
    let mut header = String::new();

    let parts: Vec<&str> = line.split("@@").collect();
    if parts.len() >= 3 {
        let range_part = parts[1].trim();
        header = parts[2..].join("@@").trim().to_string();

        for token in range_part.split_whitespace() {
            if let Some(old_spec) = token.strip_prefix('-') {
                let nums: Vec<&str> = old_spec.split(',').collect();
                old_start = nums[0].parse().unwrap_or(1);
                if nums.len() > 1 {
                    old_lines = nums[1].parse().unwrap_or(1);
                }
            } else if let Some(new_spec) = token.strip_prefix('+') {
                let nums: Vec<&str> = new_spec.split(',').collect();
                new_start = nums[0].parse().unwrap_or(1);
                if nums.len() > 1 {
                    new_lines = nums[1].parse().unwrap_or(1);
                }
            }
        }
    }

    (old_start, old_lines, new_start, new_lines, header)
}

fn compute_intra_line_diffs(lines: &mut [DiffLine]) {
    let mut i = 0;
    while i < lines.len() {
        if lines[i].kind == LineKind::Del
            && i + 1 < lines.len()
            && lines[i + 1].kind == LineKind::Add
        {
            let del_text = &lines[i].text;
            let add_text = &lines[i + 1].text;

            if del_text.len() < 500 && add_text.len() < 500 {
                let diff = TextDiff::from_words(del_text, add_text);
                let mut del_ranges = Vec::new();
                let mut add_ranges = Vec::new();

                let mut del_pos = 0u32;
                let mut add_pos = 0u32;

                for change in diff.iter_all_changes() {
                    let len = change.value().len() as u32;
                    match change.tag() {
                        ChangeTag::Delete => {
                            del_ranges.push((del_pos, del_pos + len));
                            del_pos += len;
                        }
                        ChangeTag::Insert => {
                            add_ranges.push((add_pos, add_pos + len));
                            add_pos += len;
                        }
                        ChangeTag::Equal => {
                            del_pos += len;
                            add_pos += len;
                        }
                    }
                }

                if !del_ranges.is_empty() {
                    lines[i].word_ranges = Some(del_ranges);
                }
                if !add_ranges.is_empty() {
                    lines[i + 1].word_ranges = Some(add_ranges);
                }
            }
            i += 2;
        } else {
            i += 1;
        }
    }
}

pub async fn get_scope_files(
    runner: &GitRunner,
    repo_path: &Path,
    scope: &DiffScope,
    ignore_whitespace: bool,
) -> Result<Vec<FileChange>, AppError> {
    match scope {
        DiffScope::Commit { sha } => {
            let mut numstat_args = vec!["show", "--numstat", "-M", "--root", "--format=", "-z"];
            let mut status_args = vec!["show", "--name-status", "-M", "--root", "--format=", "-z"];
            if ignore_whitespace {
                numstat_args.push("-w");
                status_args.push("-w");
            }
            numstat_args.push(sha);
            status_args.push(sha);

            let numstat_out = runner.run_read(repo_path, &numstat_args).await?;
            if !numstat_out.success() {
                return Err(AppError::other(numstat_out.stderr));
            }
            let changes = parse_numstat_z(&numstat_out.stdout);

            let status_map = match runner.run_read(repo_path, &status_args).await {
                Ok(o) if o.success() => parse_name_status_z(&o.stdout),
                _ => HashMap::new(),
            };

            Ok(merge_changes_with_status(changes, status_map))
        }
        DiffScope::Range { from, to } => {
            let mut numstat_args = vec!["diff", "--numstat", "-M", "-z"];
            let mut status_args = vec!["diff", "--name-status", "-M", "-z"];
            if ignore_whitespace {
                numstat_args.push("-w");
                status_args.push("-w");
            }
            numstat_args.push(from);
            numstat_args.push(to);
            status_args.push(from);
            status_args.push(to);

            let numstat_out = runner.run_read(repo_path, &numstat_args).await?;
            if !numstat_out.success() {
                return Err(AppError::other(numstat_out.stderr));
            }
            let changes = parse_numstat_z(&numstat_out.stdout);

            let status_map = match runner.run_read(repo_path, &status_args).await {
                Ok(o) if o.success() => parse_name_status_z(&o.stdout),
                _ => HashMap::new(),
            };

            Ok(merge_changes_with_status(changes, status_map))
        }
        DiffScope::Working => {
            let mut numstat_args = vec!["diff", "--numstat", "-M", "-z"];
            let mut status_args = vec!["diff", "--name-status", "-M", "-z"];
            if ignore_whitespace {
                numstat_args.push("-w");
                status_args.push("-w");
            }
            numstat_args.push("HEAD");
            status_args.push("HEAD");

            let changes = match runner.run_read(repo_path, &numstat_args).await {
                Ok(o) if o.success() => parse_numstat_z(&o.stdout),
                _ => Vec::new(),
            };

            let status_map = match runner.run_read(repo_path, &status_args).await {
                Ok(o) if o.success() => parse_name_status_z(&o.stdout),
                _ => HashMap::new(),
            };

            let mut merged_changes = merge_changes_with_status(changes, status_map);

            // Add untracked files
            if let Ok(untracked_out) = runner
                .run_read(
                    repo_path,
                    &["status", "--porcelain=v2", "-z", "--untracked-files=all"],
                )
                .await
            {
                if untracked_out.success() {
                    for part in untracked_out.stdout.split(|&b| b == 0) {
                        if part.starts_with(b"? ") {
                            let path = String::from_utf8_lossy(&part[2..]).to_string();
                            merged_changes.push(FileChange {
                                path,
                                old_path: None,
                                status: FileStatus::Untracked,
                                additions: 0,
                                deletions: 0,
                                binary: false,
                                old_mode: None,
                                new_mode: None,
                            });
                        }
                    }
                }
            }
            Ok(merged_changes)
        }
    }
}

pub async fn get_scope_file_diff(
    runner: &GitRunner,
    repo_path: &Path,
    scope: &DiffScope,
    path: &str,
    context_lines: u32,
    ignore_whitespace: bool,
    force: bool,
) -> Result<DiffFile, AppError> {
    let ctx_flag = format!("-U{}", context_lines);
    let mut args = vec!["-M", &ctx_flag, "--no-color", "--no-ext-diff"];
    if ignore_whitespace {
        args.push("-w");
    }

    let out = match scope {
        DiffScope::Commit { sha } => {
            let mut show_args = vec!["show", "--format="];
            show_args.extend_from_slice(&args);
            show_args.push(sha);
            show_args.push("--");
            show_args.push(path);
            runner.run_read(repo_path, &show_args).await?
        }
        DiffScope::Range { from, to } => {
            let mut range_args = vec!["diff"];
            range_args.extend_from_slice(&args);
            range_args.push(from);
            range_args.push(to);
            range_args.push("--");
            range_args.push(path);
            runner.run_read(repo_path, &range_args).await?
        }
        DiffScope::Working => {
            // Check if file is untracked
            let full_file_path = repo_path.join(path);
            let mut work_args = vec!["diff"];
            work_args.extend_from_slice(&args);
            work_args.push("HEAD");
            work_args.push("--");
            work_args.push(path);

            let res = runner.run_read(repo_path, &work_args).await?;
            if res.stdout.is_empty() && full_file_path.exists() {
                // Try untracked diff: git diff --no-index -- /dev/null <path>
                let null_path = if cfg!(windows) { "NUL" } else { "/dev/null" };
                runner
                    .run_read(
                        repo_path,
                        &["diff", "--no-index", "--no-color", null_path, path],
                    )
                    .await?
            } else {
                res
            }
        }
    };

    let diff_text = out.stdout_string();
    let change = FileChange {
        path: path.to_string(),
        old_path: None,
        status: FileStatus::Modified,
        additions: 0,
        deletions: 0,
        binary: diff_text.contains("Binary files"),
        old_mode: None,
        new_mode: None,
    };

    Ok(parse_unified_diff(&diff_text, change, force))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_unified_diff_with_intra_line() {
        let raw_diff = "@@ -10,3 +10,3 @@ function test()\n-const ttl = 3600\n+const ttl = 900\n return ttl;\n";
        let change = FileChange {
            path: "test.ts".to_string(),
            old_path: None,
            status: FileStatus::Modified,
            additions: 1,
            deletions: 1,
            binary: false,
            old_mode: None,
            new_mode: None,
        };

        let diff_file = parse_unified_diff(raw_diff, change, false);
        assert_eq!(diff_file.hunks.len(), 1);
        let hunk = &diff_file.hunks[0];
        assert_eq!(hunk.old_start, 10);
        assert_eq!(hunk.new_start, 10);
        assert_eq!(hunk.lines.len(), 3);
        assert_eq!(hunk.lines[0].kind, LineKind::Del);
        assert_eq!(hunk.lines[1].kind, LineKind::Add);
        assert_eq!(hunk.lines[2].kind, LineKind::Context);

        // Word diff was computed for del and add
        assert!(hunk.lines[0].word_ranges.is_some());
        assert!(hunk.lines[1].word_ranges.is_some());
    }

    #[test]
    fn test_parse_unified_diff_truncated() {
        let raw_diff = (0..2005)
            .map(|i| format!("+line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let change = FileChange {
            path: "big.txt".to_string(),
            old_path: None,
            status: FileStatus::Added,
            additions: 2005,
            deletions: 0,
            binary: false,
            old_mode: None,
            new_mode: None,
        };

        let diff_file = parse_unified_diff(&raw_diff, change, false);
        assert!(diff_file.truncated);
        assert!(diff_file.hunks.is_empty());
    }

    #[test]
    fn test_diff_scope_deserialization() {
        let json = r#"{"type":"commit","sha":"d4e8257"}"#;
        let scope: DiffScope = serde_json::from_str(json).expect("failed to deserialize");
        match scope {
            DiffScope::Commit { sha } => assert_eq!(sha, "d4e8257"),
            _ => panic!("Expected DiffScope::Commit"),
        }
    }

    struct TempTestRepo {
        path: std::path::PathBuf,
    }

    impl TempTestRepo {
        async fn new(runner: &GitRunner) -> Self {
            let path =
                std::env::temp_dir().join(format!("vantage_diff_test_{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&path).unwrap();

            runner.run_write(&path, &["init"], 10).await.unwrap();
            runner
                .run_write(&path, &["config", "user.name", "Vantage Test"], 10)
                .await
                .unwrap();
            runner
                .run_write(&path, &["config", "user.email", "vantage@test.local"], 10)
                .await
                .unwrap();
            runner
                .run_write(&path, &["config", "commit.gpgsign", "false"], 10)
                .await
                .unwrap();

            Self { path }
        }
    }

    impl Drop for TempTestRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[tokio::test]
    async fn test_get_scope_files_for_commits() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;

        // Root commit
        let f1 = repo.path.join("file1.txt");
        std::fs::write(&f1, "hello\n").unwrap();
        runner
            .run_write(&repo.path, &["add", "file1.txt"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["commit", "-m", "root commit"], 10)
            .await
            .unwrap();

        let root_sha = runner
            .run_read(&repo.path, &["rev-parse", "HEAD"])
            .await
            .unwrap()
            .stdout_string()
            .trim()
            .to_string();

        let scope_root = DiffScope::Commit { sha: root_sha };
        let files = get_scope_files(&runner, &repo.path, &scope_root, false)
            .await
            .expect("root commit files");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "file1.txt");

        // Second commit
        let f2 = repo.path.join("file2.txt");
        std::fs::write(&f2, "world\n").unwrap();
        runner
            .run_write(&repo.path, &["add", "file2.txt"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["commit", "-m", "second commit"], 10)
            .await
            .unwrap();

        let scope_head = DiffScope::Commit {
            sha: "HEAD".to_string(),
        };
        let head_files = get_scope_files(&runner, &repo.path, &scope_head, false)
            .await
            .expect("head commit files");
        assert_eq!(head_files.len(), 1);
        assert_eq!(head_files[0].path, "file2.txt");
    }

    #[test]
    fn test_parse_numstat_z_additions_only_is_modified() {
        // Output from git diff --numstat -z for an edit with only additions to an existing file
        let output = b"15\t0\texisting.rs\0";
        let changes = parse_numstat_z(output);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].path, "existing.rs");
        assert_eq!(changes[0].additions, 15);
        assert_eq!(changes[0].deletions, 0);
        // Must NOT be inferred as Added!
        assert_eq!(changes[0].status, FileStatus::Modified);
    }

    #[test]
    fn test_parse_numstat_z_renames() {
        // In git diff --numstat -M -z, rename is "<add>\t<del>\t\0<old>\0<new>\0"
        let output = b"2\t1\t\0src/old.rs\0src/new.rs\0";
        let changes = parse_numstat_z(output);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].path, "src/new.rs");
        assert_eq!(changes[0].old_path, Some("src/old.rs".to_string()));
        assert_eq!(changes[0].status, FileStatus::Renamed);
        assert_eq!(changes[0].additions, 2);
        assert_eq!(changes[0].deletions, 1);
    }

    #[test]
    fn test_parse_name_status_z_and_merge() {
        let name_status_raw = b"M\0mod.rs\0A\0new.rs\0D\0del.rs\0R100\0prev.rs\0renamed.rs\0";
        let status_map = parse_name_status_z(name_status_raw);

        assert_eq!(
            status_map.get("mod.rs"),
            Some(&(FileStatus::Modified, None))
        );
        assert_eq!(status_map.get("new.rs"), Some(&(FileStatus::Added, None)));
        assert_eq!(status_map.get("del.rs"), Some(&(FileStatus::Deleted, None)));
        assert_eq!(
            status_map.get("renamed.rs"),
            Some(&(FileStatus::Renamed, Some("prev.rs".to_string())))
        );

        let numstat_raw =
            b"10\t0\tmod.rs\025\t0\tnew.rs\00\t8\tdel.rs\05\t2\t\0prev.rs\0renamed.rs\0";
        let initial_changes = parse_numstat_z(numstat_raw);
        let merged = merge_changes_with_status(initial_changes, status_map);

        assert_eq!(merged.len(), 4);
        let mod_change = merged.iter().find(|c| c.path == "mod.rs").unwrap();
        assert_eq!(mod_change.status, FileStatus::Modified);
        assert_eq!(mod_change.additions, 10);

        let new_change = merged.iter().find(|c| c.path == "new.rs").unwrap();
        assert_eq!(new_change.status, FileStatus::Added);
        assert_eq!(new_change.additions, 25);

        let del_change = merged.iter().find(|c| c.path == "del.rs").unwrap();
        assert_eq!(del_change.status, FileStatus::Deleted);
        assert_eq!(del_change.deletions, 8);

        let ren_change = merged.iter().find(|c| c.path == "renamed.rs").unwrap();
        assert_eq!(ren_change.status, FileStatus::Renamed);
        assert_eq!(ren_change.old_path, Some("prev.rs".to_string()));
    }
}
