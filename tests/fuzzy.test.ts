import { describe, it, expect } from "vitest";
import { fuzzyScore } from "../src/lib/fuzzy";

describe("fuzzyScore", () => {
  it("matches empty query", () => {
    const res = fuzzyScore("", "api-server");
    expect(res).not.toBeNull();
    expect(res?.score).toBe(1);
  });

  it("fails when query longer than text", () => {
    const res = fuzzyScore("longqueryhere", "short");
    expect(res).toBeNull();
  });

  it("matches subsequence with word-start bonuses", () => {
    const res = fuzzyScore("api", "api-server");
    expect(res).not.toBeNull();
    expect(res?.indices).toEqual([0, 1, 2]);
  });

  it("matches camelCase word starts", () => {
    const res = fuzzyScore("as", "apiServer");
    expect(res).not.toBeNull();
    expect(res?.indices).toEqual([0, 3]);
  });

  it("returns null on non-matching string", () => {
    const res = fuzzyScore("xyz", "api-server");
    expect(res).toBeNull();
  });
});
