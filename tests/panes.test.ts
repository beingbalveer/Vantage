import { beforeEach, describe, expect, it } from "vitest";
import { get } from "svelte/store";
import {
  paneCount,
  focusedPaneIndex,
  cyclePaneCount,
  focusPane,
  closePane,
  setPaneCount,
} from "../src/lib/domain/stores/panes";

describe("panes store", () => {
  beforeEach(() => {
    setPaneCount(1);
    focusedPaneIndex.set(0);
  });

  it("cycles pane count 1 -> 2 -> 3 -> 1", () => {
    expect(get(paneCount)).toBe(1);
    cyclePaneCount();
    expect(get(paneCount)).toBe(2);
    cyclePaneCount();
    expect(get(paneCount)).toBe(3);
    cyclePaneCount();
    expect(get(paneCount)).toBe(1);
  });

  it("clamps pane count to 1..3", () => {
    setPaneCount(9);
    expect(get(paneCount)).toBe(3);
    setPaneCount(0);
    expect(get(paneCount)).toBe(1);
  });

  it("focus reveals a pane before focusing so focus is never invisible", () => {
    setPaneCount(1);
    focusPane(2);
    expect(get(paneCount)).toBe(3);
    expect(get(focusedPaneIndex)).toBe(2);
  });

  it("close never drops below one pane and re-clamps focus", () => {
    setPaneCount(1);
    closePane(0);
    expect(get(paneCount)).toBe(1);

    setPaneCount(3);
    focusPane(2);
    closePane(2);
    expect(get(paneCount)).toBe(2);
    expect(get(focusedPaneIndex)).toBeLessThan(get(paneCount));
  });
});
