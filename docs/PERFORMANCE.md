# Performance budgets

This document records Vantage's reproducible performance harness, the baseline
measured on the reference machine, and the regression budgets enforced by
`scripts/benchmark.sh --check`.

The harness covers the Git engine and build cost. It deliberately does not
launch the GPUI app: it creates disposable repositories and calls the same
`vantage-core` functions the dashboard uses. Window startup, GPUI layout/paint
time and avatar network fetches are out of scope.

## Reference machine

| | |
|---|---|
| Date | 2026-11-16 |
| Machine | Apple M2, 8 cores, 16 GiB RAM, APFS SSD |
| OS | macOS 27.0.1 |
| Toolchain | rustc 1.100.0, cargo 1.100.0, git 2.54.0 |
| Profile | workspace `dev` profile (`opt-level = 1`, line tables) |

Times are medians of three iterations unless noted; the large working-tree
fixture and the cold/warm Cargo checks are single runs. Process startup and
filesystem cache make the figures noisy on a busy machine; compare runs on the
same hardware rather than across machines.

## Running the harness

```bash
# Engine metrics: overview scans, time-to-detail, peak memory.
scripts/benchmark.sh

# Add cold/warm `cargo check` and target-size budgets. This compiles the
# workspace in a disposable target directory and takes several minutes.
scripts/benchmark.sh --with-cargo

# Fail (exit non-zero) when a budget is exceeded.
scripts/benchmark.sh --check
scripts/benchmark.sh --check --with-cargo

# Useful options.
scripts/benchmark.sh --iterations 5 --repo-counts "5 20 50"
scripts/benchmark.sh --out /tmp/vantage-perf.tsv
scripts/benchmark.sh --keep          # keep the fixture for inspection
scripts/benchmark.sh --dry-run       # describe the run without creating anything
```

Metrics are tab-separated `key<TAB>value` lines on stdout; progress and section
headers go to stderr. The `--out` file is a clean copy for attaching to a review
or a scheduled-job artifact. On Linux, `--with-cargo` needs the same system
packages as the CI `lint-and-test` job because it checks the GPUI crate.

The script never touches Vantage's workspace state, the user's Git config, or the
repository's `target/` directory:

- every repository lives under a `mktemp -d` fixture and is removed on exit;
- Git runs with an isolated `GIT_CONFIG_GLOBAL`, `GIT_CONFIG_NOSYSTEM=1` and a
  hookless config, so the benchmark cannot pick up host hooks or signing keys;
- the benchmark binary and the optional `cargo check` target live in the fixture
  directory;
- `state.json` is never read because the harness calls `vantage-core` directly,
  not `Backend` or the app.

`--keep` prints the fixture path and leaves it in place for inspection.

## Metrics

| Key | Unit | What it measures |
|---|---|---|
| `cargo_check_cold_s` | seconds | `cargo check --workspace --locked` with a fresh target directory. |
| `cargo_check_warm_s` | seconds | The same check immediately afterwards, reusing that target directory. |
| `cargo_target_size_mib` | MiB | `du -sm` of the disposable target directory after the warm check. This is dev build state, not release artifact size. |
| `overview_N_repos` | count | Number of repositories in the scan. |
| `overview_N_status_ms` | ms | Sum of one `query_repo_status` per repository. This is the cheapest existing per-row path and the baseline that P1-03's one-process overview is measured against. |
| `overview_N_full_ms` | ms | Status plus every detail read that `Backend::snapshot` performs per row today: default branch, branches, 15 commits, working-tree scan, stash count, identity and remote. |
| `detail_total_ms` | ms | One repository's full detail path, broken down by the `detail_*_ms` stage metrics. |
| `detail_status_ms` | ms | Status query inside the detail path. |
| `detail_default_branch_ms` | ms | `repos::default_branch`. |
| `detail_branches_ms` | ms | Local branch list plus branch-tip metadata. |
| `detail_commits_ms` | ms | The 15-commit history page. |
| `detail_changed_files_ms` | ms | Working-tree status and, for a dirty tree, both numstat scans and both full patches. |
| `detail_stash_ms` | ms | Stash list. |
| `detail_identity_ms` | ms | Commit identity config read. |
| `detail_remote_ms` | ms | `origin` remote config read. |
| `detail_changed_files_count` | count | Files reported by the working-tree scan. |
| `detail_patch_bytes` | bytes | Bytes of unstaged plus staged patch text held while building previews. |
| `detail_large_ms` | ms | Detail path for the 100-changed-file / ~10 MB patch fixture. |
| `detail_large_changed_files` | count | Changed files in that fixture (100). |
| `detail_large_patch_bytes` | bytes | Patch bytes in that fixture (~10.8 MB). |
| `memory_peak_rss_mib` | MiB | Peak RSS of the large-detail process, from `/usr/bin/time -l` on macOS or `/usr/bin/time -v` on Linux. |

`overview_N_full_ms` is the number that matters for the current UI: `snapshot()`
eagerly loads details for every row, so opening the dashboard pays the full
cost. `overview_N_status_ms` is tracked separately so the effect of P1-03's
status-only overview and lazy detail can be seen immediately. Today, selecting a
repository is mostly a model clone because its details are already in the
snapshot; `detail_total_ms` is therefore the cost that lazy loading will move to
selection.

The benchmark example mirrors the engine sequence in
`crates/vantage/src/backend/repo_map.rs` and `backend/details.rs`. If those paths
gain or lose a Git read, update `crates/vantage-core/examples/bench_scan.rs` in
the same change.

## Baseline

Measured with `scripts/benchmark.sh` (three iterations) and the large fixture run.

### Build

| Metric | Baseline | Budget |
|---|---:|---:|
| `cargo_check_cold_s` | 118.2 s | 300 s |
| `cargo_check_warm_s` | 1.0 s | 30 s |
| `cargo_target_size_mib` | 792 MiB | 2048 MiB |

### Overview

| Repositories | Status-only | Full snapshot | Full budget |
|---:|---:|---:|---:|
| 5 | 420 ms | 981 ms | 4,000 ms |
| 20 | 2,355 ms | 6,697 ms | 16,000 ms |
| 50 | 5,391 ms | 12,620 ms | 35,000 ms |

The first iteration of a run is often slower than later ones, and a run taken
while the machine was busier measured 2,785 / 12,613 / 26,635 ms for the
5/20/50-repository full scans (the 50-repository maximum was 32.8 s). That
spread is why the ceilings are deliberately above the recorded medians. The
high cost is the problem P1-03 addresses: each row currently runs 15–17 Git
processes.

### Time-to-detail

| Stage | Baseline |
|---|---:|
| Status | 77 ms |
| Default branch | 26 ms |
| Branches | 30 ms |
| Commits (15) | 19 ms |
| Changed files | 16 ms |
| Stashes | 15 ms |
| Identity | 14 ms |
| Remote | 14 ms |
| **Total** | **210 ms** |

Stage values come from the measured run whose total is the median, so they add
up to the reported total instead of being independent medians. A busier run
measured a 731 ms total, which the 900 ms budget still covers. Clean
repositories skip the numstat and patch commands, so
`detail_changed_files_ms` is close to one `git status` on this fixture.

### Large working tree and memory

| Metric | Baseline |
|---|---:|
| Changed files | 100 |
| Patch bytes | 10,829,617 (~10.3 MiB) |
| `detail_large_ms` | 683 ms |
| Peak RSS | 34.3 MiB |

The peak covers the benchmark process, which holds the raw stdout buffer and the
decoded patch string for the unstaged and staged diffs. It does not add the
memory of Git child processes. The measured figure is well below the budget and
is a guard against reintroducing an unbounded second copy of a large patch.

## Regression budgets

`scripts/benchmark.sh --check` compares the metrics it measured with these
ceilings. A missing metric (for example `cargo_check_cold_s` when `--with-cargo`
was not passed) is skipped, not treated as a failure.

| Environment variable | Default | Metric |
|---|---:|---|
| `PERF_MAX_COLD_CHECK_S` | 300 | `cargo_check_cold_s` |
| `PERF_MAX_WARM_CHECK_S` | 30 | `cargo_check_warm_s` |
| `PERF_MAX_TARGET_MIB` | 2048 | `cargo_target_size_mib` |
| `PERF_MAX_OVERVIEW_5_MS` | 4000 | `overview_5_full_ms` |
| `PERF_MAX_OVERVIEW_20_MS` | 16000 | `overview_20_full_ms` |
| `PERF_MAX_OVERVIEW_50_MS` | 35000 | `overview_50_full_ms` |
| `PERF_MAX_DETAIL_MS` | 900 | `detail_total_ms` |
| `PERF_MAX_DETAIL_LARGE_MS` | 1500 | `detail_large_ms` |
| `PERF_MAX_RSS_MIB` | 128 | `memory_peak_rss_mib` |

These are regression ceilings for the current implementation, not product
targets. They were set to cover the slower runs observed while establishing the
baseline (for example, the 50-repository full scan reached 32.8 s once) while
still failing a clear step change. They are not fixed multipliers. When P1-03
lands, re-measure and tighten the overview and detail budgets rather than
carrying the old ceilings forward.

Absolute timings differ on CI runners. A scheduled job should set the variables
for its own hardware, for example:

```bash
PERF_MAX_OVERVIEW_50_MS=90000 \
PERF_MAX_DETAIL_MS=3000 \
PERF_MAX_DETAIL_LARGE_MS=5000 \
  scripts/benchmark.sh --check --with-cargo --out performance.tsv
```

Use an otherwise idle machine: a concurrent build or test run can double Git
process latency. Keep the TSV from every run so a regression can be compared
with the previous baseline instead of only the pass/fail bit.

## Updating the baseline

1. Run `scripts/benchmark.sh --with-cargo --out baseline.tsv` on the reference
   machine, or on the machine class that will enforce the budget.
2. Confirm the large fixture reported 100 changed files and at least 10 MB of
   patch bytes; the script asserts this during setup.
3. Update the baseline tables and, if the change is deliberate, the budget
   defaults in `scripts/benchmark.sh` and this document together.
4. Note the date, hardware and any known load in the reference-machine table, so
   future comparisons are honest about what was measured.

## Limitations

- The harness measures engine calls, not GPUI paint or window startup. A slow
  first frame caused by layout or font work will not appear here.
- It does not measure avatar downloads, GitHub lookups or any network work.
- Idle CPU and watcher cost while a repository builds are not measured. Those
  need the running GPUI app and the platform watcher, so they remain a manual
  macOS check rather than part of this headless harness.
- Fixtures are small and clean except for the large working-tree case; a
  real repository with thousands of refs, huge histories or a slow filesystem
  can be much slower.
- `cargo check` figures depend on the dependency cache, Cargo's incremental
  state and the linker; they are useful for spotting a step change, not for
  comparing different machines.
- The benchmark example duplicates the detail sequence from
  `repo_map.rs`/`details.rs`; keeping the two in step is a manual review item.
- Linux requires GNU `/usr/bin/time`; without it the script warns and skips the
  memory metric.
