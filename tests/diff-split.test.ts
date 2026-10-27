import { describe, it, expect } from "vitest";
import { buildSplitRows, type SplitRow } from "../src/lib/domain/diff-split";
import type { DiffLine, LineKind } from "../src/lib/platform/types";

function line(
  kind: LineKind,
  oldNo: number | null,
  newNo: number | null,
  text: string,
  wordRanges: [number, number][] | null = null
): DiffLine {
  return { kind, oldNo, newNo, text, wordRanges };
}

function lineRows(rows: SplitRow[]) {
  return rows.filter((r): r is Extract<SplitRow, { type: "line" }> => r.type === "line");
}

describe("buildSplitRows", () => {
  it("returns no rows for an empty hunk", () => {
    expect(buildSplitRows([])).toEqual([]);
  });

  it("shares context lines on both sides", () => {
    const rows = buildSplitRows([line("context", 7, 7, "unchanged")]);
    expect(rows).toHaveLength(1);
    expect(rows[0]).toEqual({
      type: "line",
      left: { kind: "context", oldNo: 7, newNo: 7, text: "unchanged", wordRanges: null },
      right: { kind: "context", oldNo: 7, newNo: 7, text: "unchanged", wordRanges: null },
    });
  });

  it("pairs a deletion run with the following addition run index by index", () => {
    const rows = lineRows(
      buildSplitRows([
        line("del", 10, null, "old one"),
        line("del", 11, null, "old two"),
        line("add", null, 10, "new one"),
        line("add", null, 11, "new two"),
      ])
    );

    expect(rows).toHaveLength(2);
    expect(rows[0].left.kind).toBe("del");
    expect(rows[0].left.oldNo).toBe(10);
    expect(rows[0].right.kind).toBe("add");
    expect(rows[0].right.newNo).toBe(10);
    expect(rows[0].right.text).toBe("new one");
    expect(rows[1].left.text).toBe("old two");
    expect(rows[1].right.text).toBe("new two");
  });

  it("pads the right side when the deletion run is longer", () => {
    const rows = lineRows(
      buildSplitRows([
        line("del", 1, null, "a"),
        line("del", 2, null, "b"),
        line("del", 3, null, "c"),
        line("add", null, 1, "x"),
      ])
    );

    expect(rows).toHaveLength(3);
    expect(rows[2].left.kind).toBe("del");
    expect(rows[2].left.text).toBe("c");
    expect(rows[2].right.kind).toBe("empty");
    expect(rows[2].right.oldNo).toBeNull();
    expect(rows[2].right.newNo).toBeNull();
  });

  it("pads the left side when the addition run is longer", () => {
    const rows = lineRows(
      buildSplitRows([
        line("del", 1, null, "a"),
        line("add", null, 1, "x"),
        line("add", null, 2, "y"),
        line("add", null, 3, "z"),
      ])
    );

    expect(rows).toHaveLength(3);
    expect(rows[0].left.kind).toBe("del");
    expect(rows[2].left.kind).toBe("empty");
    expect(rows[2].right.kind).toBe("add");
    expect(rows[2].right.newNo).toBe(3);
  });

  it("keeps a standalone addition on the right with an empty left cell", () => {
    const rows = lineRows(
      buildSplitRows([
        line("context", 1, 1, "start"),
        line("add", null, 2, "inserted"),
        line("context", 2, 3, "end"),
      ])
    );

    expect(rows).toHaveLength(3);
    expect(rows[1].left.kind).toBe("empty");
    expect(rows[1].right.kind).toBe("add");
    expect(rows[1].right.newNo).toBe(2);
    expect(rows[2].left.oldNo).toBe(2);
    expect(rows[2].right.newNo).toBe(3);
  });

  it("keeps the two sides aligned across multiple change blocks", () => {
    const rows = lineRows(
      buildSplitRows([
        line("del", 1, null, "a"),
        line("add", null, 1, "A"),
        line("context", 2, 2, "same"),
        line("add", null, 3, "B"),
        line("context", 3, 4, "same again"),
        line("del", 4, null, "d"),
        line("add", null, 5, "D"),
      ])
    );

    expect(rows).toHaveLength(5);
    expect(rows[0].left.kind).toBe("del");
    expect(rows[0].right.kind).toBe("add");
    expect(rows[1].left.kind).toBe("context");
    expect(rows[1].right.kind).toBe("context");
    expect(rows[2].left.kind).toBe("empty");
    expect(rows[2].right.kind).toBe("add");
    expect(rows[3].left.oldNo).toBe(3);
    expect(rows[3].right.newNo).toBe(4);
    expect(rows[4].left.kind).toBe("del");
    expect(rows[4].right.kind).toBe("add");
    expect(rows[4].left.oldNo).toBe(4);
    expect(rows[4].right.newNo).toBe(5);
  });

  it("turns no-newline markers into full-width notes", () => {
    const rows = buildSplitRows([
      line("del", 1, null, "old"),
      line("add", null, 1, "new"),
      line("noNewline", null, null, "\\ No newline at end of file"),
    ]);

    expect(rows).toHaveLength(2);
    expect(rows[1]).toEqual({
      type: "note",
      text: "\\ No newline at end of file",
    });
  });

  it("keeps deletion/addition pairing across interleaved no-newline markers", () => {
    const rows = buildSplitRows([
      line("del", 1, null, "old"),
      line("noNewline", null, null, "\\ No newline at end of file"),
      line("add", null, 1, "new"),
      line("noNewline", null, null, "\\ No newline at end of file"),
    ]);

    expect(rows).toHaveLength(3);
    expect(rows[0].type).toBe("line");
    if (rows[0].type === "line") {
      expect(rows[0].left.kind).toBe("del");
      expect(rows[0].right.kind).toBe("add");
      expect(rows[0].right.text).toBe("new");
    }
    expect(rows[1].type).toBe("note");
    expect(rows[2].type).toBe("note");
  });

  it("preserves word-level ranges on paired cells", () => {
    const rows = lineRows(
      buildSplitRows([
        line("del", 1, null, "const ttl = 3600", [[12, 16]]),
        line("add", null, 1, "const ttl = 900", [[12, 15]]),
      ])
    );

    expect(rows[0].left.wordRanges).toEqual([[12, 16]]);
    expect(rows[0].right.wordRanges).toEqual([[12, 15]]);
  });
});
