/**
 * Utility for smooth, draggable column resizing with pointer capture, bounds clamping, and localStorage persistence.
 */
export function createHorizontalResizer(options: {
  getWidth: () => number;
  setWidth: (width: number) => void;
  minWidth: number;
  maxWidth: number;
  storageKey?: string;
  onResizeStart?: () => void;
  onResizeEnd?: () => void;
}) {
  return function onpointerdown(e: PointerEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();

    const handle = e.currentTarget as HTMLElement | null;
    const pointerId = e.pointerId;

    if (handle && typeof handle.setPointerCapture === "function") {
      try {
        handle.setPointerCapture(pointerId);
      } catch (err) {
        console.warn("Could not set pointer capture:", err);
      }
    }

    const startX = e.clientX;
    const rawStartWidth = options.getWidth();
    const startWidth =
      typeof rawStartWidth === "number" && !Number.isNaN(rawStartWidth) && Number.isFinite(rawStartWidth)
        ? rawStartWidth
        : options.minWidth;

    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
    document.body.classList.add("resizing-col");
    options.onResizeStart?.();

    function onpointermove(ev: PointerEvent) {
      ev.preventDefault();
      const delta = ev.clientX - startX;
      const next = Math.max(options.minWidth, Math.min(options.maxWidth, startWidth + delta));
      options.setWidth(next);
    }

    function onpointerup(ev: PointerEvent) {
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      document.body.classList.remove("resizing-col");

      if (handle && typeof handle.releasePointerCapture === "function") {
        try {
          if (handle.hasPointerCapture(pointerId)) {
            handle.releasePointerCapture(pointerId);
          }
        } catch {}
      }

      window.removeEventListener("pointermove", onpointermove);
      window.removeEventListener("pointerup", onpointerup);
      window.removeEventListener("pointercancel", onpointerup);

      const finalWidth = options.getWidth();
      if (options.storageKey && typeof finalWidth === "number" && !Number.isNaN(finalWidth) && Number.isFinite(finalWidth)) {
        try {
          localStorage.setItem(options.storageKey, finalWidth.toString());
        } catch {}
      }
      options.onResizeEnd?.();
    }

    window.addEventListener("pointermove", onpointermove);
    window.addEventListener("pointerup", onpointerup);
    window.addEventListener("pointercancel", onpointerup);
  };
}

/**
 * Keyboard nudge handler for the same dividers, so resize is reachable without
 * a pointer (RELAY-UI · UI-08). Arrow keys step by 12px, Shift+Arrow by 40px.
 */
export function createHorizontalResizerKeyboard(options: {
  getWidth: () => number;
  setWidth: (width: number) => void;
  minWidth: number;
  maxWidth: number;
  storageKey?: string;
}) {
  return function onkeydown(e: KeyboardEvent) {
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    e.preventDefault();
    const step = e.shiftKey ? 40 : 12;
    const direction = e.key === "ArrowLeft" ? -1 : 1;
    const next = Math.max(
      options.minWidth,
      Math.min(options.maxWidth, options.getWidth() + direction * step)
    );
    options.setWidth(next);
    if (options.storageKey) {
      try {
        localStorage.setItem(options.storageKey, next.toString());
      } catch {}
    }
  };
}

/**
 * Utility for vertical bottom drawer resizing (dragging upward increases height, downward decreases height).
 */
export function createVerticalBottomResizer(options: {
  getHeight: () => number;
  setHeight: (height: number) => void;
  minHeight: number;
  maxHeight: number;
  storageKey?: string;
  onResizeStart?: () => void;
  onResizeEnd?: () => void;
}) {
  return function onpointerdown(e: PointerEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();

    const handle = e.currentTarget as HTMLElement | null;
    const pointerId = e.pointerId;

    if (handle && typeof handle.setPointerCapture === "function") {
      try {
        handle.setPointerCapture(pointerId);
      } catch (err) {
        console.warn("Could not set pointer capture:", err);
      }
    }

    const startY = e.clientY;
    const rawStartHeight = options.getHeight();
    const startHeight =
      typeof rawStartHeight === "number" && !Number.isNaN(rawStartHeight) && Number.isFinite(rawStartHeight)
        ? rawStartHeight
        : options.minHeight;

    document.body.style.cursor = "row-resize";
    document.body.style.userSelect = "none";
    document.body.classList.add("resizing-row");
    options.onResizeStart?.();

    function onpointermove(ev: PointerEvent) {
      ev.preventDefault();
      // Dragging upward (ev.clientY < startY) increases height (delta > 0)
      const delta = startY - ev.clientY;
      const next = Math.max(options.minHeight, Math.min(options.maxHeight, startHeight + delta));
      options.setHeight(next);
    }

    function onpointerup(ev: PointerEvent) {
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      document.body.classList.remove("resizing-row");

      if (handle && typeof handle.releasePointerCapture === "function") {
        try {
          if (handle.hasPointerCapture(pointerId)) {
            handle.releasePointerCapture(pointerId);
          }
        } catch {}
      }

      window.removeEventListener("pointermove", onpointermove);
      window.removeEventListener("pointerup", onpointerup);
      window.removeEventListener("pointercancel", onpointerup);

      const finalHeight = options.getHeight();
      if (options.storageKey && typeof finalHeight === "number" && !Number.isNaN(finalHeight) && Number.isFinite(finalHeight)) {
        try {
          localStorage.setItem(options.storageKey, finalHeight.toString());
        } catch {}
      }
      options.onResizeEnd?.();
    }

    window.addEventListener("pointermove", onpointermove);
    window.addEventListener("pointerup", onpointerup);
    window.addEventListener("pointercancel", onpointerup);
  };
}

/**
 * Keyboard nudge handler for vertical bottom drawers (ArrowUp increases, ArrowDown decreases).
 */
export function createVerticalBottomResizerKeyboard(options: {
  getHeight: () => number;
  setHeight: (height: number) => void;
  minHeight: number;
  maxHeight: number;
  storageKey?: string;
}) {
  return function onkeydown(e: KeyboardEvent) {
    if (e.key !== "ArrowUp" && e.key !== "ArrowDown") return;
    e.preventDefault();
    const step = e.shiftKey ? 40 : 16;
    const direction = e.key === "ArrowUp" ? 1 : -1;
    const next = Math.max(
      options.minHeight,
      Math.min(options.maxHeight, options.getHeight() + direction * step)
    );
    options.setHeight(next);
    if (options.storageKey) {
      try {
        localStorage.setItem(options.storageKey, next.toString());
      } catch {}
    }
  };
}

