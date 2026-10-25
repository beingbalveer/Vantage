//! Line-level patch synthesis.
//!
//! The UI renders a unified diff with context. Every staged/discarded line is
//! sent back with its old/new line number and text. This module re-parses the
//! *authoritative* `git diff` output for the file (produced with `-U0`),
//! verifies that every selected line still exists and matches, and emits a
//! minimal patch that `git apply` can consume atomically.
//!
//! Why synthesize instead of applying the displayed diff: the displayed diff
//! may contain expanded context, uses a different context width, and (for the
//! working scope) mixes index and worktree state. Building from a fresh
//! index→worktree or HEAD→index diff keeps the patch preimage identical to the
//! tree we are about to modify.

use crate::error::AppError;
use crate::state::LineKind;
use serde::{Deserialize, Serialize};

/// A line the user selected in the rendered diff.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedLine {
    pub kind: LineKind,
    pub old_no: Option<u32>,
    pub new_no: Option<u32>,
    /// Rendered text (without the leading +/-/space). Used to detect that the
    /// file changed after the diff was displayed.
    #[serde(default)]
    pub text: String,
}

/// Which tree the synthesized patch will be applied to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchTarget {
    /// index → worktree diff, applied forward to the index.
    Stage,
    /// HEAD → index diff, applied in reverse to the index.
    Unstage,
    /// index → worktree diff, applied in reverse to the worktree.
    Discard,
}

/// Result of synthesis. `ResetIndex` is used when the selection covers an
/// entire added/deleted file, where a patch is less reliable than Git's own
/// index reset (and works for root commits too).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Synthesized {
    Patch(Vec<u8>),
    /// A new-file patch that must be applied *forward* to the index. Used to
    /// unstage part of a staged deletion (Git cannot reverse-create an index
    /// entry).
    AddFile(Vec<u8>),
    ResetIndex,
}

#[derive(Debug, Clone)]
struct ParsedLine {
    kind: LineKind,
    old_no: Option<u32>,
    new_no: Option<u32>,
    /// Full raw line including the diff prefix and line terminator.
    raw: Vec<u8>,
    /// Line text without the diff prefix or line terminator.
    text: Vec<u8>,
    /// The `\ No newline at end of file` marker that follows this line.
    marker: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
struct ParsedHunk {
    old_start: u32,
    old_lines: u32,
    new_start: u32,
    new_lines: u32,
    suffix: Vec<u8>,
    lines: Vec<ParsedLine>,
}

#[derive(Debug, Clone)]
struct ParsedFile {
    header: Vec<Vec<u8>>,
    hunks: Vec<ParsedHunk>,
    new_file: bool,
    deleted_file: bool,
}

fn split_inclusive_lines(data: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, byte) in data.iter().enumerate() {
        if *byte == b'\n' {
            out.push(&data[start..=i]);
            start = i + 1;
        }
    }
    if start < data.len() {
        out.push(&data[start..]);
    }
    out
}

fn strip_line_end(line: &[u8]) -> &[u8] {
    let mut end = line.len();
    if end > 0 && line[end - 1] == b'\n' {
        end -= 1;
    }
    if end > 0 && line[end - 1] == b'\r' {
        end -= 1;
    }
    &line[..end]
}

fn parse_range(token: &[u8]) -> (u32, u32) {
    let text = String::from_utf8_lossy(token);
    let mut parts = text.split(',');
    let start = parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let lines = parts
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    (start, lines)
}

fn parse_hunk_header(line: &[u8]) -> Option<(u32, u32, u32, u32, Vec<u8>)> {
    if !line.starts_with(b"@@ ") {
        return None;
    }
    let second = line[2..]
        .windows(2)
        .position(|w| w == b"@@")
        .map(|p| p + 2)?;
    let ranges = &line[3..second];
    let suffix = line[second + 2..].to_vec();

    let mut old_start = 1;
    let mut old_lines = 1;
    let mut new_start = 1;
    let mut new_lines = 1;

    for token in ranges.split(|b| b.is_ascii_whitespace()) {
        if token.is_empty() {
            continue;
        }
        if token[0] == b'-' {
            let (s, l) = parse_range(&token[1..]);
            old_start = s;
            old_lines = l;
        } else if token[0] == b'+' {
            let (s, l) = parse_range(&token[1..]);
            new_start = s;
            new_lines = l;
        }
    }

    Some((old_start, old_lines, new_start, new_lines, suffix))
}

fn parse_file_diff(raw: &[u8]) -> Result<ParsedFile, AppError> {
    let lines = split_inclusive_lines(raw);
    let mut header = Vec::new();
    let mut hunks: Vec<ParsedHunk> = Vec::new();
    let mut file_count = 0usize;
    let mut new_file = false;
    let mut deleted_file = false;
    let mut binary = false;
    let mut submodule = false;
    let mut rename = false;
    let mut mode_change = false;

    let mut current: Option<ParsedHunk> = None;
    let mut in_hunk = false;

    for line in lines {
        let bare = strip_line_end(line);
        if line.starts_with(b"diff --git ") {
            file_count += 1;
            if file_count > 1 {
                return Err(AppError::other(
                    "Partial operations support one file at a time; the diff contained multiple files.",
                ));
            }
            in_hunk = false;
            if let Some(h) = current.take() {
                hunks.push(h);
            }
            header.push(line.to_vec());
            continue;
        }

        if let Some((old_start, old_lines, new_start, new_lines, suffix)) =
            parse_hunk_header(line)
        {
            if let Some(h) = current.take() {
                hunks.push(h);
            }
            in_hunk = true;
            current = Some(ParsedHunk {
                old_start,
                old_lines,
                new_start,
                new_lines,
                suffix,
                lines: Vec::new(),
            });
            continue;
        }

        if !in_hunk {
            header.push(line.to_vec());
            if bare.starts_with(b"new file mode") {
                new_file = true;
            } else if bare.starts_with(b"deleted file mode") {
                deleted_file = true;
            } else if bare.starts_with(b"rename from ")
                || bare.starts_with(b"rename to ")
                || bare.starts_with(b"copy from ")
                || bare.starts_with(b"copy to ")
            {
                rename = true;
            } else if bare.starts_with(b"old mode ") || bare.starts_with(b"new mode ") {
                mode_change = true;
            } else if bare.starts_with(b"Binary files ") || bare == b"GIT binary patch" {
                binary = true;
            }
            continue;
        }

        let hunk = current
            .as_mut()
            .ok_or_else(|| AppError::other("Malformed unified diff: hunk body without header"))?;

        if bare.starts_with(b"\\") {
            if let Some(prev) = hunk.lines.last_mut() {
                prev.marker = Some(line.to_vec());
            }
            continue;
        }

        if line.is_empty() {
            continue;
        }

        match line[0] {
            b'+' => hunk.lines.push(ParsedLine {
                kind: LineKind::Add,
                old_no: None,
                new_no: None,
                raw: line.to_vec(),
                text: bare[1..].to_vec(),
                marker: None,
            }),
            b'-' => hunk.lines.push(ParsedLine {
                kind: LineKind::Del,
                old_no: None,
                new_no: None,
                raw: line.to_vec(),
                text: bare[1..].to_vec(),
                marker: None,
            }),
            b' ' => hunk.lines.push(ParsedLine {
                kind: LineKind::Context,
                old_no: None,
                new_no: None,
                raw: line.to_vec(),
                text: bare[1..].to_vec(),
                marker: None,
            }),
            _ => {
                // Not a valid body line; keep it out of the patch.
            }
        }
    }

    if let Some(h) = current.take() {
        hunks.push(h);
    }

    if file_count == 0 {
        return Err(AppError::other(
            "No changes for this file were found. Refresh the diff and try again.",
        ));
    }

    // Assign line numbers and detect submodule entries. In a zero-length
    // range Git's start is the line *before* the insertion point.
    for hunk in &mut hunks {
        let mut old_cursor = if hunk.old_lines == 0 {
            hunk.old_start.saturating_add(1)
        } else {
            hunk.old_start
        };
        let mut new_cursor = if hunk.new_lines == 0 {
            hunk.new_start.saturating_add(1)
        } else {
            hunk.new_start
        };
        for line in &mut hunk.lines {
            match line.kind {
                LineKind::Add => {
                    line.new_no = Some(new_cursor);
                    new_cursor = new_cursor.saturating_add(1);
                }
                LineKind::Del => {
                    line.old_no = Some(old_cursor);
                    old_cursor = old_cursor.saturating_add(1);
                }
                LineKind::Context => {
                    line.old_no = Some(old_cursor);
                    line.new_no = Some(new_cursor);
                    old_cursor = old_cursor.saturating_add(1);
                    new_cursor = new_cursor.saturating_add(1);
                }
                LineKind::NoNewline => {}
            }
            if line.text.starts_with(b"Subproject commit ") {
                submodule = true;
            }
        }
    }

    if binary {
        return Err(AppError::other(
            "Binary files cannot be partially staged or discarded.",
        ));
    }
    if submodule {
        return Err(AppError::other(
            "Submodule changes must be staged or discarded as a whole file.",
        ));
    }
    if rename {
        return Err(AppError::other(
            "Renamed or copied files must be staged or discarded as a whole file.",
        ));
    }
    if mode_change {
        return Err(AppError::other(
            "File mode changes must be staged or discarded as a whole file.",
        ));
    }

    Ok(ParsedFile {
        header,
        hunks,
        new_file,
        deleted_file,
    })
}

fn normalize_text(bytes: &[u8]) -> Vec<u8> {
    let mut end = bytes.len();
    while end > 0 && (bytes[end - 1] == b'\n' || bytes[end - 1] == b'\r') {
        end -= 1;
    }
    bytes[..end].to_vec()
}

fn line_key(line: &ParsedLine) -> (LineKind, Option<u32>, Option<u32>) {
    (line.kind, line.old_no, line.new_no)
}

fn header_path_token(header: &[Vec<u8>], marker: &[u8]) -> Option<Vec<u8>> {
    for line in header {
        let bare = strip_line_end(line);
        if bare.starts_with(marker) {
            return Some(bare[marker.len()..].to_vec());
        }
    }
    None
}

/// Token appears as `b/foo`, `"b/foo bar"`, or `/dev/null`.
fn retarget_token(token: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    if token.starts_with(b"\"") {
        let inner = &token[1..];
        if inner.starts_with(from) {
            let mut out = b"\"".to_vec();
            out.extend_from_slice(to);
            out.extend_from_slice(&inner[from.len()..]);
            out
        } else {
            token.to_vec()
        }
    } else if token.starts_with(from) {
        let mut out = to.to_vec();
        out.extend_from_slice(&token[from.len()..]);
        out
    } else {
        token.to_vec()
    }
}

fn header_path(header: &[Vec<u8>], marker: &[u8]) -> Option<Vec<u8>> {
    header_path_token(header, marker)
}

fn file_mode(parsed: &ParsedFile) -> Vec<u8> {
    for line in &parsed.header {
        let bare = strip_line_end(line);
        if let Some(rest) = bare.strip_prefix(b"deleted file mode ") {
            return rest.to_vec();
        }
        if let Some(rest) = bare.strip_prefix(b"new file mode ") {
            return rest.to_vec();
        }
        if let Some(idx) = bare.iter().position(|b| *b == b' ') {
            let mode = &bare[idx + 1..];
            if mode.len() == 6 && mode.iter().all(|b| (b'0'..=b'7').contains(b)) {
                return mode.to_vec();
            }
        }
    }
    b"100644".to_vec()
}

/// Build the header for the patch. Some cases need the file-level metadata to
/// change because a partial operation is no longer a whole-file add/delete.
fn build_header(parsed: &ParsedFile, target: PatchTarget, all_file_lines_selected: bool) -> Vec<u8> {
    let mut out = Vec::new();

    // Unstage a whole added/deleted file: reset the index entry instead of
    // applying a reverse patch (unborn-HEAD safe in the caller).
    if target == PatchTarget::Unstage {
        if parsed.new_file && all_file_lines_selected {
            return Vec::new();
        }
        if parsed.deleted_file && all_file_lines_selected {
            return Vec::new();
        }
    }

    let new_to_modified = parsed.new_file
        && (target == PatchTarget::Unstage || target == PatchTarget::Discard)
        && !all_file_lines_selected;
    let deleted_to_modified = parsed.deleted_file
        && target == PatchTarget::Stage
        && !all_file_lines_selected;

    let plus_token = header_path(&parsed.header, b"+++ ");
    let minus_token = header_path(&parsed.header, b"--- ");

    for line in &parsed.header {
        let bare = strip_line_end(line);
        if bare.starts_with(b"diff --git ") {
            out.extend_from_slice(line);
            continue;
        }
        if new_to_modified {
            if bare.starts_with(b"new file mode") {
                continue;
            }
            if bare.starts_with(b"--- ") && bare == b"--- /dev/null" {
                if let Some(token) = &plus_token {
                    out.extend_from_slice(b"--- ");
                    out.extend_from_slice(&retarget_token(token, b"b/", b"a/"));
                    out.push(b'\n');
                    continue;
                }
            }
            out.extend_from_slice(line);
            continue;
        }
        if deleted_to_modified {
            if bare.starts_with(b"deleted file mode") {
                continue;
            }
            if bare.starts_with(b"+++ ") && bare == b"+++ /dev/null" {
                if let Some(token) = &minus_token {
                    out.extend_from_slice(b"+++ ");
                    out.extend_from_slice(&retarget_token(token, b"a/", b"b/"));
                    out.push(b'\n');
                    continue;
                }
            }
            out.extend_from_slice(line);
            continue;
        }
        out.extend_from_slice(line);
    }

    out
}

/// Build a new-file patch containing the selected deleted lines. Used to
/// unstage part of a staged deletion: Git cannot reverse-create an index entry,
/// but it can create a new partial file from the lines we want to keep.
fn build_partial_restore_new_file(
    parsed: &ParsedFile,
    selected: &[(usize, usize)],
) -> Result<Vec<u8>, AppError> {
    let a_token = header_path(&parsed.header, b"--- ")
        .ok_or_else(|| AppError::other("Cannot determine path from the diff header"))?;
    let b_token = retarget_token(&a_token, b"a/", b"b/");
    let mode = file_mode(parsed);

    let mut body: Vec<(Vec<u8>, Option<Vec<u8>>)> = Vec::new();
    for (h_idx, l_idx) in selected {
        let line = parsed
            .hunks
            .get(*h_idx)
            .and_then(|h| h.lines.get(*l_idx))
            .ok_or_else(|| AppError::other("Selected line no longer exists in the diff"))?;
        if line.kind != LineKind::Del {
            continue;
        }
        let bare = strip_line_end(&line.raw);
        let mut plus = Vec::with_capacity(bare.len());
        plus.push(b'+');
        plus.extend_from_slice(&bare[1..]);
        plus.push(b'\n');
        let marker = line.marker.clone();
        body.push((plus, marker));
    }

    if body.is_empty() {
        return Err(AppError::other(
            "Select at least one deleted line to unstage.",
        ));
    }

    let count = body.len();
    let mut out = Vec::new();
    out.extend_from_slice(b"diff --git ");
    out.extend_from_slice(&a_token);
    out.push(b' ');
    out.extend_from_slice(&b_token);
    out.push(b'\n');
    out.extend_from_slice(b"new file mode ");
    out.extend_from_slice(&mode);
    out.push(b'\n');
    out.extend_from_slice(b"--- /dev/null\n+++ ");
    out.extend_from_slice(&b_token);
    out.push(b'\n');
    out.extend_from_slice(format!("@@ -0,0 +1,{} @@\n", count).as_bytes());
    for (idx, (line, marker)) in body.iter().enumerate() {
        out.extend_from_slice(line);
        // Only the final line of a file can carry the no-newline marker.
        if idx + 1 == body.len() {
            if let Some(marker) = marker {
                out.extend_from_slice(marker);
            }
        }
    }

    Ok(out)
}

fn selected_set<'a>(
    parsed: &ParsedFile,
    selected: &'a [SelectedLine],
) -> Result<Vec<(usize, usize)>, AppError> {
    let mut index = std::collections::HashMap::new();
    for (h_idx, hunk) in parsed.hunks.iter().enumerate() {
        for (l_idx, line) in hunk.lines.iter().enumerate() {
            if line.kind == LineKind::Add || line.kind == LineKind::Del {
                index.insert(line_key(line), (h_idx, l_idx));
            }
        }
    }

    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for sel in selected {
        if sel.kind != LineKind::Add && sel.kind != LineKind::Del {
            continue;
        }
        let key = (sel.kind, sel.old_no, sel.new_no);
        let (h_idx, l_idx) = index.get(&key).ok_or_else(|| {
            AppError::other(
                "The diff changed since it was displayed. Refresh the diff and try again. Nothing was changed.",
            )
        })?;
        if !seen.insert((*h_idx, *l_idx)) {
            continue;
        }
        let line = &parsed.hunks[*h_idx].lines[*l_idx];
        if normalize_text(&line.text) != normalize_text(sel.text.as_bytes()) {
            return Err(AppError::other(
                "The file changed since the diff was displayed. Refresh the diff and try again. Nothing was changed.",
            ));
        }
        out.push((*h_idx, *l_idx));
    }

    if out.is_empty() {
        return Err(AppError::other(
            "Select at least one added or removed line.",
        ));
    }

    out.sort_unstable();
    Ok(out)
}

/// True when every added/removed line in the parsed diff is selected.
fn all_file_lines_selected(parsed: &ParsedFile, selected: &[(usize, usize)]) -> bool {
    let selected_set: std::collections::HashSet<(usize, usize)> =
        selected.iter().copied().collect();
    let mut total = 0usize;
    for (h_idx, hunk) in parsed.hunks.iter().enumerate() {
        for (l_idx, line) in hunk.lines.iter().enumerate() {
            if line.kind == LineKind::Add || line.kind == LineKind::Del {
                total += 1;
                if !selected_set.contains(&(h_idx, l_idx)) {
                    return false;
                }
            }
        }
    }
    total > 0
}

/// A hunk that changes the final newline cannot be split safely: dropping one
/// side of a replacement produces a syntactically valid but wrong file. Require
/// the whole hunk up front rather than silently corrupting the file.
fn validate_no_newline_groups(parsed: &ParsedFile, selected: &[(usize, usize)]) -> Result<(), AppError> {
    let selected_set: std::collections::HashSet<(usize, usize)> =
        selected.iter().copied().collect();
    for (h_idx, hunk) in parsed.hunks.iter().enumerate() {
        let has_marker = hunk.lines.iter().any(|l| l.marker.is_some());
        if !has_marker {
            continue;
        }
        let any_selected = hunk
            .lines
            .iter()
            .enumerate()
            .any(|(l_idx, l)| {
                (l.kind == LineKind::Add || l.kind == LineKind::Del)
                    && selected_set.contains(&(h_idx, l_idx))
            });
        if !any_selected {
            continue;
        }
        for (l_idx, line) in hunk.lines.iter().enumerate() {
            if (line.kind == LineKind::Add || line.kind == LineKind::Del)
                && !selected_set.contains(&(h_idx, l_idx))
            {
                return Err(AppError::other(
                    "This hunk changes the file's final newline, so it can only be applied as a whole. Select the whole hunk and try again.",
                ));
            }
        }
    }
    Ok(())
}

/// Reject selections that would leave a file in a state the patch format
/// cannot express.
fn validate_target(
    parsed: &ParsedFile,
    target: PatchTarget,
    all_selected: bool,
) -> Result<(), AppError> {
    if parsed.deleted_file && target == PatchTarget::Discard && !all_selected {
        return Err(AppError::other(
            "Only the whole deletion of this file can be discarded. Select the entire file's deleted lines.",
        ));
    }
    Ok(())
}

#[derive(Default)]
struct EmitGroup {
    indices: Vec<usize>,
    first_del_old: Option<u32>,
    /// Position on the postimage of *our* patch (used for forward patches).
    first_add_patch_new: Option<u32>,
    /// Position on the original diff's new side (used for reverse patches).
    first_add_orig_new: Option<u32>,
}

struct Cursors {
    orig_old: u32,
    orig_new: u32,
    patch_old: u32,
    patch_new: u32,
}

/// Returns the net line delta of the emitted group.
fn flush_group(
    hunk: &ParsedHunk,
    group: &mut EmitGroup,
    out: &mut Vec<u8>,
    cursors: &Cursors,
    target: PatchTarget,
) -> i64 {
    if group.indices.is_empty() {
        return 0;
    }
    // A pure insertion uses the line it is placed after; a pure deletion uses
    // the line before the hole it creates.
    let (old_start, new_start) = match target {
        PatchTarget::Stage => (
            group
                .first_del_old
                .unwrap_or_else(|| cursors.patch_old.saturating_sub(1)),
            group
                .first_add_patch_new
                .unwrap_or_else(|| cursors.patch_new.saturating_sub(1)),
        ),
        PatchTarget::Unstage | PatchTarget::Discard => (
            group
                .first_del_old
                .unwrap_or_else(|| cursors.orig_old.saturating_sub(1)),
            group
                .first_add_orig_new
                .unwrap_or_else(|| cursors.orig_new.saturating_sub(1)),
        ),
    };
    let old_lines = group
        .indices
        .iter()
        .filter(|i| hunk.lines[**i].kind == LineKind::Del)
        .count();
    let new_lines = group
        .indices
        .iter()
        .filter(|i| hunk.lines[**i].kind == LineKind::Add)
        .count();
    out.extend_from_slice(
        format!(
            "@@ -{},{} +{},{} @@",
            old_start, old_lines, new_start, new_lines
        )
        .as_bytes(),
    );
    out.extend_from_slice(&hunk.suffix);
    for idx in group.indices.iter() {
        let line = &hunk.lines[*idx];
        out.extend_from_slice(&line.raw);
        if let Some(marker) = &line.marker {
            out.extend_from_slice(marker);
        }
    }
    let delta = new_lines as i64 - old_lines as i64;
    group.indices.clear();
    group.first_del_old = None;
    group.first_add_patch_new = None;
    group.first_add_orig_new = None;
    delta
}

/// Synthesize a patch from raw `git diff -U0` output.
pub fn synthesize(
    raw: &[u8],
    selected: &[SelectedLine],
    target: PatchTarget,
) -> Result<Synthesized, AppError> {
    let parsed = parse_file_diff(raw)?;
    let selected = selected_set(&parsed, selected)?;
    validate_no_newline_groups(&parsed, &selected)?;
    let all_selected = all_file_lines_selected(&parsed, &selected);

    if target == PatchTarget::Unstage && parsed.deleted_file && !all_selected {
        return Ok(Synthesized::AddFile(build_partial_restore_new_file(
            &parsed, &selected,
        )?));
    }

    if target == PatchTarget::Unstage
        && ((parsed.new_file && all_selected) || (parsed.deleted_file && all_selected))
    {
        return Ok(Synthesized::ResetIndex);
    }

    validate_target(&parsed, target, all_selected)?;

    let header = build_header(&parsed, target, all_selected);
    if header.is_empty() {
        return Ok(Synthesized::ResetIndex);
    }

    let selected_set: std::collections::HashSet<(usize, usize)> =
        selected.iter().copied().collect();
    let mut out = header;
    // Net effect of everything emitted so far. It maps preimage line numbers
    // to the postimage of *our* patch, which intentionally ignores unselected
    // additions/deletions from the displayed diff.
    let mut net_delta: i64 = 0;

    for (h_idx, hunk) in parsed.hunks.iter().enumerate() {
        let orig_old = if hunk.old_lines == 0 {
            hunk.old_start.saturating_add(1)
        } else {
            hunk.old_start
        };
        let orig_new = if hunk.new_lines == 0 {
            hunk.new_start.saturating_add(1)
        } else {
            hunk.new_start
        };
        let mut cursors = Cursors {
            orig_old,
            orig_new,
            patch_old: orig_old,
            patch_new: (orig_old as i64 + net_delta).max(1) as u32,
        };
        let mut group = EmitGroup::default();

        for (l_idx, line) in hunk.lines.iter().enumerate() {
            let is_selected = selected_set.contains(&(h_idx, l_idx));
            let is_change = line.kind == LineKind::Add || line.kind == LineKind::Del;

            if is_selected && is_change {
                let previous_selected = l_idx > 0
                    && selected_set.contains(&(h_idx, l_idx - 1))
                    && (hunk.lines[l_idx - 1].kind == LineKind::Add
                        || hunk.lines[l_idx - 1].kind == LineKind::Del);
                if !previous_selected {
                    net_delta += flush_group(hunk, &mut group, &mut out, &cursors, target);
                }
                if line.kind == LineKind::Del && group.first_del_old.is_none() {
                    group.first_del_old = line.old_no;
                }
                if line.kind == LineKind::Add && group.first_add_patch_new.is_none() {
                    group.first_add_patch_new = Some(cursors.patch_new);
                    group.first_add_orig_new = Some(cursors.orig_new);
                }
                group.indices.push(l_idx);
            } else {
                net_delta += flush_group(hunk, &mut group, &mut out, &cursors, target);
            }

            match line.kind {
                LineKind::Add => {
                    cursors.orig_new = cursors.orig_new.saturating_add(1);
                    if is_selected {
                        cursors.patch_new = cursors.patch_new.saturating_add(1);
                    }
                }
                LineKind::Del => {
                    cursors.orig_old = cursors.orig_old.saturating_add(1);
                    cursors.patch_old = cursors.patch_old.saturating_add(1);
                    if !is_selected {
                        // An unselected deletion stays in the preimage, so the
                        // postimage of our patch still contains it.
                        cursors.patch_new = cursors.patch_new.saturating_add(1);
                    }
                }
                LineKind::Context => {
                    cursors.orig_old = cursors.orig_old.saturating_add(1);
                    cursors.orig_new = cursors.orig_new.saturating_add(1);
                    cursors.patch_old = cursors.patch_old.saturating_add(1);
                    cursors.patch_new = cursors.patch_new.saturating_add(1);
                }
                LineKind::NoNewline => {}
            }
        }
        net_delta += flush_group(hunk, &mut group, &mut out, &cursors, target);
    }

    Ok(Synthesized::Patch(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sel(kind: LineKind, old_no: Option<u32>, new_no: Option<u32>, text: &str) -> SelectedLine {
        SelectedLine {
            kind,
            old_no,
            new_no,
            text: text.to_string(),
        }
    }

    const MODIFIED: &str = "\
diff --git a/f.txt b/f.txt
index 1111111..2222222 100644
--- a/f.txt
+++ b/f.txt
@@ -1,0 +2,1 @@
+inserted
@@ -3,1 +4,1 @@
-three
+THREE
";

    #[test]
    fn synthesizes_single_line_change_with_recounted_header() {
        let patch = synthesize(
            MODIFIED.as_bytes(),
            &[sel(LineKind::Del, Some(3), None, "three")],
            PatchTarget::Stage,
        )
        .unwrap();
        let Synthesized::Patch(bytes) = patch else {
            panic!("expected patch")
        };
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("@@ -3,1 +2,0 @@\n"));
        assert!(text.contains("-three\n"));
        assert!(!text.contains("+THREE"));
    }

    #[test]
    fn synthesizes_add_line_at_correct_position() {
        let patch = synthesize(
            MODIFIED.as_bytes(),
            &[sel(LineKind::Add, None, Some(2), "inserted")],
            PatchTarget::Stage,
        )
        .unwrap();
        let Synthesized::Patch(bytes) = patch else {
            panic!("expected patch")
        };
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("@@ -1,0 +2,1 @@\n"));
        assert!(text.contains("+inserted\n"));
    }

    #[test]
    fn groups_adjacent_replacement_lines() {
        let patch = synthesize(
            MODIFIED.as_bytes(),
            &[
                sel(LineKind::Del, Some(3), None, "three"),
                sel(LineKind::Add, None, Some(4), "THREE"),
            ],
            PatchTarget::Stage,
        )
        .unwrap();
        let Synthesized::Patch(bytes) = patch else {
            panic!("expected patch")
        };
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("@@ -3,1 +3,1 @@\n-three\n+THREE\n"));
    }

    #[test]
    fn refuses_selection_when_text_changed() {
        let err = synthesize(
            MODIFIED.as_bytes(),
            &[sel(LineKind::Del, Some(3), None, "different")],
            PatchTarget::Stage,
        )
        .unwrap_err();
        assert!(err.message.contains("changed since the diff was displayed"));
    }

    #[test]
    fn preserves_no_newline_marker_when_whole_hunk_selected() {
        let raw = "\
diff --git a/f.txt b/f.txt
--- a/f.txt
+++ b/f.txt
@@ -2,1 +2,1 @@
-b
\\ No newline at end of file
+B
\\ No newline at end of file
";
        let patch = synthesize(
            raw.as_bytes(),
            &[
                sel(LineKind::Del, Some(2), None, "b"),
                sel(LineKind::Add, None, Some(2), "B"),
            ],
            PatchTarget::Stage,
        )
        .unwrap();
        let Synthesized::Patch(bytes) = patch else {
            panic!("expected patch")
        };
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("-b\n\\ No newline at end of file\n"));
        assert!(text.contains("+B\n\\ No newline at end of file\n"));
    }

    #[test]
    fn refuses_partial_no_newline_hunk() {
        let raw = "\
diff --git a/f.txt b/f.txt
--- a/f.txt
+++ b/f.txt
@@ -2,1 +2,2 @@
-b
\\ No newline at end of file
+b
+c
\\ No newline at end of file
";
        let err = synthesize(
            raw.as_bytes(),
            &[sel(LineKind::Add, None, Some(3), "c")],
            PatchTarget::Stage,
        )
        .unwrap_err();
        assert!(err.message.contains("final newline"));
    }

    #[test]
    fn preserves_crlf_bytes() {
        let raw = "diff --git a/f.txt b/f.txt\r
--- a/f.txt\r
+++ b/f.txt\r
@@ -2,1 +2,1 @@\r
-b\r
+B\r
";
        // The actual diff separators are LF; build the fixture byte-exactly.
        let raw = raw.replace("\r\n", "\n");
        let raw = raw.replace("-b\n", "-b\r\n").replace("+B\n", "+B\r\n");
        let patch = synthesize(
            raw.as_bytes(),
            &[
                sel(LineKind::Del, Some(2), None, "b"),
                sel(LineKind::Add, None, Some(2), "B"),
            ],
            PatchTarget::Stage,
        )
        .unwrap();
        let Synthesized::Patch(bytes) = patch else {
            panic!("expected patch")
        };
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("-b\r\n+B\r\n"));
    }

    #[test]
    fn preserves_tabs_and_spaces() {
        let raw = "\
diff --git a/f.txt b/f.txt
--- a/f.txt
+++ b/f.txt
@@ -1,1 +1,1 @@
-\told value
+\tnew   value
";
        let patch = synthesize(
            raw.as_bytes(),
            &[
                sel(LineKind::Del, Some(1), None, "\told value"),
                sel(LineKind::Add, None, Some(1), "\tnew   value"),
            ],
            PatchTarget::Stage,
        )
        .unwrap();
        let Synthesized::Patch(bytes) = patch else {
            panic!("expected patch")
        };
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("-\told value\n+\tnew   value\n"));
    }

    #[test]
    fn partial_new_file_keeps_new_file_header() {
        let raw = "\
diff --git a/new.txt b/new.txt
new file mode 100644
index 0000000..4cb29ea
--- /dev/null
+++ b/new.txt
@@ -0,0 +1,3 @@
+one
+two
+three
";
        let patch = synthesize(
            raw.as_bytes(),
            &[
                sel(LineKind::Add, None, Some(1), "one"),
                sel(LineKind::Add, None, Some(2), "two"),
            ],
            PatchTarget::Stage,
        )
        .unwrap();
        let Synthesized::Patch(bytes) = patch else {
            panic!("expected patch")
        };
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("new file mode 100644"));
        assert!(text.contains("@@ -0,0 +1,2 @@\n+one\n+two\n"));
    }

    #[test]
    fn partial_new_file_unstage_rewrites_to_modified_header() {
        let raw = "\
diff --git a/new.txt b/new.txt
new file mode 100644
index 0000000..4cb29ea
--- /dev/null
+++ b/new.txt
@@ -0,0 +1,3 @@
+one
+two
+three
";
        let patch = synthesize(
            raw.as_bytes(),
            &[sel(LineKind::Add, None, Some(2), "two")],
            PatchTarget::Unstage,
        )
        .unwrap();
        let Synthesized::Patch(bytes) = patch else {
            panic!("expected patch")
        };
        let text = String::from_utf8(bytes).unwrap();
        assert!(!text.contains("new file mode"));
        assert!(text.contains("--- a/new.txt"));
        assert!(text.contains("+++ b/new.txt"));
        assert!(text.contains("@@ -0,0 +2,1 @@\n+two\n"));
    }

    #[test]
    fn whole_new_file_unstage_resets_index() {
        let raw = "\
diff --git a/new.txt b/new.txt
new file mode 100644
--- /dev/null
+++ b/new.txt
@@ -0,0 +1,1 @@
+one
";
        let patch = synthesize(
            raw.as_bytes(),
            &[sel(LineKind::Add, None, Some(1), "one")],
            PatchTarget::Unstage,
        )
        .unwrap();
        assert_eq!(patch, Synthesized::ResetIndex);
    }

    #[test]
    fn partial_deleted_file_stage_rewrites_to_modified_header() {
        let raw = "\
diff --git a/gone.txt b/gone.txt
deleted file mode 100644
index de98044..0000000
--- a/gone.txt
+++ /dev/null
@@ -1,3 +0,0 @@
-a
-b
-c
";
        let patch = synthesize(
            raw.as_bytes(),
            &[sel(LineKind::Del, Some(2), None, "b")],
            PatchTarget::Stage,
        )
        .unwrap();
        let Synthesized::Patch(bytes) = patch else {
            panic!("expected patch")
        };
        let text = String::from_utf8(bytes).unwrap();
        assert!(!text.contains("deleted file mode"));
        assert!(text.contains("--- a/gone.txt"));
        assert!(text.contains("+++ b/gone.txt"));
        assert!(text.contains("@@ -2,1 +1,0 @@\n-b\n"));
    }

    #[test]
    fn whole_deleted_file_stage_keeps_deleted_header() {
        let raw = "\
diff --git a/gone.txt b/gone.txt
deleted file mode 100644
--- a/gone.txt
+++ /dev/null
@@ -1,1 +0,0 @@
-a
";
        let patch = synthesize(
            raw.as_bytes(),
            &[sel(LineKind::Del, Some(1), None, "a")],
            PatchTarget::Stage,
        )
        .unwrap();
        let Synthesized::Patch(bytes) = patch else {
            panic!("expected patch")
        };
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("deleted file mode 100644"));
        assert!(text.contains("+++ /dev/null"));
    }

    #[test]
    fn partial_deleted_file_unstage_builds_partial_new_file() {
        let raw = "\
diff --git a/gone.txt b/gone.txt
deleted file mode 100644
index de98044..0000000
--- a/gone.txt
+++ /dev/null
@@ -1,3 +0,0 @@
-a
-b
-c
";
        let patch = synthesize(
            raw.as_bytes(),
            &[sel(LineKind::Del, Some(2), None, "b")],
            PatchTarget::Unstage,
        )
        .unwrap();
        let Synthesized::AddFile(bytes) = patch else {
            panic!("expected add-file patch")
        };
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("new file mode 100644"));
        assert!(text.contains("--- /dev/null"));
        assert!(text.contains("+++ b/gone.txt"));
        assert!(text.contains("@@ -0,0 +1,1 @@\n+b\n"));
    }

    #[test]
    fn refusing_partial_deleted_discard() {
        let raw = "\
diff --git a/gone.txt b/gone.txt
deleted file mode 100644
--- a/gone.txt
+++ /dev/null
@@ -1,2 +0,0 @@
-a
-b
";
        let err = synthesize(
            raw.as_bytes(),
            &[sel(LineKind::Del, Some(1), None, "a")],
            PatchTarget::Discard,
        )
        .unwrap_err();
        assert!(err.message.contains("whole deletion"));
    }

    #[test]
    fn unstage_replacement_in_multi_hunk_diff() {
        let raw = "\
diff --git a/f.txt b/f.txt
index 1111111..2222222 100644
--- a/f.txt
+++ b/f.txt
@@ -2 +2 @@
-b
+B
@@ -5 +5 @@
-e
+E
";
        let patch = synthesize(
            raw.as_bytes(),
            &[
                sel(LineKind::Del, Some(2), None, "b"),
                sel(LineKind::Add, None, Some(2), "B"),
            ],
            PatchTarget::Unstage,
        )
        .unwrap();
        let Synthesized::Patch(bytes) = patch else {
            panic!("expected patch")
        };
        let text = String::from_utf8(bytes).unwrap();
        println!("PATCH:\n{}", text);
        assert!(text.contains("@@ -2,1 +2,1 @@\n-b\n+B\n"));
        assert!(!text.contains("+E"));
    }

    #[test]
    fn refuses_binary_diff() {
        let raw = "\
diff --git a/img.png b/img.png
index 1111111..2222222 100644
Binary files a/img.png and b/img.png differ
";
        let err = synthesize(raw.as_bytes(), &[], PatchTarget::Stage).unwrap_err();
        assert!(err.message.contains("Binary"));
    }

    #[test]
    fn refuses_rename_diff() {
        let raw = "\
diff --git a/old.txt b/new.txt
similarity index 100%
rename from old.txt
rename to new.txt
";
        let err = synthesize(raw.as_bytes(), &[], PatchTarget::Stage).unwrap_err();
        assert!(err.message.contains("Renamed"));
    }

    #[test]
    fn refuses_submodule_diff() {
        let raw = "\
diff --git a/sub b/sub
index 1111111..2222222 160000
--- a/sub
+++ b/sub
@@ -1,1 +1,1 @@
-Subproject commit 1111111111111111111111111111111111111111
+Subproject commit 2222222222222222222222222222222222222222
";
        let err = synthesize(
            raw.as_bytes(),
            &[
                sel(
                    LineKind::Del,
                    Some(1),
                    None,
                    "Subproject commit 1111111111111111111111111111111111111111",
                ),
                sel(
                    LineKind::Add,
                    None,
                    Some(1),
                    "Subproject commit 2222222222222222222222222222222222222222",
                ),
            ],
            PatchTarget::Stage,
        )
        .unwrap_err();
        assert!(err.message.contains("Submodule"));
    }
}
