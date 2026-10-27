import { describe, it, expect } from "vitest";
import {
  formatStashIndex,
  formatStashLabel,
  summarizeStashSubject,
} from "../src/lib/domain/stash";
import type { StashEntry } from "../src/lib/platform/types";

function entry(partial: Partial<StashEntry>): StashEntry {
  return {
    index: 0,
    sha: "abc1234",
    subject: "WIP on main: abc1234 add auth",
    date: "2026-10-19T10:00:00+00:00",
    ...partial,
  };
}

describe("formatStashIndex", () => {
  it("renders the canonical stash selector", () => {
    expect(formatStashIndex(0)).toBe("stash@{0}");
    expect(formatStashIndex(12)).toBe("stash@{12}");
  });
});

describe("formatStashLabel", () => {
  it("combines selector and subject", () => {
    expect(formatStashLabel(entry({ index: 2, subject: "On main: fix auth" }))).toBe(
      "stash@{2} · On main: fix auth"
    );
  });
});

describe("summarizeStashSubject", () => {
  it("strips the WIP prefix and short sha", () => {
    expect(
      summarizeStashSubject("WIP on main: 7f3a2c1 add auth middleware")
    ).toBe("On main: add auth middleware");
  });

  it("keeps a custom message", () => {
    expect(summarizeStashSubject("On feature/api: my message")).toBe(
      "On feature/api: my message"
    );
  });

  it("drops the sha when there is no message body", () => {
    expect(summarizeStashSubject("WIP on main: 7f3a2c1")).toBe("On main");
  });

  it("returns unrecognised subjects unchanged", () => {
    expect(summarizeStashSubject("something custom")).toBe("something custom");
  });

  it("handles empty input safely", () => {
    expect(summarizeStashSubject("")).toBe("");
  });
});
