// Shared types + geometry helpers for context menus (DESIGN.md §6.3).

export interface MenuItem {
  id: string;
  label: string;
  danger?: boolean;
  disabled?: boolean;
  shortcut?: string;
  onselect: () => void;
}

export interface MenuPoint {
  left: number;
  top: number;
}

/**
 * Clamp a menu's top-left corner so the whole menu stays inside the viewport,
 * keeping at least `pad` px from every edge. If the menu is larger than the
 * viewport it is pinned to the top/left edge.
 */
export function clampMenuPosition(
  x: number,
  y: number,
  width: number,
  height: number,
  viewportWidth: number,
  viewportHeight: number,
  pad = 8
): MenuPoint {
  return {
    left: Math.max(pad, Math.min(x, viewportWidth - width - pad)),
    top: Math.max(pad, Math.min(y, viewportHeight - height - pad)),
  };
}
