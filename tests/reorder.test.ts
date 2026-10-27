import { describe, it, expect } from "vitest";
import { moveItem, findDropTarget } from "../src/lib/reorder";

const list = [{ id: "a" }, { id: "b" }, { id: "c" }, { id: "d" }];
const ids = (items: { id: string }[]) => items.map((item) => item.id);

describe("moveItem", () => {
  it("moves an item down with an after drop", () => {
    expect(ids(moveItem(list, "a", "c", "after")!)).toEqual(["b", "c", "a", "d"]);
  });

  it("moves an item up with a before drop", () => {
    expect(ids(moveItem(list, "d", "b", "before")!)).toEqual(["a", "d", "b", "c"]);
  });

  it("moves an item to the start", () => {
    expect(ids(moveItem(list, "c", "a", "before")!)).toEqual(["c", "a", "b", "d"]);
  });

  it("moves an item to the end", () => {
    expect(ids(moveItem(list, "a", "d", "after")!)).toEqual(["b", "c", "d", "a"]);
  });

  it("treats a drop on itself as a no-op", () => {
    expect(moveItem(list, "b", "b", "before")).toBeNull();
  });

  it("returns null for unknown ids", () => {
    expect(moveItem(list, "missing", "b", "before")).toBeNull();
    expect(moveItem(list, "b", "missing", "after")).toBeNull();
  });

  it("does not mutate the input list", () => {
    const input = [...list];
    moveItem(input, "a", "d", "after");
    expect(ids(input)).toEqual(["a", "b", "c", "d"]);
  });
});

describe("findDropTarget", () => {
  const rows = [
    { id: "a", top: 0, height: 52 },
    { id: "b", top: 52, height: 52 },
    { id: "c", top: 104, height: 52 },
  ];

  it("targets the row above the pointer midpoint with before", () => {
    expect(findDropTarget(rows, 60, "c")).toEqual({ id: "b", position: "before" });
  });

  it("resolves the insertion point below a row's midpoint", () => {
    // Hovering the lower half of "b" inserts between "b" and "c".
    expect(findDropTarget(rows, 90, "a")).toEqual({ id: "c", position: "before" });
  });

  it("maps empty space above the list to the first row", () => {
    expect(findDropTarget(rows, -20, "c")).toEqual({ id: "a", position: "before" });
  });

  it("maps empty space below the list to after the last row", () => {
    expect(findDropTarget(rows, 400, "a")).toEqual({ id: "c", position: "after" });
  });

  it("returns null when the pointer is over the dragged row", () => {
    expect(findDropTarget(rows, 10, "a")).toBeNull();
  });

  it("returns null when the dragged row is already at the end", () => {
    expect(findDropTarget(rows, 400, "c")).toBeNull();
  });

  it("returns null for an empty list", () => {
    expect(findDropTarget([], 10, "a")).toBeNull();
  });
});
