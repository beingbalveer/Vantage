import { describe, it, expect } from "vitest";
import { computeVirtualWindow } from "../src/lib/domain/format";

describe("computeVirtualWindow", () => {
  it("handles empty or zero items correctly", () => {
    const res = computeVirtualWindow(0, 0, 800, 24);
    expect(res).toEqual({
      startIndex: 0,
      endIndex: 0,
      offsetTop: 0,
      totalHeight: 0,
      visibleCount: 0,
    });
  });

  it("calculates initial window with overscan", () => {
    // 1000 items, each 20px high. Viewport is 400px (20 items visible). Overscan = 5.
    const res = computeVirtualWindow(1000, 0, 400, 20, 5);
    expect(res.totalHeight).toBe(20000);
    expect(res.startIndex).toBe(0); // 0 - 5 capped at 0
    expect(res.endIndex).toBe(25); // 20 + 5
    expect(res.offsetTop).toBe(0);
    expect(res.visibleCount).toBe(25);
  });

  it("calculates mid-scroll window with offsets and overscan", () => {
    // 1000 items, 20px high.
    // Scrolled to 2000px (item index 100).
    // Viewport is 400px (20 items, lines 100 to 120).
    // Overscan = 10.
    const res = computeVirtualWindow(1000, 2000, 400, 20, 10);
    expect(res.startIndex).toBe(90); // 100 - 10
    expect(res.endIndex).toBe(130); // 120 + 10
    expect(res.offsetTop).toBe(1800); // 90 * 20
    expect(res.visibleCount).toBe(40);
  });

  it("caps endIndex at totalCount when near bottom", () => {
    // 100 items, 20px high -> 2000px total height.
    // Scrolled to 1800px.
    const res = computeVirtualWindow(100, 1800, 400, 20, 10);
    expect(res.endIndex).toBe(100);
    expect(res.startIndex).toBe(80);
    expect(res.totalHeight).toBe(2000);
  });

  it("renders the last rows when scrollTop is stale after the list shrinks", () => {
    // 2000px of scroll offset, but the filtered list is only 10 rows (200px).
    const res = computeVirtualWindow(10, 2000, 400, 20, 2);
    expect(res.startIndex).toBe(7); // clamped to last item - overscan
    expect(res.endIndex).toBe(10);
    expect(res.visibleCount).toBe(3);
    expect(res.offsetTop).toBe(140);
  });
});
