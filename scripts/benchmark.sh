#!/usr/bin/env bash
#
# Reproducible engine benchmarks for Vantage.
#
# Every repository is created under a disposable `mktemp -d` directory and
# removed on exit. The script never reads or writes Vantage's workspace state,
# the user's Git config, or the repository's own `target/` directory. Git runs
# with an isolated `GIT_CONFIG_GLOBAL`/`GIT_CONFIG_NOSYSTEM`, and the optional
# Cargo build uses its own target directory inside the fixture. Pass `--keep` to
# inspect the fixture after a run.
#
# Usage:
#   scripts/benchmark.sh                    # engine metrics (overview/detail/memory)
#   scripts/benchmark.sh --with-cargo       # also cold/warm `cargo check` and target size
#   scripts/benchmark.sh --check            # fail when a documented budget is exceeded
#   scripts/benchmark.sh --help
#
# Metrics are tab-separated `key<TAB>value` lines on stdout; progress goes to
# stderr. See docs/PERFORMANCE.md for metric definitions and budgets.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

ITERATIONS=3
REPO_COUNTS="5 20 50"
WITH_CARGO=0
CHECK=0
DRY_RUN=0
KEEP=0
QUIET=0
SKIP_MEMORY=0
OUT_FILE=""
CARGO_TARGET_OPTION=""

TMP_ROOT=""
METRICS_FILE=""
BENCH=""

# Budgets used by --check. Override any of them in the environment; the
# defaults are calibrated on the reference machine in docs/PERFORMANCE.md and
# should be raised for slower CI runners.
PERF_MAX_COLD_CHECK_S="${PERF_MAX_COLD_CHECK_S:-300}"
PERF_MAX_WARM_CHECK_S="${PERF_MAX_WARM_CHECK_S:-30}"
PERF_MAX_TARGET_MIB="${PERF_MAX_TARGET_MIB:-2048}"
PERF_MAX_OVERVIEW_5_MS="${PERF_MAX_OVERVIEW_5_MS:-4000}"
PERF_MAX_OVERVIEW_20_MS="${PERF_MAX_OVERVIEW_20_MS:-16000}"
PERF_MAX_OVERVIEW_50_MS="${PERF_MAX_OVERVIEW_50_MS:-35000}"
PERF_MAX_DETAIL_MS="${PERF_MAX_DETAIL_MS:-900}"
PERF_MAX_DETAIL_LARGE_MS="${PERF_MAX_DETAIL_LARGE_MS:-1500}"
PERF_MAX_RSS_MIB="${PERF_MAX_RSS_MIB:-128}"

usage() {
    cat <<'EOF'
Vantage performance benchmark

Usage: scripts/benchmark.sh [options]

Options:
  -h, --help            Show this help and exit.
  -n, --dry-run         Describe the run without creating fixtures or compiling.
      --with-cargo      Also measure cold/warm `cargo check` and target size.
                        This is slow: it compiles the workspace in a disposable
                        target directory.
      --iterations N    Measured runs per benchmark; medians are reported
                        (default: 3).
      --repo-counts L   Space-separated repository counts for the overview scan
                        (default: "5 20 50").
      --skip-memory     Skip the 100-changed-file / 10 MB patch RSS fixture.
      --check           Compare measured metrics with the budgets below and
                        exit non-zero if any is exceeded.
      --out FILE        Also write the tab-separated metrics to FILE.
      --cargo-target D  Reuse D as the Cargo target directory for the benchmark
                        binary. By default it lives in the disposable fixture.
      --keep            Keep the temporary fixture directory and print its path.
      --quiet           Suppress progress messages; metrics still go to stdout.

Metrics are written to stdout as `key<TAB>value`. Times are milliseconds unless
the key ends in `_s` (seconds) or `_mib` (MiB). Budgets can be overridden:

  PERF_MAX_COLD_CHECK_S       PERF_MAX_OVERVIEW_5_MS
  PERF_MAX_WARM_CHECK_S       PERF_MAX_OVERVIEW_20_MS
  PERF_MAX_TARGET_MIB         PERF_MAX_OVERVIEW_50_MS
  PERF_MAX_DETAIL_MS          PERF_MAX_DETAIL_LARGE_MS
  PERF_MAX_RSS_MIB

Everything runs against disposable fixtures. Vantage's state.json, the user's
Git config and the repository's target/ directory are never touched.
EOF
}

log() {
    [ "$QUIET" -eq 1 ] || printf 'benchmark: %s\n' "$*" >&2
}

section() {
    [ "$QUIET" -eq 1 ] || printf '\n== %s ==\n' "$*" >&2
}

warn() {
    printf 'benchmark: warning: %s\n' "$*" >&2
}

die() {
    printf 'benchmark: error: %s\n' "$*" >&2
    exit 1
}

cleanup() {
    if [ -n "$TMP_ROOT" ] && [ -d "$TMP_ROOT" ]; then
        if [ "$KEEP" -eq 1 ]; then
            printf 'benchmark: fixture kept at %s\n' "$TMP_ROOT" >&2
        else
            rm -rf "$TMP_ROOT"
        fi
    fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

parse_args() {
    while [ "$#" -gt 0 ]; do
        case "$1" in
            -h|--help)
                usage
                exit 0
                ;;
            -n|--dry-run)
                DRY_RUN=1
                ;;
            --with-cargo)
                WITH_CARGO=1
                ;;
            --iterations)
                shift
                [ "$#" -gt 0 ] || die "--iterations needs a value"
                ITERATIONS="$1"
                ;;
            --iterations=*)
                ITERATIONS="${1#*=}"
                ;;
            --repo-counts)
                shift
                [ "$#" -gt 0 ] || die "--repo-counts needs a value"
                REPO_COUNTS="$1"
                ;;
            --repo-counts=*)
                REPO_COUNTS="${1#*=}"
                ;;
            --skip-memory)
                SKIP_MEMORY=1
                ;;
            --check)
                CHECK=1
                ;;
            --out)
                shift
                [ "$#" -gt 0 ] || die "--out needs a value"
                OUT_FILE="$1"
                ;;
            --out=*)
                OUT_FILE="${1#*=}"
                ;;
            --cargo-target)
                shift
                [ "$#" -gt 0 ] || die "--cargo-target needs a value"
                CARGO_TARGET_OPTION="$1"
                ;;
            --cargo-target=*)
                CARGO_TARGET_OPTION="${1#*=}"
                ;;
            --keep)
                KEEP=1
                ;;
            --quiet)
                QUIET=1
                ;;
            *)
                die "unknown option: $1 (try --help)"
                ;;
        esac
        shift
    done
}

validate_args() {
    case "$ITERATIONS" in
        ''|*[!0-9]*) die "--iterations must be a positive integer" ;;
    esac
    [ "$ITERATIONS" -ge 1 ] && [ "$ITERATIONS" -le 50 ] \
        || die "--iterations must be between 1 and 50"

    local max_count=0
    local count seen=0
    for count in $REPO_COUNTS; do
        case "$count" in
            ''|*[!0-9]*) die "invalid repository count: $count" ;;
        esac
        [ "$count" -ge 1 ] || die "repository counts must be positive"
        [ "$count" -le 200 ] || die "repository counts above 200 are not supported"
        [ "$count" -gt "$max_count" ] && max_count="$count"
        seen=$((seen + 1))
    done
    [ "$seen" -gt 0 ] || die "--repo-counts needs at least one number"

    case "$CARGO_TARGET_OPTION" in
        *' '*|*$'\t'*) die "--cargo-target must not contain spaces" ;;
    esac
}

now_seconds() {
    if command -v perl >/dev/null 2>&1; then
        perl -MTime::HiRes=time -e 'printf "%.6f\n", time'
    else
        date +%s
    fi
}

emit() {
    printf '%s\t%s\n' "$1" "$2" | tee -a "$METRICS_FILE"
}

time_command() {
    local label="$1"
    shift
    local started finished status
    started="$(now_seconds)"
    set +e
    "$@" >"$TMP_ROOT/${label}.log" 2>&1
    status=$?
    set -e
    finished="$(now_seconds)"
    if [ "$status" -ne 0 ]; then
        tail -40 "$TMP_ROOT/${label}.log" >&2 || true
        die "command failed: $*"
    fi
    awk -v start="$started" -v end="$finished" 'BEGIN { printf "%.3f", end - start }'
}

print_dry_run() {
    local max_count=0
    local count
    for count in $REPO_COUNTS; do
        [ "$count" -gt "$max_count" ] && max_count="$count"
    done

    cat <<EOF
benchmark: dry run (nothing will be created)

  fixture root      temporary directory from mktemp
  engine benchmark  cargo build -p vantage-core --example bench_scan
  overview          ${ITERATIONS} iteration(s) for ${REPO_COUNTS} repositories
                    (${max_count} disposable repos in total)
  detail            ${ITERATIONS} iteration(s) on one clean repository
EOF
    if [ "$SKIP_MEMORY" -eq 0 ]; then
        cat <<EOF
  memory            detail scan of 100 changed files / ~10 MB patch
EOF
    fi
    if [ "$WITH_CARGO" -eq 1 ]; then
        cat <<EOF
  cargo check       cold then warm, disposable target directory, then target size
EOF
    fi
    if [ "$CHECK" -eq 1 ]; then
        cat <<EOF

  budgets           cargo cold <= ${PERF_MAX_COLD_CHECK_S}s, warm <= ${PERF_MAX_WARM_CHECK_S}s
                    target <= ${PERF_MAX_TARGET_MIB} MiB
                    overview 5/20/50 <= ${PERF_MAX_OVERVIEW_5_MS}/${PERF_MAX_OVERVIEW_20_MS}/${PERF_MAX_OVERVIEW_50_MS} ms
                    detail <= ${PERF_MAX_DETAIL_MS} ms, large detail <= ${PERF_MAX_DETAIL_LARGE_MS} ms
                    peak RSS <= ${PERF_MAX_RSS_MIB} MiB
EOF
    fi
    cat <<EOF

  state.json        not read or written
  user git config   not read (isolated GIT_CONFIG_GLOBAL, GIT_CONFIG_NOSYSTEM=1)
  repository target unchanged (benchmark target lives in the fixture)
EOF
}

create_fixture_root() {
    TMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/vantage-bench.XXXXXX")" \
        || die "could not create a temporary directory"
    METRICS_FILE="$TMP_ROOT/metrics.tsv"
    : >"$METRICS_FILE"
}

isolate_git() {
    mkdir -p "$TMP_ROOT/no-hooks" "$TMP_ROOT/fixtures"
    cat >"$TMP_ROOT/gitconfig" <<EOF
[user]
    name = Vantage Benchmark
    email = bench@vantage.invalid
[init]
    defaultBranch = main
[commit]
    gpgsign = false
[tag]
    gpgsign = false
[core]
    hooksPath = $TMP_ROOT/no-hooks
EOF
    export GIT_CONFIG_GLOBAL="$TMP_ROOT/gitconfig"
    export GIT_CONFIG_NOSYSTEM=1
    export GIT_TERMINAL_PROMPT=0
    export LC_ALL=C
}

make_repo() {
    local repo="$1"
    local name
    name="$(basename "$repo")"

    mkdir -p "$repo/src/nested"
    printf '# %s\n\nDisposable benchmark fixture.\n' "$name" >"$repo/README.md"
    printf 'fn main() {\n    println!("%s");\n}\n' "$name" >"$repo/src/main.rs"
    printf 'export const name = "%s";\n' "$name" >"$repo/src/nested/app.ts"

    git init -q -b main "$repo"
    git -C "$repo" add -A
    git -C "$repo" commit -q -m "initial commit"
    printf '\n// second line\n' >>"$repo/src/main.rs"
    printf '\n# notes\n' >>"$repo/README.md"
    git -C "$repo" add -A
    git -C "$repo" commit -q -m "second commit"
    git -C "$repo" branch feature
}

make_big_repo() {
    local repo="$1"
    local file count changed
    local index=1

    git init -q -b main "$repo"
    mkdir -p "$repo/src"
    while [ "$index" -le 99 ]; do
        file="$(printf '%s/src/file_%03d.txt' "$repo" "$index")"
        awk -v n="$index" \
            'BEGIN { for (line = 1; line <= 20; line++) printf "file %03d line %02d baseline\n", n, line }' \
            >"$file"
        index=$((index + 1))
    done
    awk 'BEGIN { for (line = 0; line < 100000; line++) printf "old line %06d abcdefghijklmnopqrstuvwxyz0123456789\n", line }' \
        >"$repo/big.txt"
    git -C "$repo" add -A
    git -C "$repo" commit -q -m "benchmark base"

    index=1
    while [ "$index" -le 99 ]; do
        file="$(printf '%s/src/file_%03d.txt' "$repo" "$index")"
        printf 'changed line 1\nchanged line 2\nchanged line 3\n' >>"$file"
        index=$((index + 1))
    done
    awk 'BEGIN { for (line = 0; line < 100000; line++) printf "new line %06d zyxwvutsrqponmlkjihgfedcba9876543210\n", line }' \
        >"$repo/big.txt"

    changed="$(git -C "$repo" status --porcelain=v2 | wc -l | tr -d ' ')"
    [ "$changed" -ge 100 ] || die "big fixture has only $changed changed files"
    count="$(git -C "$repo" diff --no-color --no-ext-diff | wc -c | tr -d ' ')"
    [ "$count" -ge 10000000 ] || die "big fixture patch is only $count bytes"
}

build_benchmark() {
    local target
    if [ -n "$CARGO_TARGET_OPTION" ]; then
        target="$CARGO_TARGET_OPTION"
    else
        target="$TMP_ROOT/cargo-target"
    fi

    BENCH="$target/debug/examples/bench_scan"
    if [ ! -x "$BENCH" ]; then
        log "building the engine benchmark (vantage-core example)"
        mkdir -p "$target"
        cargo build -q -p vantage-core --example bench_scan --locked --target-dir "$target" \
            || die "could not build crates/vantage-core/examples/bench_scan.rs"
    fi
    [ -x "$BENCH" ] || die "benchmark binary not found at $BENCH"
}

create_overview_fixtures() {
    local max_count="$1"
    local root="$TMP_ROOT/fixtures/repos"
    local index=1

    mkdir -p "$root"
    log "creating $max_count disposable repositories"
    while [ "$index" -le "$max_count" ]; do
        make_repo "$root/$(printf 'repo-%03d' "$index")"
        index=$((index + 1))
    done
}

run_overview() {
    local count="$1"
    shift
    local output="$TMP_ROOT/overview-${count}.tsv"
    local key value

    section "Overview scan: ${count} repositories"
    "$BENCH" overview --iterations "$ITERATIONS" "$@" >"$output" \
        || die "overview benchmark failed for ${count} repositories"
    while IFS=$'\t' read -r key value; do
        case "$key" in
            repos)
                emit "overview_${count}_repos" "$value"
                ;;
            overview_*)
                emit "overview_${count}_${key#overview_}" "$value"
                ;;
        esac
    done <"$output"
}

run_detail() {
    local repo="$1"
    local output="$TMP_ROOT/detail.tsv"
    local key value

    section "Time to detail"
    "$BENCH" detail --iterations "$ITERATIONS" "$repo" >"$output" \
        || die "detail benchmark failed"
    while IFS=$'\t' read -r key value; do
        case "$key" in
            detail_*)
                emit "$key" "$value"
                ;;
        esac
    done <"$output"
}

run_memory() {
    local repo="$1"
    local output="$TMP_ROOT/memory-detail.tsv"
    local timing="$TMP_ROOT/memory-time.txt"
    local key value peak

    if [ ! -x /usr/bin/time ]; then
        warn "/usr/bin/time is unavailable; skipping the peak-RSS measurement"
        return
    fi

    section "Memory: 100 changed files and a ~10 MB patch"
    if [ "$(uname -s)" = "Darwin" ]; then
        if ! /usr/bin/time -l "$BENCH" detail --iterations 1 "$repo" \
            >"$output" 2>"$timing"; then
            warn "memory fixture scan failed"
            return
        fi
        peak="$(awk '/maximum resident set size/ { print $1; exit }' "$timing")"
        [ -n "$peak" ] || {
            warn "could not parse macOS peak RSS"
            return
        }
        emit "memory_peak_rss_mib" "$(awk -v bytes="$peak" 'BEGIN { printf "%.1f", bytes / 1048576 }')"
    else
        if ! /usr/bin/time -v "$BENCH" detail --iterations 1 "$repo" \
            >"$output" 2>"$timing"; then
            warn "memory fixture scan failed"
            return
        fi
        peak="$(awk -F: '/Maximum resident set size/ {
                value = $2
                gsub(/^[ \t]+/, "", value)
                gsub(/[ \t].*/, "", value)
                print value
                exit
            }' "$timing")"
        [ -n "$peak" ] || {
            warn "could not parse Linux peak RSS"
            return
        }
        emit "memory_peak_rss_mib" "$(awk -v kb="$peak" 'BEGIN { printf "%.1f", kb / 1024 }')"
    fi

    while IFS=$'\t' read -r key value; do
        case "$key" in
            detail_total_ms)
                emit "detail_large_ms" "$value"
                ;;
            detail_patch_bytes)
                emit "detail_large_patch_bytes" "$value"
                ;;
            detail_changed_files_count)
                emit "detail_large_changed_files" "$value"
                ;;
        esac
    done <"$output"
}

run_cargo_budgets() {
    local target="$TMP_ROOT/cargo-check-target"
    local cold warm size

    section "Build budgets (disposable target)"
    log "cold cargo check: compiles the workspace from scratch and can take minutes"
    cold="$(time_command "cargo-check-cold" cargo check --workspace --locked --target-dir "$target")"
    emit "cargo_check_cold_s" "$cold"

    log "warm cargo check"
    warm="$(time_command "cargo-check-warm" cargo check --workspace --locked --target-dir "$target")"
    emit "cargo_check_warm_s" "$warm"

    size="$(du -sm "$target" | awk '{ print $1 }')"
    emit "cargo_target_size_mib" "$size"
}

metric_value() {
    awk -F'\t' -v key="$1" '$1 == key { print $2; exit }' "$METRICS_FILE"
}

check_budget() {
    local key="$1"
    local limit="$2"
    local value
    value="$(metric_value "$key")"
    [ -n "$value" ] || return 0

    if ! awk -v value="$value" -v limit="$limit" -v key="$key" 'BEGIN {
            if (value > limit) {
                printf "FAIL  %-30s %10.3f > %10.3f\n", key, value, limit
                exit 1
            }
            printf "ok    %-30s %10.3f <= %10.3f\n", key, value, limit
        }'; then
        return 1
    fi
}

run_checks() {
    local failures=0

    section "Budgets"
    check_budget cargo_check_cold_s "$PERF_MAX_COLD_CHECK_S" || failures=$((failures + 1))
    check_budget cargo_check_warm_s "$PERF_MAX_WARM_CHECK_S" || failures=$((failures + 1))
    check_budget cargo_target_size_mib "$PERF_MAX_TARGET_MIB" || failures=$((failures + 1))
    check_budget overview_5_full_ms "$PERF_MAX_OVERVIEW_5_MS" || failures=$((failures + 1))
    check_budget overview_20_full_ms "$PERF_MAX_OVERVIEW_20_MS" || failures=$((failures + 1))
    check_budget overview_50_full_ms "$PERF_MAX_OVERVIEW_50_MS" || failures=$((failures + 1))
    check_budget detail_total_ms "$PERF_MAX_DETAIL_MS" || failures=$((failures + 1))
    check_budget detail_large_ms "$PERF_MAX_DETAIL_LARGE_MS" || failures=$((failures + 1))
    check_budget memory_peak_rss_mib "$PERF_MAX_RSS_MIB" || failures=$((failures + 1))

    if [ "$failures" -gt 0 ]; then
        die "$failures performance budget(s) exceeded"
    fi
}

main() {
    parse_args "$@"
    validate_args

    if [ "$DRY_RUN" -eq 1 ]; then
        print_dry_run
        exit 0
    fi

    create_fixture_root
    isolate_git
    build_benchmark

    local max_count=0
    local count index
    for count in $REPO_COUNTS; do
        [ "$count" -gt "$max_count" ] && max_count="$count"
    done

    create_overview_fixtures "$max_count"

    local repo_args
    for count in $REPO_COUNTS; do
        repo_args=()
        index=1
        while [ "$index" -le "$count" ]; do
            repo_args+=("$TMP_ROOT/fixtures/repos/$(printf 'repo-%03d' "$index")")
            index=$((index + 1))
        done
        run_overview "$count" "${repo_args[@]}"
    done

    run_detail "$TMP_ROOT/fixtures/repos/repo-001"

    if [ "$SKIP_MEMORY" -eq 0 ]; then
        section "Creating the large working-tree fixture"
        make_big_repo "$TMP_ROOT/fixtures/big/repo"
        run_memory "$TMP_ROOT/fixtures/big/repo"
    fi

    if [ "$WITH_CARGO" -eq 1 ]; then
        run_cargo_budgets
    fi

    if [ -n "$OUT_FILE" ]; then
        mkdir -p "$(dirname "$OUT_FILE")"
        cp "$METRICS_FILE" "$OUT_FILE"
        log "metrics written to $OUT_FILE"
    fi

    if [ "$CHECK" -eq 1 ]; then
        run_checks
    fi

    log "done"
}

main "$@"
