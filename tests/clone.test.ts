import { describe, it, expect } from "vitest";
import {
  suggestFolderName,
  isSafeFolderName,
  progressMessage,
  validateDestination,
  type CloneDestinationState,
} from "../src/lib/clone";

const dest = (overrides: Partial<CloneDestinationState> = {}): CloneDestinationState => ({
  exists: false,
  isDir: false,
  isEmpty: true,
  insideRepo: false,
  ...overrides,
});

describe("suggestFolderName", () => {
  it("derives the name from https URLs", () => {
    expect(suggestFolderName("https://github.com/owner/repo.git")).toBe("repo");
    expect(suggestFolderName("https://github.com/owner/repo")).toBe("repo");
    expect(suggestFolderName("https://github.com/owner/repo/")).toBe("repo");
  });

  it("derives the name from scp-style and ssh URLs", () => {
    expect(suggestFolderName("git@github.com:owner/repo.git")).toBe("repo");
    expect(suggestFolderName("git@github.com:repo.git")).toBe("repo");
    expect(suggestFolderName("ssh://git@host:2222/owner/repo.git")).toBe("repo");
  });

  it("derives the name from local paths", () => {
    expect(suggestFolderName("/Users/x/projects/my-app")).toBe("my-app");
    expect(suggestFolderName("C:\\code\\repo")).toBe("repo");
    expect(suggestFolderName("~/foo/bar/")).toBe("bar");
  });

  it("ignores query strings and fragments", () => {
    expect(suggestFolderName("https://github.com/owner/repo.git?ref=main")).toBe("repo");
    expect(suggestFolderName("https://github.com/owner/repo#readme")).toBe("repo");
  });

  it("returns null for empty or unusable input", () => {
    expect(suggestFolderName("")).toBeNull();
    expect(suggestFolderName("   ")).toBeNull();
    expect(suggestFolderName("/")).toBeNull();
    expect(suggestFolderName("https://github.com/owner/.git")).toBeNull();
  });
});

describe("isSafeFolderName", () => {
  it("accepts normal names", () => {
    expect(isSafeFolderName("my-app")).toBe(true);
    expect(isSafeFolderName("repo_2")).toBe(true);
  });

  it("rejects unsafe names", () => {
    expect(isSafeFolderName("")).toBe(false);
    expect(isSafeFolderName("..")).toBe(false);
    expect(isSafeFolderName("a/b")).toBe(false);
    expect(isSafeFolderName("a\\b")).toBe(false);
    expect(isSafeFolderName("bad:name")).toBe(false);
    expect(isSafeFolderName("trailing.")).toBe(false);
    expect(isSafeFolderName(".git")).toBe(false);
  });
});

describe("progressMessage", () => {
  it("strips carriage returns and collapses padding", () => {
    expect(progressMessage("Receiving objects:  45% (45/100)\r")).toBe(
      "Receiving objects: 45% (45/100)"
    );
  });

  it("returns null for blank lines", () => {
    expect(progressMessage("\r\n")).toBeNull();
    expect(progressMessage("   ")).toBeNull();
  });

  it("keeps ordinary lines intact", () => {
    expect(progressMessage("Cloning into 'repo'...")).toBe("Cloning into 'repo'...");
  });
});

describe("validateDestination", () => {
  it("allows a new or empty destination", () => {
    expect(validateDestination(dest())).toBeNull();
    expect(validateDestination(dest({ exists: true, isDir: true, isEmpty: true }))).toBeNull();
  });

  it("rejects a non-empty existing folder", () => {
    expect(
      validateDestination(dest({ exists: true, isDir: true, isEmpty: false }))
    ).toMatch(/not empty/);
  });

  it("rejects a file at the destination", () => {
    expect(validateDestination(dest({ exists: true, isDir: false }))).toMatch(/file already exists/);
  });

  it("rejects a destination inside an existing repo", () => {
    expect(validateDestination(dest({ insideRepo: true }))).toMatch(/inside an existing Git repository/);
  });
});
