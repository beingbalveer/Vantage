import { describe, it, expect } from "vitest";
import { SHORTCUTS } from "../src/lib/keys";

describe("keys registry", () => {
  it("has unique shortcut IDs", () => {
    const ids = SHORTCUTS.map((s) => s.id);
    const uniqueIds = new Set(ids);
    expect(ids.length).toBe(uniqueIds.size);
  });

  it("all shortcuts have valid category and description", () => {
    for (const s of SHORTCUTS) {
      expect(s.description.length).toBeGreaterThan(0);
      expect(["Navigation", "Review", "Diff", "Panes", "Terminal"]).toContain(s.category);
    }
  });
});
