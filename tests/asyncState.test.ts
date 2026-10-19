import { describe, it, expect, vi, afterEach } from "vitest";
import {
  classifyError,
  normalizeError,
  createDelayedToggle,
  emptyFileListMessage,
  emptyDiffMessage,
} from "../src/lib/asyncState";

describe("normalizeError", () => {
  it("reads a serialized Tauri AppError object", () => {
    const err = normalizeError({
      code: "NotARepo",
      message: "The path '/x' is not a Git repository.",
      stderr: null,
    });
    expect(err.kind).toBe("not-a-repo");
    expect(err.code).toBe("NotARepo");
    expect(err.message).toContain("not a Git repository");
    expect(err.retryable).toBe(false);
  });

  it("handles a plain string rejection", () => {
    const err = normalizeError("Git executable was not found on PATH.");
    expect(err.kind).toBe("git-missing");
    expect(err.retryable).toBe(false);
  });

  it("keeps stderr and marks recoverable failures retryable", () => {
    const err = normalizeError({ code: "Other", message: "boom", stderr: "fatal: nope" });
    expect(err.stderr).toBe("fatal: nope");
    expect(err.retryable).toBe(true);
  });

  it("falls back to a default message when the backend sends none", () => {
    const err = normalizeError({ code: "Timeout" });
    expect(err.kind).toBe("timeout");
    expect(err.message.length).toBeGreaterThan(0);
  });

  it("normalizes Error instances", () => {
    const err = normalizeError(new Error("kaboom"));
    expect(err.message).toBe("kaboom");
    expect(err.kind).toBe("unknown");
    expect(err.retryable).toBe(true);
  });
});

describe("classifyError", () => {
  it("classifies a missing repository path", () => {
    expect(classifyError("", "Repository path does not exist")).toBe("path-missing");
  });

  it("classifies a locked index", () => {
    expect(classifyError("", "Unable to create '.git/index.lock': File exists")).toBe(
      "file-locked"
    );
  });

  it("classifies an auth failure from the code", () => {
    expect(classifyError("AuthFailed", "")).toBe("auth");
  });

  it("falls back to unknown", () => {
    expect(classifyError("", "weird thing happened")).toBe("unknown");
  });
});

describe("createDelayedToggle", () => {
  afterEach(() => vi.useRealTimers());

  it("does not reveal for work under the delay (avoids flicker)", () => {
    vi.useFakeTimers();
    const seen: boolean[] = [];
    const t = createDelayedToggle(150, (v) => seen.push(v));
    t.begin();
    vi.advanceTimersByTime(100);
    t.end();
    vi.advanceTimersByTime(200);
    expect(seen).toEqual([]);
  });

  it("reveals after the delay and hides when work finishes", () => {
    vi.useFakeTimers();
    const seen: boolean[] = [];
    const t = createDelayedToggle(150, (v) => seen.push(v));
    t.begin();
    vi.advanceTimersByTime(150);
    expect(seen).toEqual([true]);
    t.end();
    expect(seen).toEqual([true, false]);
  });

  it("stays visible while overlapping work is in flight", () => {
    vi.useFakeTimers();
    const seen: boolean[] = [];
    const t = createDelayedToggle(150, (v) => seen.push(v));
    t.begin();
    t.begin();
    vi.advanceTimersByTime(150);
    expect(seen).toEqual([true]);
    t.end();
    expect(seen).toEqual([true]);
    t.end();
    expect(seen).toEqual([true, false]);
  });

  it("cancel hides immediately and drops pending work", () => {
    vi.useFakeTimers();
    const seen: boolean[] = [];
    const t = createDelayedToggle(150, (v) => seen.push(v));
    t.begin();
    vi.advanceTimersByTime(150);
    t.cancel();
    expect(seen).toEqual([true, false]);
    t.end();
    vi.advanceTimersByTime(200);
    expect(seen).toEqual([true, false]);
  });
});

describe("empty states", () => {
  it("distinguishes a clean tree from a stale selection", () => {
    expect(emptyFileListMessage("working", 0).title).toBe("Working tree clean");
    expect(emptyFileListMessage("working", 3).title).toBe("No changed files");
    expect(emptyFileListMessage("range", 0).title).toBe("No changes in this range");
    expect(emptyFileListMessage("commit", 0).title).toBe("No changes in this range");
  });

  it("distinguishes a binary file from an empty diff", () => {
    expect(emptyDiffMessage(true).title).toBe("Binary file");
    expect(emptyDiffMessage(false).title).toBe("No changes in this file");
  });
});
