import { describe, it, expect } from "vitest";
import {
  validateBranchName,
  isValidBranchName,
  describeStartPoint,
} from "../src/lib/domain/branchName";

describe("validateBranchName", () => {
  it("accepts ordinary branch names", () => {
    for (const name of [
      "main",
      "feature/ALPH-152-branch-crud",
      "fix/gh-42",
      "release-0.1.1",
      "user.name/experiment",
      "a",
    ]) {
      expect(validateBranchName(name), name).toBeNull();
    }
  });

  it("trims surrounding whitespace before validating", () => {
    expect(validateBranchName("  feature/x  ")).toBeNull();
  });

  it("rejects empty and whitespace-only names", () => {
    expect(validateBranchName("")).toMatch(/empty/i);
    expect(validateBranchName("   ")).toMatch(/empty/i);
  });

  it("rejects a leading dash", () => {
    expect(validateBranchName("-bad")).toMatch(/start with '-'/);
  });

  it("rejects the bare '@'", () => {
    expect(validateBranchName("@")).toMatch(/not a valid branch name/);
  });

  it("rejects spaces and control characters", () => {
    expect(validateBranchName("has space")).toMatch(/spaces/i);
    expect(validateBranchName("tab\there")).toBeTruthy();
    expect(validateBranchName("new\nline")).toBeTruthy();
  });

  it("rejects git special characters", () => {
    for (const name of ["a~b", "a^b", "a:b", "a?b", "a*b", "a[b", "a\\b"]) {
      expect(validateBranchName(name), name).toBeTruthy();
    }
  });

  it("rejects '..' and '@{'", () => {
    expect(validateBranchName("a..b")).toMatch(/\.\./);
    expect(validateBranchName("a@{b}")).toMatch(/@\{/);
  });

  it("rejects leading/trailing slashes and empty segments", () => {
    expect(validateBranchName("/main")).toMatch(/start or end/i);
    expect(validateBranchName("main/")).toMatch(/start or end/i);
    expect(validateBranchName("a//b")).toMatch(/empty path segment|\/\//);
  });

  it("rejects trailing dots and .lock", () => {
    expect(validateBranchName("main.")).toMatch(/end with '\.'/);
    expect(validateBranchName("main.lock")).toMatch(/\.lock/);
    expect(validateBranchName("main.lock/child")).toMatch(/\.lock/);
  });

  it("rejects segments starting with a dot", () => {
    expect(validateBranchName(".hidden")).toMatch(/start with '\.'/);
    expect(validateBranchName("feature/.hidden")).toMatch(/start with '\.'/);
  });

  it("has a single-source isValidBranchName helper", () => {
    expect(isValidBranchName("feature/x")).toBe(true);
    expect(isValidBranchName("bad name")).toBe(false);
  });
});

describe("describeStartPoint", () => {
  it("labels HEAD with the current branch", () => {
    expect(describeStartPoint("HEAD", "main")).toBe("HEAD (main)");
  });

  it("falls back to plain HEAD without a branch", () => {
    expect(describeStartPoint("", null)).toBe("HEAD");
    expect(describeStartPoint("HEAD")).toBe("HEAD");
  });

  it("passes through an explicit sha or branch", () => {
    expect(describeStartPoint("7f3a2c1", "main")).toBe("7f3a2c1");
    expect(describeStartPoint("origin/main", "main")).toBe("origin/main");
  });
});
