import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  createHorizontalResizerKeyboard,
  createVerticalBottomResizer,
  createVerticalBottomResizerKeyboard,
} from "../src/lib/resizer";

function fakeStorage() {
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", {
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
  } as unknown as Storage);
  return store;
}

function key(k: string, shiftKey = false) {
  return { key: k, shiftKey, preventDefault: vi.fn() } as unknown as KeyboardEvent;
}

beforeEach(() => {
  vi.unstubAllGlobals();
});

describe("createHorizontalResizerKeyboard", () => {
  it("nudges right by 12px and stores the width", () => {
    const store = fakeStorage();
    let width = 240;
    const handler = createHorizontalResizerKeyboard({
      getWidth: () => width,
      setWidth: (w) => (width = w),
      minWidth: 160,
      maxWidth: 600,
      storageKey: "relay_files_col_width",
    });
    handler(key("ArrowRight"));
    expect(width).toBe(252);
    expect(store.get("relay_files_col_width")).toBe("252");
  });

  it("nudges left and clamps at the minimum", () => {
    fakeStorage();
    let width = 165;
    const handler = createHorizontalResizerKeyboard({
      getWidth: () => width,
      setWidth: (w) => (width = w),
      minWidth: 160,
      maxWidth: 600,
    });
    handler(key("ArrowLeft"));
    expect(width).toBe(160);
    handler(key("ArrowLeft"));
    expect(width).toBe(160);
  });

  it("uses a 40px step with Shift and clamps at the maximum", () => {
    fakeStorage();
    let width = 590;
    const handler = createHorizontalResizerKeyboard({
      getWidth: () => width,
      setWidth: (w) => (width = w),
      minWidth: 160,
      maxWidth: 600,
    });
    handler(key("ArrowRight", true));
    expect(width).toBe(600);
  });

  it("ignores unrelated keys", () => {
    fakeStorage();
    let width = 240;
    const handler = createHorizontalResizerKeyboard({
      getWidth: () => width,
      setWidth: (w) => (width = w),
      minWidth: 160,
      maxWidth: 600,
    });
    handler(key("ArrowUp"));
    handler(key("Enter"));
    expect(width).toBe(240);
  });

  it("tolerates storage failures without throwing", () => {
    vi.stubGlobal("localStorage", {
      getItem: () => null,
      setItem: () => {
        throw new Error("blocked");
      },
      removeItem: () => {},
      clear: () => {},
      key: () => null,
      length: 0,
    } as unknown as Storage);
    let width = 300;
    const handler = createHorizontalResizerKeyboard({
      getWidth: () => width,
      setWidth: (w) => (width = w),
      minWidth: 160,
      maxWidth: 600,
      storageKey: "relay_sidebar_width",
    });
    expect(() => handler(key("ArrowRight"))).not.toThrow();
    expect(width).toBe(312);
  });
});

describe("createVerticalBottomResizerKeyboard", () => {
  it("nudges up to increase bottom drawer height", () => {
    fakeStorage();
    let height = 240;
    const handler = createVerticalBottomResizerKeyboard({
      getHeight: () => height,
      setHeight: (h) => (height = h),
      minHeight: 80,
      maxHeight: 800,
      storageKey: "vantage_terminal_height",
    });
    handler(key("ArrowUp"));
    expect(height).toBe(256);
  });

  it("nudges down and clamps at minimum height", () => {
    fakeStorage();
    let height = 90;
    const handler = createVerticalBottomResizerKeyboard({
      getHeight: () => height,
      setHeight: (h) => (height = h),
      minHeight: 80,
      maxHeight: 800,
    });
    handler(key("ArrowDown"));
    expect(height).toBe(80);
    handler(key("ArrowDown"));
    expect(height).toBe(80);
  });
});
