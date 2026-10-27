import { describe, it, expect } from "vitest";
import { prefersReducedMotion, scrollBehavior } from "../src/lib/design/motion";

describe("motion helpers", () => {
  it("uses smooth scrolling when motion is allowed", () => {
    expect(scrollBehavior(false)).toBe("smooth");
  });

  it("uses instant scrolling when reduced motion is requested", () => {
    expect(scrollBehavior(true)).toBe("auto");
  });

  it("reads the reduced-motion media query", () => {
    const matcher = (query: string) => ({
      matches: query === "(prefers-reduced-motion: reduce)",
    });
    expect(prefersReducedMotion(matcher)).toBe(true);
  });

  it("treats a false media query as motion allowed", () => {
    expect(prefersReducedMotion(() => ({ matches: false }))).toBe(false);
  });

  it("defaults to motion allowed without a matcher", () => {
    expect(prefersReducedMotion(undefined)).toBe(false);
  });

  it("fails safe if matchMedia throws", () => {
    expect(
      prefersReducedMotion(() => {
        throw new Error("unavailable");
      })
    ).toBe(false);
  });
});
