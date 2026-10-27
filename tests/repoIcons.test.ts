import { describe, it, expect, beforeEach, vi } from "vitest";

/**
 * In-memory localStorage so the persistence paths can be exercised in node.
 * This mirrors the browser storage contract used by repoIcons.ts.
 */
function installLocalStorage() {
  const store = new Map<string, string>();
  const ls = {
    getItem: (k: string) => (store.has(k) ? store.get(k)! : null),
    setItem: (k: string, v: string) => {
      store.set(k, String(v));
    },
    removeItem: (k: string) => {
      store.delete(k);
    },
    clear: () => store.clear(),
    key: (i: number) => Array.from(store.keys())[i] ?? null,
    get length() {
      return store.size;
    },
  };
  vi.stubGlobal("localStorage", ls);
  return ls;
}

beforeEach(() => {
  vi.resetModules();
  vi.unstubAllGlobals();
});

describe("repoIcons · pure helpers", () => {
  it("shortBranch keeps short names intact", async () => {
    const { shortBranch } = await import("../src/lib/domain/stores/repoIcons");
    expect(shortBranch("main")).toBe("main");
    expect(shortBranch("dev")).toBe("dev");
  });

  it("shortBranch reduces long names to a stable short label", async () => {
    const { shortBranch } = await import("../src/lib/domain/stores/repoIcons");
    const short = shortBranch("feature/very-long-branch-name");
    expect(short.length).toBeLessThanOrEqual(6);
    expect(short).toBe("very-l");
  });

  it("shortBranch uses the final path segment", async () => {
    const { shortBranch } = await import("../src/lib/domain/stores/repoIcons");
    expect(shortBranch("origin/bugfix")).toBe("bugfix");
    expect(shortBranch("release/1.0")).toBe("1.0");
  });

  it("shortBranch falls back to HEAD when missing", async () => {
    const { shortBranch } = await import("../src/lib/domain/stores/repoIcons");
    expect(shortBranch(null)).toBe("HEAD");
    expect(shortBranch(undefined)).toBe("HEAD");
    expect(shortBranch("   ")).toBe("HEAD");
  });

  it("defaultAppIcon is deterministic and independent of call order", async () => {
    const { defaultAppIcon, appIcons } = await import("../src/lib/domain/stores/repoIcons");
    const first = defaultAppIcon("repo-alpha");
    expect(defaultAppIcon("repo-alpha")).toBe(first);
    // Simulate list reordering: the id is what matters, not the position.
    defaultAppIcon("repo-beta");
    defaultAppIcon("repo-gamma");
    expect(defaultAppIcon("repo-alpha")).toBe(first);
    expect(appIcons).toContain(first);
  });

  it("defaultAppIcon produces varied icons across ids", async () => {
    const { defaultAppIcon } = await import("../src/lib/domain/stores/repoIcons");
    const icons = new Set(
      ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l"].map((id) =>
        defaultAppIcon(id)
      )
    );
    expect(icons.size).toBeGreaterThan(1);
  });
});

describe("repoIcons · validation and persistence", () => {
  it("rejects a non-image data URL", async () => {
    installLocalStorage();
    const { saveRepoIcon, repoIcons } = await import("../src/lib/domain/stores/repoIcons");
    const ok = saveRepoIcon("r1", { kind: "image", value: "data:text/html,<script>" });
    expect(ok).toBe(false);
    let state: any;
    repoIcons.subscribe((v) => (state = v))();
    expect(state.r1).toBeUndefined();
  });

  it("rejects an empty id", async () => {
    installLocalStorage();
    const { saveRepoIcon } = await import("../src/lib/domain/stores/repoIcons");
    expect(saveRepoIcon("", { kind: "app", value: "code" })).toBe(false);
  });

  it("persists valid choices and restores them after a restart", async () => {
    installLocalStorage();
    const first = await import("../src/lib/domain/stores/repoIcons");
    expect(first.saveRepoIcon("r1", { kind: "app", value: "grid" })).toBe(true);

    // Simulate a full app restart: fresh module registry, same storage.
    vi.resetModules();
    const second = await import("../src/lib/domain/stores/repoIcons");
    let restored: any;
    second.repoIcons.subscribe((v) => (restored = v))();
    expect(restored.r1).toEqual({ kind: "app", value: "grid" });
  });

  it("keeps different icons for two branch views of the same path", async () => {
    installLocalStorage();
    const { saveRepoIcon, repoIcons } = await import("../src/lib/domain/stores/repoIcons");
    saveRepoIcon("view-main", { kind: "app", value: "code" });
    saveRepoIcon("view-dev", { kind: "app", value: "terminal" });
    let state: any;
    repoIcons.subscribe((v) => (state = v))();
    expect(state["view-main"]).toEqual({ kind: "app", value: "code" });
    expect(state["view-dev"]).toEqual({ kind: "app", value: "terminal" });
  });

  it("survives corrupt stored metadata without throwing", async () => {
    const ls = installLocalStorage();
    ls.setItem("relay_repo_icons_v1", "{not valid json");
    const mod = await import("../src/lib/domain/stores/repoIcons");
    let state: any;
    mod.repoIcons.subscribe((v) => (state = v))();
    expect(state).toEqual({});
  });

  it("drops invalid entries from stored metadata", async () => {
    const ls = installLocalStorage();
    ls.setItem(
      "relay_repo_icons_v1",
      JSON.stringify({
        good: { kind: "app", value: "grid" },
        badKind: { kind: "app", value: "not-an-icon" },
        badImage: { kind: "image", value: "https://example.com/x.png" },
        notObject: 42,
      })
    );
    const mod = await import("../src/lib/domain/stores/repoIcons");
    let state: any;
    mod.repoIcons.subscribe((v) => (state = v))();
    expect(state).toEqual({ good: { kind: "app", value: "grid" } });
  });

  it("reports failure without erasing existing metadata when storage throws", async () => {
    installLocalStorage();
    const mod = await import("../src/lib/domain/stores/repoIcons");
    mod.saveRepoIcon("keep", { kind: "app", value: "code" });

    const failing = {
      getItem: () => null,
      setItem: () => {
        throw new Error("quota exceeded");
      },
      removeItem: () => {},
      clear: () => {},
      key: () => null,
      length: 0,
    } as unknown as Storage;
    vi.stubGlobal("localStorage", failing);

    expect(mod.saveRepoIcon("new", { kind: "app", value: "grid" })).toBe(false);
    let state: any;
    mod.repoIcons.subscribe((v) => (state = v))();
    // Pre-existing metadata is preserved.
    expect(state.keep).toEqual({ kind: "app", value: "code" });
  });

  it("persists tech, emoji, and letter icons and supports clearRepoIcon", async () => {
    installLocalStorage();
    const { saveRepoIcon, clearRepoIcon, repoIcons } = await import("../src/lib/domain/stores/repoIcons");
    expect(saveRepoIcon("r-rust", { kind: "tech", value: "rust" })).toBe(true);
    expect(saveRepoIcon("r-emoji", { kind: "emoji", value: "🚀" })).toBe(true);
    expect(saveRepoIcon("r-letter", { kind: "letter", value: "P", color: "#2563eb" })).toBe(true);

    let state: any;
    repoIcons.subscribe((v) => (state = v))();
    expect(state["r-rust"]).toEqual({ kind: "tech", value: "rust" });
    expect(state["r-emoji"]).toEqual({ kind: "emoji", value: "🚀" });
    expect(state["r-letter"]).toEqual({ kind: "letter", value: "P", color: "#2563eb" });

    // Clearing resets the entry
    expect(clearRepoIcon("r-rust")).toBe(true);
    repoIcons.subscribe((v) => (state = v))();
    expect(state["r-rust"]).toBeUndefined();
    expect(state["r-emoji"]).toEqual({ kind: "emoji", value: "🚀" });
  });
});

describe("repoIcons · readIconFile validation", () => {
  it("rejects non-image files", async () => {
    installLocalStorage();
    const { readIconFile } = await import("../src/lib/domain/stores/repoIcons");
    const fake = { type: "text/plain", size: 10 } as unknown as File;
    await expect(readIconFile(fake)).rejects.toThrow(/image file/i);
  });

  it("rejects oversized images before reading", async () => {
    installLocalStorage();
    const { readIconFile } = await import("../src/lib/domain/stores/repoIcons");
    const fake = { type: "image/png", size: 600 * 1024 } as unknown as File;
    await expect(readIconFile(fake)).rejects.toThrow(/too large/i);
  });
});
