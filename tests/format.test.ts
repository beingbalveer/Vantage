import { describe, it, expect } from "vitest";
import { formatRelativeTime, hashStringToColor, formatStats, splitPath } from "../src/lib/format";

describe("format", () => {
  it("formats just now for recent times", () => {
    const now = new Date().toISOString();
    expect(formatRelativeTime(now)).toBe("just now");
  });

  it("formats minutes ago", () => {
    const date = new Date(Date.now() - 5 * 60 * 1000).toISOString();
    expect(formatRelativeTime(date)).toBe("5m ago");
  });

  it("formats hours ago", () => {
    const date = new Date(Date.now() - 3 * 3600 * 1000).toISOString();
    expect(formatRelativeTime(date)).toBe("3h ago");
  });

  it("hashes repo IDs to a stable color palette", () => {
    const color1 = hashStringToColor("repo-123");
    const color2 = hashStringToColor("repo-123");
    expect(color1).toBe(color2);
    expect(color1.startsWith("#")).toBe(true);
  });

  it("formats additions and deletions correctly", () => {
    expect(formatStats(12, 3)).toBe("+12 −3");
  });

  describe("splitPath", () => {
    it("splits deep nested paths into directory and filename", () => {
      const res = splitPath("apps/web/src/components/ChatContainer.tsx");
      expect(res.dir).toBe("apps/web/src/components");
      expect(res.name).toBe("ChatContainer.tsx");
    });

    it("handles root-level files with empty directory", () => {
      const res = splitPath("README.md");
      expect(res.dir).toBe("");
      expect(res.name).toBe("README.md");
    });

    it("normalizes Windows backslashes", () => {
      const res = splitPath("apps\\api\\src\\routers\\workspaces.py");
      expect(res.dir).toBe("apps/api/src/routers");
      expect(res.name).toBe("workspaces.py");
    });

    it("handles single-level paths", () => {
      const res = splitPath("src/main.rs");
      expect(res.dir).toBe("src");
      expect(res.name).toBe("main.rs");
    });
  });
});
