import { describe, it, expect } from "vitest";
import {
  HISTORY_PAGE_SIZE,
  formatHistoryFooter,
  hasMoreHistory,
} from "../src/lib/domain/history";

describe("history footer label", () => {
  it("exposes the 200-commit page size", () => {
    expect(HISTORY_PAGE_SIZE).toBe(200);
  });

  it("shows loaded of total when more pages remain (no lying 200+)", () => {
    expect(formatHistoryFooter({ total: 1234, capped: false, loaded: 200 }, false)).toBe(
      "Showing 200 of 1234 commits"
    );
  });

  it("shows the plain count once fully loaded", () => {
    expect(formatHistoryFooter({ total: 3, capped: false, loaded: 3 }, false)).toBe(
      "3 commits on this branch"
    );
    expect(formatHistoryFooter({ total: 1, capped: false, loaded: 1 }, false)).toBe(
      "1 commit on this branch"
    );
  });

  it("is empty for an empty repo", () => {
    expect(formatHistoryFooter({ total: 0, capped: false, loaded: 0 }, false)).toBe("");
  });

  it("shows the match count when filtering", () => {
    expect(formatHistoryFooter({ total: 12, capped: false, loaded: 12 }, true)).toBe("12 matches");
    expect(formatHistoryFooter({ total: 1, capped: false, loaded: 1 }, true)).toBe("1 match");
  });

  it("flags a capped search with a plus", () => {
    expect(formatHistoryFooter({ total: 2000, capped: true, loaded: 200 }, true)).toBe(
      "2000+ matches"
    );
  });

  it("is empty for a filter with no results", () => {
    expect(formatHistoryFooter({ total: 0, capped: false, loaded: 0 }, true)).toBe("");
  });
});

describe("hasMoreHistory", () => {
  it("is true while pages remain", () => {
    expect(hasMoreHistory({ total: 400, capped: false, loaded: 200 })).toBe(true);
  });
  it("is false once fully loaded", () => {
    expect(hasMoreHistory({ total: 200, capped: false, loaded: 200 })).toBe(false);
    expect(hasMoreHistory({ total: 0, capped: false, loaded: 0 })).toBe(false);
  });
});
