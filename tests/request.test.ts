import { describe, it, expect } from "vitest";
import { RequestTracker, isAbortError } from "../src/lib/platform/request";

describe("RequestTracker", () => {
  it("keeps only the newest request current", () => {
    const tracker = new RequestTracker();
    const first = tracker.begin();
    const second = tracker.begin();

    expect(first.isCurrent()).toBe(false);
    expect(second.isCurrent()).toBe(true);
    expect(first.signal.aborted).toBe(true);
    expect(second.signal.aborted).toBe(false);
  });

  it("aborts on cancel and invalidates the current generation", () => {
    const tracker = new RequestTracker();
    const handle = tracker.begin();

    tracker.cancel();

    expect(handle.isCurrent()).toBe(false);
    expect(handle.signal.aborted).toBe(true);
  });

  it("never revives handles after dispose", () => {
    const tracker = new RequestTracker();
    tracker.dispose();
    const handle = tracker.begin();

    expect(handle.isCurrent()).toBe(false);
    expect(handle.signal.aborted).toBe(true);
  });

  it("ignores stale handles after a newer begin", () => {
    const tracker = new RequestTracker();
    const first = tracker.begin();
    const second = tracker.begin();

    expect(first.isCurrent()).toBe(false);
    expect(second.isCurrent()).toBe(true);
  });
});

describe("isAbortError", () => {
  it("detects AbortError-shaped objects", () => {
    expect(isAbortError({ name: "AbortError" })).toBe(true);
    expect(isAbortError(new Error("boom"))).toBe(false);
    expect(isAbortError(null)).toBe(false);
    expect(isAbortError("AbortError")).toBe(false);
  });
});
