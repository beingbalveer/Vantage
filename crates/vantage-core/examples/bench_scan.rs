//! Engine-level benchmark used by `scripts/benchmark.sh`.
//!
//! The GPUI app cannot be driven headlessly without a window server, so this
//! binary measures the `vantage-core` work behind a dashboard scan:
//!
//! * `overview` times one `query_repo_status` per repository, then the status
//!   plus the branch, commit, changed-file, stash, identity and remote reads
//!   that `backend::repo_map::map_repo` performs for every row today.
//! * `detail` times those same stages for one repository, which is the engine
//!   cost of showing a repository's detail pane.
//!
//! It never reads Vantage's workspace state. The shell harness creates every
//! repository in a disposable directory and removes it afterwards.
//!
//! Output is tab-separated `key<TAB>value` pairs so the harness can record and
//! compare metrics without parsing prose. Timings are milliseconds.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use tokio::runtime::Runtime;
use vantage_core::git::{diff, log, refs, stash, status};
use vantage_core::{repos, GitRunner};

/// Commits the dashboard keeps in a row's history strip.
const COMMIT_LIMIT: u32 = 15;

const USAGE: &str = "\
bench_scan — engine benchmark used by scripts/benchmark.sh

USAGE:
    bench_scan overview [--iterations N] <repo>...
    bench_scan detail   [--iterations N] <repo>

OPTIONS:
    --iterations N   Measured runs, median is reported (default: 3)
    -h, --help       Print this help

Output is tab-separated `key<TAB>value` lines; times are milliseconds.
";

enum Mode {
    Overview,
    Detail,
}

struct Options {
    mode: Mode,
    iterations: usize,
    repos: Vec<PathBuf>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("bench_scan: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let options = match parse_args()? {
        Some(options) => options,
        None => return Ok(()),
    };

    for repo in &options.repos {
        if !repo.join(".git").exists() {
            return Err(format!("{} is not a Git work tree", repo.display()));
        }
    }

    let runtime = Runtime::new().map_err(|error| format!("start the runtime: {error}"))?;
    let runner = GitRunner::new();

    match options.mode {
        Mode::Overview => benchmark_overview(&runtime, &runner, &options.repos, options.iterations),
        Mode::Detail => benchmark_detail(&runtime, &runner, &options.repos[0], options.iterations),
    }
}

fn parse_args() -> Result<Option<Options>, String> {
    let mut args = std::env::args().skip(1);
    let Some(mode) = args.next() else {
        print!("{USAGE}");
        return Ok(None);
    };

    let mode = match mode.as_str() {
        "overview" => Mode::Overview,
        "detail" => Mode::Detail,
        "-h" | "--help" => {
            print!("{USAGE}");
            return Ok(None);
        }
        other => return Err(format!("unknown mode `{other}`\n\n{USAGE}")),
    };

    let mut iterations = 3usize;
    let mut repos = Vec::new();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--iterations" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--iterations needs a value".to_string())?;
                iterations = value
                    .parse()
                    .map_err(|_| format!("invalid iteration count `{value}`"))?;
                if iterations == 0 || iterations > 50 {
                    return Err("--iterations must be between 1 and 50".to_string());
                }
            }
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(None);
            }
            other if other.starts_with('-') => {
                return Err(format!("unknown option `{other}`\n\n{USAGE}"));
            }
            other => repos.push(PathBuf::from(other)),
        }
    }

    if repos.is_empty() {
        return Err(format!("at least one repository is required\n\n{USAGE}"));
    }
    if matches!(mode, Mode::Detail) && repos.len() != 1 {
        return Err(format!(
            "detail measures one repository at a time, got {}\n\n{USAGE}",
            repos.len()
        ));
    }

    Ok(Some(Options {
        mode,
        iterations,
        repos,
    }))
}

fn benchmark_overview(
    runtime: &Runtime,
    runner: &GitRunner,
    repos: &[PathBuf],
    iterations: usize,
) -> Result<(), String> {
    let mut status_runs = Vec::with_capacity(iterations);
    let mut full_runs = Vec::with_capacity(iterations);

    for _ in 0..iterations {
        let started = Instant::now();
        for (index, repo) in repos.iter().enumerate() {
            let id = format!("bench-{index}");
            let repo_status =
                runtime.block_on(status::query_repo_status(runner, &id, repo, None, None));
            if let Some(error) = repo_status.error {
                return Err(format!("{}: {error}", repo.display()));
            }
        }
        status_runs.push(started.elapsed().as_secs_f64() * 1_000.0);

        // Today `Backend::snapshot` maps every row eagerly, so a full overview
        // also pays for every detail read. Keep that number separate from the
        // status-only path so P1-03's lazy overview can be compared directly.
        let started = Instant::now();
        for repo in repos {
            measure_detail(runtime, runner, repo)?;
        }
        full_runs.push(started.elapsed().as_secs_f64() * 1_000.0);
    }

    metric("schema", "1");
    metric("mode", "overview");
    metric("repos", repos.len());
    metric("iterations", iterations);
    metric("overview_status_ms", format_ms(median(&status_runs)));
    metric("overview_status_min_ms", format_ms(min(&status_runs)));
    metric("overview_status_max_ms", format_ms(max(&status_runs)));
    metric("overview_full_ms", format_ms(median(&full_runs)));
    metric("overview_full_min_ms", format_ms(min(&full_runs)));
    metric("overview_full_max_ms", format_ms(max(&full_runs)));
    Ok(())
}

fn benchmark_detail(
    runtime: &Runtime,
    runner: &GitRunner,
    repo: &Path,
    iterations: usize,
) -> Result<(), String> {
    let mut runs = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        runs.push(measure_detail(runtime, runner, repo)?);
    }

    let totals: Vec<f64> = runs.iter().map(|run| run.total_ms).collect();
    let representative = median_run(&runs);

    metric("schema", "1");
    metric("mode", "detail");
    metric("repos", 1);
    metric("iterations", iterations);
    metric("detail_total_ms", format_ms(representative.total_ms));
    metric("detail_total_min_ms", format_ms(min(&totals)));
    metric("detail_total_max_ms", format_ms(max(&totals)));
    metric("detail_status_ms", format_ms(representative.status_ms));
    metric(
        "detail_default_branch_ms",
        format_ms(representative.default_branch_ms),
    );
    metric("detail_branches_ms", format_ms(representative.branches_ms));
    metric("detail_commits_ms", format_ms(representative.commits_ms));
    metric(
        "detail_changed_files_ms",
        format_ms(representative.changed_files_ms),
    );
    metric("detail_stash_ms", format_ms(representative.stash_ms));
    metric("detail_identity_ms", format_ms(representative.identity_ms));
    metric("detail_remote_ms", format_ms(representative.remote_ms));
    metric("detail_changed_files_count", representative.changed_files);
    metric("detail_patch_bytes", representative.patch_bytes);
    Ok(())
}

/// The measured run whose total is the median.
///
/// Deriving every stage from one run keeps the breakdown internally consistent:
/// taking an independent median per stage can make the stages sum to more than
/// the reported total when the machine is noisy.
fn median_run(runs: &[DetailTimes]) -> &DetailTimes {
    let mut order: Vec<usize> = (0..runs.len()).collect();
    order.sort_by(|&a, &b| {
        runs[a]
            .total_ms
            .partial_cmp(&runs[b].total_ms)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    &runs[order[order.len() / 2]]
}

struct DetailTimes {
    total_ms: f64,
    status_ms: f64,
    default_branch_ms: f64,
    branches_ms: f64,
    commits_ms: f64,
    changed_files_ms: f64,
    stash_ms: f64,
    identity_ms: f64,
    remote_ms: f64,
    changed_files: u32,
    patch_bytes: usize,
}

/// The engine work `backend::repo_map::map_repo` does for one row.
///
/// Keep this in step with `crates/vantage/src/backend/repo_map.rs` and
/// `details.rs`; it deliberately uses the same core functions and commands.
fn measure_detail(
    runtime: &Runtime,
    runner: &GitRunner,
    repo: &Path,
) -> Result<DetailTimes, String> {
    let total_started = Instant::now();

    let started = Instant::now();
    let repo_status = runtime.block_on(status::query_repo_status(
        runner,
        "bench-detail",
        repo,
        None,
        None,
    ));
    let status_ms = elapsed_ms(started);
    if let Some(error) = repo_status.error {
        return Err(format!("{}: {error}", repo.display()));
    }

    let started = Instant::now();
    let _default_branch = runtime.block_on(repos::default_branch(runner, repo));
    let default_branch_ms = elapsed_ms(started);

    let started = Instant::now();
    runtime
        .block_on(refs::list_branches(runner, repo))
        .map_err(|error| format!("{}: list branches: {error}", repo.display()))?;
    let tips = runtime
        .block_on(runner.run_read(
            repo,
            &[
                "for-each-ref",
                "--format=%(refname:short)\x1f%(committerdate:unix)",
                "refs/heads",
            ],
        ))
        .map_err(|error| format!("{}: branch tips: {error}", repo.display()))?;
    if !tips.success() {
        return Err(format!("{}: branch tips: {}", repo.display(), tips.stderr));
    }
    let _branch_count = tips.stdout_string().lines().count();
    let branches_ms = elapsed_ms(started);

    let started = Instant::now();
    let commit_branch = repo_status.branch.as_deref().unwrap_or("HEAD");
    runtime
        .block_on(log::get_log_page(
            runner,
            repo,
            Some(commit_branch),
            0,
            COMMIT_LIMIT,
            None,
            None,
        ))
        .map_err(|error| format!("{}: load commits: {error}", repo.display()))?;
    let commits_ms = elapsed_ms(started);

    let started = Instant::now();
    let (changed_files, patch_bytes) = runtime
        .block_on(scan_changed_files(runner, repo))
        .map_err(|error| format!("{}: changed files: {error}", repo.display()))?;
    let changed_files_ms = elapsed_ms(started);

    let started = Instant::now();
    runtime
        .block_on(stash::list_stashes(runner, repo))
        .map_err(|error| format!("{}: list stashes: {error}", repo.display()))?;
    let stash_ms = elapsed_ms(started);

    // These two match `details::commit_identity` / `details::has_remote`:
    // cheap config reads whose failure is represented by `None`, not an error.
    let started = Instant::now();
    let _identity = runtime
        .block_on(runner.run_read(repo, &["config", "--get-regexp", "^user\\.(name|email)$"]));
    let identity_ms = elapsed_ms(started);

    let started = Instant::now();
    let _remote =
        runtime.block_on(runner.run_read(repo, &["config", "--get", "remote.origin.url"]));
    let remote_ms = elapsed_ms(started);

    Ok(DetailTimes {
        total_ms: elapsed_ms(total_started),
        status_ms,
        default_branch_ms,
        branches_ms,
        commits_ms,
        changed_files_ms,
        stash_ms,
        identity_ms,
        remote_ms,
        changed_files,
        patch_bytes,
    })
}

/// The working-tree commands behind `details::changed_files`.
///
/// It deliberately keeps both the raw `Vec<u8>` and the decoded `String` of
/// each patch alive until the block scan finishes, because that is the peak
/// memory the app pays for a large `git diff` today.
async fn scan_changed_files(runner: &GitRunner, repo: &Path) -> Result<(u32, usize), String> {
    let output = runner
        .run_read(
            repo,
            &["status", "--porcelain=v2", "-z", "--untracked-files=all"],
        )
        .await
        .map_err(|error| error.to_string())?;
    if !output.success() {
        return Err(output.stderr);
    }
    let parsed = status::parse_porcelain_v2(&output.stdout);
    if parsed.dirty_count == 0 {
        return Ok((0, 0));
    }

    for args in [
        &["diff", "--numstat", "-M", "-z"][..],
        &["diff", "--numstat", "-M", "-z", "--cached"][..],
    ] {
        let output = runner
            .run_read(repo, args)
            .await
            .map_err(|error| error.to_string())?;
        if output.success() {
            let _changes = diff::parse_numstat_z(&output.stdout);
        }
    }

    let mut patch_bytes = 0usize;
    for args in [
        &["diff", "--no-color", "--no-ext-diff"][..],
        &["diff", "--no-color", "--no-ext-diff", "--cached"][..],
    ] {
        let output = runner
            .run_read(repo, args)
            .await
            .map_err(|error| error.to_string())?;
        if output.success() {
            // Both buffers stay alive through the scan, matching the app's
            // peak while it turns a patch into per-file previews.
            let text = output.stdout_string();
            patch_bytes += text.len();
            let _blocks = count_patch_blocks(&text);
        }
    }

    Ok((parsed.dirty_count, patch_bytes))
}

fn count_patch_blocks(patch: &str) -> usize {
    patch
        .lines()
        .filter(|line| line.starts_with("diff --git ") || line.starts_with("diff --cc "))
        .count()
}

fn elapsed_ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1_000.0
}

fn metric(key: &str, value: impl std::fmt::Display) {
    println!("{key}\t{value}");
}

fn format_ms(value: f64) -> String {
    format!("{value:.3}")
}

fn min(values: &[f64]) -> f64 {
    values.iter().copied().fold(f64::INFINITY, f64::min)
}

fn max(values: &[f64]) -> f64 {
    values.iter().copied().fold(f64::NEG_INFINITY, f64::max)
}

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let middle = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        (sorted[middle - 1] + sorted[middle]) / 2.0
    } else {
        sorted[middle]
    }
}
