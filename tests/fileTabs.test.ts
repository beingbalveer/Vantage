import { describe, expect, it } from "vitest";
import { get } from "svelte/store";
import {
  activateFileTab,
  closeFileTab,
  closeFileTabs,
  fileTabs,
  getFileTabState,
  openFileTab,
  pruneFileTabs,
} from "../src/lib/domain/stores/fileTabs";

function reset() {
  fileTabs.set({});
}

describe("file tabs", () => {
  it("opens files as tabs and focuses the newest", () => {
    reset();
    openFileTab("r1", "src/a.ts");
    openFileTab("r1", "src/b.ts");

    const state = getFileTabState(get(fileTabs), "r1");
    expect(state.tabs).toEqual(["src/a.ts", "src/b.ts"]);
    expect(state.active).toBe("src/b.ts");
  });

  it("focuses an already open file without duplicating it", () => {
    reset();
    openFileTab("r1", "a");
    openFileTab("r1", "b");
    openFileTab("r1", "a");

    const state = getFileTabState(get(fileTabs), "r1");
    expect(state.tabs).toEqual(["a", "b"]);
    expect(state.active).toBe("a");
  });

  it("activates a tab without reordering", () => {
    reset();
    openFileTab("r1", "a");
    openFileTab("r1", "b");
    activateFileTab("r1", "a");

    const state = getFileTabState(get(fileTabs), "r1");
    expect(state.tabs).toEqual(["a", "b"]);
    expect(state.active).toBe("a");
  });

  it("focuses the neighbour when the active tab closes", () => {
    reset();
    openFileTab("r1", "a");
    openFileTab("r1", "b");
    openFileTab("r1", "c");
    activateFileTab("r1", "b");
    closeFileTab("r1", "b");

    const state = getFileTabState(get(fileTabs), "r1");
    expect(state.tabs).toEqual(["a", "c"]);
    expect(state.active).toBe("c");
  });

  it("falls back to the last tab when closing the final one", () => {
    reset();
    openFileTab("r1", "a");
    openFileTab("r1", "b");
    closeFileTab("r1", "b");

    const state = getFileTabState(get(fileTabs), "r1");
    expect(state.tabs).toEqual(["a"]);
    expect(state.active).toBe("a");

    closeFileTab("r1", "a");
    const empty = getFileTabState(get(fileTabs), "r1");
    expect(empty.tabs).toEqual([]);
    expect(empty.active).toBeNull();
  });

  it("keeps repositories independent", () => {
    reset();
    openFileTab("r1", "a");
    openFileTab("r2", "b");

    expect(getFileTabState(get(fileTabs), "r1").tabs).toEqual(["a"]);
    expect(getFileTabState(get(fileTabs), "r2").tabs).toEqual(["b"]);
  });

  it("remembers that a repo has had tabs, even after closing them all", () => {
    reset();
    expect(getFileTabState(get(fileTabs), "r1").openedOnce).toBeFalsy();

    openFileTab("r1", "a");
    expect(getFileTabState(get(fileTabs), "r1").openedOnce).toBe(true);

    closeFileTab("r1", "a");
    const state = getFileTabState(get(fileTabs), "r1");
    expect(state.tabs).toEqual([]);
    expect(state.openedOnce).toBe(true);
  });

  it("prunes tabs for files that disappeared", () => {
    reset();
    openFileTab("r1", "a");
    openFileTab("r1", "b");
    pruneFileTabs("r1", ["b"]);

    const state = getFileTabState(get(fileTabs), "r1");
    expect(state.tabs).toEqual(["b"]);
    expect(state.active).toBe("b");
  });

  it("closes several tabs and keeps a surviving active tab", () => {
    reset();
    openFileTab("r1", "a");
    openFileTab("r1", "b");
    openFileTab("r1", "c");
    activateFileTab("r1", "a");

    closeFileTabs("r1", ["b", "c"]);

    const state = getFileTabState(get(fileTabs), "r1");
    expect(state.tabs).toEqual(["a"]);
    expect(state.active).toBe("a");
  });

  it("focuses the nearest surviving tab when the active tab is bulk-closed", () => {
    reset();
    openFileTab("r1", "a");
    openFileTab("r1", "b");
    openFileTab("r1", "c");
    openFileTab("r1", "d");
    activateFileTab("r1", "c");

    // Closing "to the right" keeps the clicked tab active.
    closeFileTabs("r1", ["d"]);
    expect(getFileTabState(get(fileTabs), "r1").active).toBe("c");

    // Closing the active tab plus its right neighbour falls back left.
    closeFileTabs("r1", ["c", "d"]);
    expect(getFileTabState(get(fileTabs), "r1").active).toBe("b");
  });

  it("closes all tabs without forgetting the strip was used", () => {
    reset();
    openFileTab("r1", "a");
    openFileTab("r1", "b");

    closeFileTabs("r1", ["a", "b"]);

    const state = getFileTabState(get(fileTabs), "r1");
    expect(state.tabs).toEqual([]);
    expect(state.active).toBeNull();
    expect(state.openedOnce).toBe(true);
  });

  it("ignores bulk closes for unknown repos or paths", () => {
    reset();
    openFileTab("r1", "a");

    closeFileTabs("r2", ["a"]);
    closeFileTabs("r1", ["missing"]);

    const state = getFileTabState(get(fileTabs), "r1");
    expect(state.tabs).toEqual(["a"]);
    expect(state.active).toBe("a");
  });
});
