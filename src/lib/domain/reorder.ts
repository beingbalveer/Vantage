export type DropPosition = "before" | "after";

export interface RowRect {
  id: string;
  top: number;
  height: number;
}

/**
 * Move `sourceId` next to `targetId` in a copy of `list`.
 *
 * Returns `null` when the move is a no-op or either id is unknown, so callers
 * can skip persistence work.
 */
export function moveItem<T extends { id: string }>(
  list: T[],
  sourceId: string,
  targetId: string,
  position: DropPosition
): T[] | null {
  if (sourceId === targetId) return null;

  const sourceIndex = list.findIndex((item) => item.id === sourceId);
  const targetIndex = list.findIndex((item) => item.id === targetId);
  if (sourceIndex === -1 || targetIndex === -1) return null;

  const next = [...list];
  const [moved] = next.splice(sourceIndex, 1);
  let insertIndex = next.findIndex((item) => item.id === targetId);
  if (position === "after") insertIndex += 1;
  next.splice(insertIndex, 0, moved);
  return next;
}

/**
 * Resolve which row the pointer is over and whether the dragged row should
 * land before or after it. The bottom half of the last row maps to "after the
 * last row", so dropping in the empty space below the list still works.
 */
export function findDropTarget(
  rows: RowRect[],
  pointerY: number,
  draggedId: string
): { id: string; position: DropPosition } | null {
  if (rows.length === 0) return null;

  for (const row of rows) {
    if (pointerY < row.top + row.height / 2) {
      if (row.id === draggedId) return null;
      return { id: row.id, position: "before" };
    }
  }

  const last = rows[rows.length - 1];
  if (last.id === draggedId) return null;
  return { id: last.id, position: "after" };
}
