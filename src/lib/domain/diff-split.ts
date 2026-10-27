// Row pairing for the side-by-side (split) diff view (ALPH-158).
//
// Pure transformation of a unified hunk's lines into aligned old/new rows so the
// pairing logic can be unit tested without a DOM. Deletions land on the left,
// additions on the right, context on both, and unmatched lines get an empty
// placeholder cell opposite them.

import type { DiffLine } from "../platform/types";

export type SplitCellKind = "context" | "add" | "del" | "empty";

export interface SplitCell {
  kind: SplitCellKind;
  oldNo: number | null;
  newNo: number | null;
  text: string;
  wordRanges: [number, number][] | null;
}

export type SplitRow =
  | { type: "line"; left: SplitCell; right: SplitCell }
  | { type: "note"; text: string };

function emptyCell(): SplitCell {
  return { kind: "empty", oldNo: null, newNo: null, text: "", wordRanges: null };
}

function cellFrom(line: DiffLine, kind: SplitCellKind): SplitCell {
  return {
    kind,
    oldNo: line.oldNo,
    newNo: line.newNo,
    text: line.text,
    wordRanges: line.wordRanges,
  };
}

function lineRow(left: SplitCell, right: SplitCell): SplitRow {
  return { type: "line", left, right };
}

/**
 * Pair a hunk's lines into aligned rows.
 *
 * * context lines share a row on both sides,
 * * a change block's deletions and additions are paired index-by-index, with
 *   the longer side padded by empty placeholder cells (no-newline markers do
 *   not break the pairing),
 * * `\ No newline at end of file` markers become full-width notes,
 * * an addition with no deletion (and vice versa) still aligns.
 */
export function buildSplitRows(lines: DiffLine[]): SplitRow[] {
  const rows: SplitRow[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    if (line.kind === "context") {
      const c = cellFrom(line, "context");
      rows.push(lineRow(c, c));
      i++;
      continue;
    }

    // Collect a contiguous change block (deletions, additions and no-newline
    // notes) so markers between the two sides cannot break the pairing.
    const dels: DiffLine[] = [];
    const adds: DiffLine[] = [];
    const notes: string[] = [];
    while (i < lines.length && lines[i].kind !== "context") {
      const current = lines[i];
      if (current.kind === "del") {
        dels.push(current);
      } else if (current.kind === "add") {
        adds.push(current);
      } else if (current.kind === "noNewline") {
        notes.push(current.text);
      }
      i++;
    }

    const pairCount = Math.max(dels.length, adds.length);
    for (let k = 0; k < pairCount; k++) {
      const left = dels[k] ? cellFrom(dels[k], "del") : emptyCell();
      const right = adds[k] ? cellFrom(adds[k], "add") : emptyCell();
      rows.push(lineRow(left, right));
    }
    for (const text of notes) {
      rows.push({ type: "note", text });
    }
  }

  return rows;
}
