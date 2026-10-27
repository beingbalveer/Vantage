import { describe, it, expect } from "vitest";
import {
  canAmendLastCommit,
  canUndoLastCommit,
  undoCommitMessage,
  amendWarning,
} from "../src/lib/domain/commitActions";

describe("commitActions guards", () => {
  describe("canAmendLastCommit", () => {
    it("allows amend when HEAD exists and no operation is in progress", () => {
      expect(canAmendLastCommit({ sha: "abc123" }, null)).toEqual({ ok: true });
    });

    it("refuses when there is no HEAD (unborn branch)", () => {
      const res = canAmendLastCommit(null, null);
      expect(res.ok).toBe(false);
      if (!res.ok) expect(res.reason).toContain("no commits to amend");
    });

    it("refuses mid-merge and names the operation", () => {
      const res = canAmendLastCommit({ sha: "abc123" }, "merge");
      expect(res.ok).toBe(false);
      if (!res.ok) expect(res.reason).toBe(
        "A merge is in progress. Finish or abort it in the terminal before amending."
      );
    });

    it("refuses mid-rebase", () => {
      const res = canAmendLastCommit({ sha: "abc123" }, "rebase");
      expect(res.ok).toBe(false);
    });
  });

  describe("canUndoLastCommit", () => {
    it("allows undo when HEAD has a parent", () => {
      expect(canUndoLastCommit(true, true, null)).toEqual({ ok: true });
    });

    it("refuses when there are no commits", () => {
      const res = canUndoLastCommit(false, false, null);
      expect(res.ok).toBe(false);
      if (!res.ok) expect(res.reason).toContain("no commits to undo");
    });

    it("refuses to undo the initial commit (no parent)", () => {
      const res = canUndoLastCommit(true, false, null);
      expect(res.ok).toBe(false);
      if (!res.ok) expect(res.reason).toContain("initial commit");
    });

    it("refuses mid-rebase", () => {
      const res = canUndoLastCommit(true, true, "rebase");
      expect(res.ok).toBe(false);
      if (!res.ok) expect(res.reason).toContain("before undoing a commit");
    });
  });
});

describe("undoCommitMessage", () => {
  it("names the undone commit and says the message is preserved", () => {
    expect(undoCommitMessage({ short: "7f3a2c1", subject: "fix typo" })).toBe(
      "Undid commit 7f3a2c1 (fix typo). Its changes are staged and the message is ready to re-commit."
    );
  });
});

describe("amendWarning", () => {
  it("warns when the commit is already pushed", () => {
    const warning = amendWarning(true);
    expect(warning).toBeTruthy();
    expect(warning!.toLowerCase()).toContain("force push");
  });

  it("stays quiet for local-only commits", () => {
    expect(amendWarning(false)).toBeNull();
  });
});
