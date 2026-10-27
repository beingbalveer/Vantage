import { describe, it, expect } from "vitest";
import { clampMenuPosition } from "../src/lib/app/menu";

describe("clampMenuPosition", () => {
  it("keeps a menu fully inside the viewport", () => {
    expect(clampMenuPosition(10, 20, 200, 120, 1000, 800)).toEqual({ left: 10, top: 20 });
  });

  it("pulls a menu back from the right/bottom edges", () => {
    expect(clampMenuPosition(950, 780, 200, 120, 1000, 800)).toEqual({ left: 792, top: 672 });
  });

  it("never returns a negative offset", () => {
    expect(clampMenuPosition(-40, -50, 200, 120, 1000, 800)).toEqual({ left: 8, top: 8 });
  });

  it("pins oversized menus to the top-left padding", () => {
    expect(clampMenuPosition(100, 100, 2000, 2000, 1000, 800)).toEqual({ left: 8, top: 8 });
  });
});
