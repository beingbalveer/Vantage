# Safety test matrix — P0 and P1

Verification companion to [`REVIEW_TODO_2026-11-16.md`](REVIEW_TODO_2026-11-16.md).
It records, for every P0 and P1 ticket, the branch and commit that implement it, the regression
tests that cover its invariant, and whether those tests are present in that branch, current
`main`, or still missing.

Snapshot date: **2026-11-16**. Base: `main` at **`ab2b483`**.
Short SHAs below identify the inspected implementation commit, normally the branch tip. P1-10
points at its test/docs commit because a following docs-only commit records that hash. Rebasing a
branch changes its SHA but not the test names.

## Legend

| Status | Meaning |
|---|---|
| **Branch only** | The test exists in the ticket branch and is not in `main`. Running it requires checking out that branch; `cargo test` on `main` does not execute GPUI/UI tests that only exist there. |
| **Main** | Current `main` already contains a test for the same invariant. |
| **Gap** | No automated test covers the stated acceptance criterion yet. Manual verification is required. |

## Matrix

| Ticket | Branch @ commit | Regression tests | Test status | Remaining gap |
|---|---|---|---|---|
| **P0-01** Composer/amend scoped to a repository | `fix/p0-01-composer-state` @ `c2d4f69` | `composer_message_is_isolated_per_repository`, `amend_mode_never_survives_a_repository_switch`, `a_normal_commit_targets_the_selected_repository_and_its_tip`, `a_commit_is_refused_when_the_repository_or_head_moved`, `an_amend_targets_the_tip_captured_when_amend_mode_started`, `a_successful_commit_spends_only_its_repositorys_draft` (`crates/vantage/src/app.rs`) | Branch only | The tests exercise `ComposerStore`/`CommitTarget` rather than a GPUI type-then-switch-and-submit flow. The state machine they cover is the one the UI calls. |
| **P0-02** Branch prompts pinned to their repository | `fix/p0-02-branch-prompt-target` @ `c6213cf` | `create_prompt_refuses_a_base_branch_that_disappeared`, `rename_prompt_refuses_a_branch_that_is_no_longer_current`, `delete_prompt_refuses_a_branch_that_is_now_current`, `branch_prompt_keeps_its_repository_after_the_selection_changes`, `branch_prompt_refuses_a_repository_that_is_gone` (`crates/vantage/src/app.rs`) | Branch only | The two `gpui::test` cases cover the selection-change race. Existence re-checks use a supplied branch list, not a live repository race. |
| **P0-03** Amend body preservation and clean-tree amend | `fix/p0-03-amend-body` @ `4c06ad3` | `test_amend_preserves_existing_body_when_body_not_supplied`, `test_amend_uses_the_edited_body`, `test_amend_clears_the_body_when_explicitly_empty`, `test_amend_refuses_without_a_head_commit`, `test_amend_refuses_an_empty_summary` (`crates/vantage-core/src/git/ops.rs`); `amend_does_not_require_a_ticked_file_but_a_commit_does` (`crates/vantage/src/app.rs`); `commit_entries_keep_the_body_for_amend` (`crates/vantage/src/backend/details.rs`); `the_commit_bar_stays_visible_for_a_clean_tree_amend` (`crates/vantage/src/views/inspector.rs`) | Branch only. Main has `test_amend_preserves_author_date_and_updates_message`, which does not assert body preservation or a clean-tree amend. | No GPUI submit test that carries an edited description through `Backend::amend`; the core call and the view predicates are separately covered. |
| **P0-04** Corrupt state and failed saves are visible/recoverable | `fix/p0-04-state-recovery` @ `6be261c` | `backups_keep_the_previous_state_and_ignore_temp_files`, `save_reports_a_state_directory_that_cannot_be_written` (`crates/vantage-core/src/persist.rs`); `startup_keeps_the_corrupt_state_warning_for_the_ui`, `set_groups_reports_a_failed_state_write` (`crates/vantage/src/backend/mod.rs`); `corrupt_state_warning_is_visible_and_lists_backups`, `warning_from_the_main_thread_is_kept_for_the_banner`, `failed_workspace_save_is_reported_instead_of_succeeding_silently` (`crates/vantage/src/app.rs`) | Branch only. Main has `corrupt_state_is_backed_up_and_defaults_are_used`, which only checks that a warning exists and the original file moved; it does not check backup contents, recovery UI, or a failed save. | No test for the optional app-instance lock (the ticket says “consider”, not required). Restoring a listed backup is reveal/open only; there is no restore action to test. |
| **P0-05** Request identity for async commit/compare/stash loads | `fix/p0-05-async-request-identity` @ `1ccd8e8` | `a_late_commit_result_never_replaces_the_commit_that_is_now_open`, `a_late_comparison_result_never_replaces_the_comparison_that_is_now_open`, `a_reload_of_the_same_target_does_not_accept_the_previous_response`, `a_late_commit_result_does_not_reopen_a_closed_view`, `a_closed_stash_dialog_ignores_its_late_result`, `a_failed_commit_load_exposes_the_error_instead_of_an_empty_diff`, `a_failed_stash_load_exposes_the_error_instead_of_an_empty_list`, `a_result_for_another_repository_is_discarded` (`crates/vantage/src/app.rs`) | Branch only | Tests simulate reversed completion order at the result-application boundary; there is no GPUI task-scheduler integration test. |
| **P0-06** Reset semantics and confirmation copy | `fix/p0-06-reset-semantics` @ `da53cbc` | `test_reset_to_refuses_a_dirty_tree_without_touching_files` (`crates/vantage-core/src/git/ops.rs`); `reset_confirmation_states_the_hard_reset_contract`, `a_dirty_tree_is_refused_before_the_reset_confirmation`, `a_clean_tree_can_reach_the_reset_confirmation` (`crates/vantage/src/app.rs`); `fix/p1-10-safety-tests` adds `reset_dirty_tree_refusal_leaves_head_and_worktree_untouched` (`crates/vantage-core/tests/safety.rs`) | Branch only for the ticket tests. Main has `test_reset_dirty_undo_restores_head_and_dirty_state`, which covers the *allowed* dirty reset with a backup, not the refusal. The new integration test passes on main’s library. | No real confirmation-button click test; the copy and the pre-dialog refusal predicate are unit-tested. |
| **P1-01** Visible navigation for the three views | `fix/p1-01-view-navigation` @ `bce879a` | `nav_counts_match_the_current_repository_list`, `only_the_selected_destination_has_the_active_state`, `nav_counts_follow_a_rescan` (`crates/vantage/src/model.rs`) | Branch only | **Gap:** no render/click test proving all three sidebar destinations are reachable in one mouse click, or that counts are painted. Only the derived model is tested. |
| **P1-02** Watch Git metadata | `fix/p1-02-watcher-git` @ `bc7d560` | `ignored_working_paths_cover_build_and_vcs_directories`, `git_metadata_matching_is_limited_to_state_files`, `repository_matching_prefers_metadata_over_ignored_directories`, `worktree_git_file_resolves_private_and_common_directories`, `external_commit_marks_repository_dirty` (`crates/vantage/src/watcher.rs`) | Branch only | **Gap:** no test for a fetch updating ahead/behind, and no test that registration failures are visible. The external-commit test allows 10 s, not the ticket’s ~1 s target. |
| **P1-03** Split overview scans from details and bound Git work | `fix/p1-03-overview-scan-performance` @ `d285008` | `limited_read_keeps_short_output_intact`, `limited_read_stops_at_the_byte_cap` (`crates/vantage-core/src/git/runner.rs`); `dirty_scan_requests_are_coalesced_into_one_follow_up`, `a_full_scan_supersedes_queued_dirty_roots`, `a_visible_request_makes_the_coalesced_scan_visible`, `a_preview_only_matches_the_row_it_was_read_for`, `details_are_loaded_once_per_head_and_branch`, `invalidating_details_discards_an_in_flight_result`, `a_failed_detail_load_stays_visible_until_the_key_changes` (`crates/vantage/src/app.rs`); `bounded_mapping_keeps_order_and_isolates_errors`, `bounded_mapping_never_exceeds_its_worker_count`, `bounded_mapping_shares_one_git_runtime`, `bounded_mapping_spawns_git_processes_on_the_shared_runtime`, `bounded_mapping_handles_an_empty_input` (`crates/vantage/src/backend/mod.rs`); `overview_scan_reads_only_row_data`, `current_branch_falls_back_to_detached_then_a_plain_name`, `an_unborn_repository_loads_details_without_history_errors` (`crates/vantage/src/backend/repo_map.rs`); `a_scan_keeps_counts_and_reads_the_patch_only_on_demand` (`crates/vantage/src/backend/details.rs`) | Branch only | **Gap:** no measured scaling assertion (“3× repositories does not triple latency”) and no patch-memory budget test. Command-count, lazy-patch and bounded-worker tests cover the mechanism; timing budgets belong to P3-08. |
| **P1-04** Text-field clipping, scrolling and long-path overflow | `fix/p1-04-text-field-overflow` @ `3f8f63c` | `caret_scroll_target_follows_the_caret_through_a_selection`, `caret_scroll_target_handles_multibyte_and_empty_values`, `setting_or_clearing_a_value_resets_the_field_scroll` (`crates/vantage/src/ui/input.rs`); `long_path_stays_inside_the_modal_and_scrolls_to_the_caret` (`crates/vantage/src/views/add_repo_modal.rs`); `long_path_stays_clear_of_the_header_actions` (`crates/vantage/src/views/inspector.rs`) | Branch only | None for the stated 1040 px and 1470 px acceptance. IME and mouse caret placement remain documented input limitations without automated coverage. |
| **P1-05** Usable minimum-width layout | `fix/p1-05-min-width-layout` @ `b80314b` | `minimum_window_keeps_both_panes_usable`, `a_wide_window_gets_the_normal_controls`, `summary_compacts_before_the_rows_do`, `an_extreme_pane_request_leaves_the_list_its_floor`, `a_closed_inspector_gives_the_whole_content_area_to_the_list`, `a_collapsed_sidebar_does_not_reserve_its_splitter`, `the_inspector_minimum_wins_over_the_list_floor_when_they_conflict` (`crates/vantage/src/layout.rs`); `compact_badges_keep_only_the_count_and_the_tooltip_carries_the_rest` (`crates/vantage/src/ui/status.rs`); `the_dashboard_stays_inside_the_supported_window_widths` (`crates/vantage/src/views/repo_table.rs`) | Branch only | None for the stated 1040×640 and wide-window acceptance. |
| **P1-06** Scoped keyboard bindings and modal focus | `fix/p1-06-keyboard-modal-focus` @ `907e0ba` | `arrows_in_a_text_field_leave_the_repository_selection_alone`, `enter_in_a_modal_field_does_not_open_the_repository`, `repository_list_keeps_arrow_and_enter_navigation`, `clicking_a_row_establishes_the_list_keyboard_context`, `each_dialog_focuses_its_first_control`, `shortcut_opens_the_add_dialog_with_the_path_focused`, `toolbar_opens_the_add_dialog_with_the_path_focused`, `menu_action_opens_the_add_dialog_with_the_path_focused`, `escape_closes_a_dialog_from_a_non_field_control`, `tab_cycles_inside_an_open_dialog`, `closing_a_dialog_restores_the_previous_field`, `new_group_reveals_with_its_input_focused`, `left_right_arrows_still_edit_inside_the_commit_message` (`crates/vantage/src/app.rs`) | Branch only | None for the stated field-isolation, Esc, Tab-trap and focus-restore acceptance. |
| **P1-07** Accessibility: contrast, sizes, labels and hit targets | `fix/p1-07-accessibility` @ `a4b00f9` | `metadata_text_meets_wcag_aa_in_both_themes`, `status_and_accent_text_meet_wcag_aa_in_both_themes`, `diff_text_meets_wcag_aa_in_both_themes`, `focus_ring_meets_non_text_contrast`, `dim_text_stays_decorative` (`crates/vantage/src/theme.rs`); `focus_ring_is_a_two_pixel_spread_shadow` (`crates/vantage/src/ui/focus.rs`); `small_visuals_grow_to_the_minimum_hit_target`, `larger_visuals_keep_their_own_size`, `primary_actions_get_a_larger_target` (`crates/vantage/src/ui/tokens.rs`); `checkbox_hit_target_is_at_least_32_square` (`crates/vantage/src/app.rs`); `enter_submits_and_escape_cancels_without_typing` (`crates/vantage/src/ui/input.rs`) | Branch only | **Gap:** accessible roles/names, screen-reader announcements and keyboard-only primary flows have no automated assertion (GPUI support is limited). Contrast, type-token and hit-target checks are covered. |
| **P1-08** README and product copy aligned | `fix/p1-08-readme-accuracy` @ `da1c454` | None — docs and copy only (`README.md`, `crates/vantage/src/app.rs`, `crates/vantage/src/views/add_repo_modal.rs`) | **Gap** | **Gap:** no automated check that user-facing claims match the shipped UI. Requires manual review on a release build; the branch itself is the evidence, not a test. |
| **P1-09** Root commits run hooks and signing | `fix/p1-09-root-commit-hooks` @ `c90070b` | `test_root_commit_uses_staged_content_and_keeps_unstaged_lines`, `test_root_commit_runs_hooks_and_commits_only_selected_paths`, `test_root_commit_failing_hook_aborts_without_commit`, `test_root_commit_uses_configured_commit_signing`, `test_commit_with_existing_history_runs_hooks_and_preserves_selection` (`crates/vantage-core/src/git/ops.rs`) | Branch only | None for the stated acceptance on Unix. Signing is exercised with a disposable fake GPG program; Windows signing is not covered. |
| **P1-10** Regression coverage for the safety invariants | `fix/p1-10-safety-tests` @ `935218d` | `literal_pathspec_staging_and_discard_only_touch_the_named_path`, `selective_root_commit_excludes_an_unselected_staged_file`, `reset_dirty_tree_refusal_leaves_head_and_worktree_untouched`, `branch_delete_refuses_unmerged_work_and_succeeds_once_merged`, `corrupt_state_load_warns_and_preserves_a_backup_file` (`crates/vantage-core/tests/safety.rs`) | This branch. The tests use only `vantage-core` public APIs and pass against `main`’s library code. | **Gap:** this ticket adds cross-cutting core coverage, not the branch-only UI harnesses listed above. The P1-08 docs claim and the P1-01 navigation click path remain manual. |

> **P1-10 commit hash:** `935218d` is the implementation commit that added the five integration
> tests and this matrix. A following docs-only commit records that hash here; a commit cannot
> embed its own hash. Both commits are on `fix/p1-10-safety-tests`.

## Cross-cutting integration tests added by P1-10

These live in `crates/vantage-core/tests/safety.rs`, run against a disposable repository per test,
and remove their temporary directories on drop.

| Test | Invariant on current `main` | Relation to existing coverage |
|---|---|---|
| `literal_pathspec_staging_and_discard_only_touch_the_named_path` | A path containing `*` or `?` is passed as a literal pathspec, so staging or discarding it does not affect sibling paths. | Main has `test_discard_literal_wildcard_filenames` (module test, `*` only) but no public-API staging test and no literal `?` case. This adds both and exercises the public `stage_path`/`discard` contract. |
| `selective_root_commit_excludes_an_unselected_staged_file` | A root commit includes only the selected path; a pre-staged unselected file stays staged and out of `HEAD`. | Main has `test_selective_root_commit_preserves_pre_staged_files` as a module test. This is the same invariant through the public `commit` API, as an external consumer sees it. |
| `reset_dirty_tree_refusal_leaves_head_and_worktree_untouched` | A dirty-tree reset without `also_discard_dirty` returns `Dirty` before moving `HEAD`, editing tracked files, deleting untracked files, or creating a backup ref. | Main has only the allowed dirty-reset/undo test (`test_reset_dirty_undo_restores_head_and_dirty_state`), not the refusal case. This is new behavioural coverage. |
| `branch_delete_refuses_unmerged_work_and_succeeds_once_merged` | `delete_branch(..., force = false)` refuses unmerged work and leaves the branch; once merged, the same call succeeds. | No branch-delete test on main. This is new behavioural coverage. |
| `corrupt_state_load_warns_and_preserves_a_backup_file` | Corrupt `state.json` produces a warning that names the backup, falls back to defaults, and leaves one `state.corrupt-<timestamp>.json` with the original bytes. | Main’s `corrupt_state_is_backed_up_and_defaults_are_used` checks warning presence and that the original moved, but not the backup contents or path in the warning. This strengthens the contract. |

Two of these intentionally overlap module tests on `main` because the ticket asks for a
cross-cutting, public-API safety suite. They are contract tests, not claims that the behaviour is
missing on `main`.

## Running the coverage

The integration suite (on this branch, against main’s library code):

```sh
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo test -p vantage-core --test safety
```

Any branch-only regression test, for example:

```sh
git switch fix/p0-01-composer-state
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo test -p vantage composer_message_is_isolated_per_repository
```

Full UI coverage requires the ticket branch to be checked out: those tests do not exist in
`main` and are not picked up by `cargo test --workspace` on `main`. The matrix above is a
snapshot of branch tips; `git log main..<branch>` lists every commit on a branch, and
`git grep -n <test-name> <branch> -- crates` confirms a test before checking the branch out.

## Gaps to close after the branches land

1. **P1-01:** a GPUI click test that reaches all three destinations from the sidebar and shows
   the active state/counts.
2. **P1-02:** fetch/ahead-behind propagation, the ~1 s target, and visible registration failures.
3. **P1-03:** scaling and patch-memory budgets (P3-08 owns the benchmark harness).
4. **P1-07:** accessible roles/names and screen-reader announcement checks where GPUI exposes
   them; a keyboard-only end-to-end primary flow.
5. **P1-08:** an automated or release-checklist check that README claims match the shipped app.
6. **P0/P1 UI flows:** several safety fixes are covered at the state-machine or core level but
   not by a full GPUI interaction (typing then switching, confirming a reset, submitting an
   amend). These require the ticket branch checked out and are explicitly not claimed here.
