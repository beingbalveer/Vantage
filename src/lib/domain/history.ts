// Pure helpers for the history list paging/filter UI (F3). Kept side-effect free
// so the count label logic is unit tested without a DOM.

/** Commits fetched per page; matches the backend's first-page budget. */
export const HISTORY_PAGE_SIZE = 200;

export interface HistoryMeta {
  /** Total commits (unfiltered) or matching commits (filtered). */
  total: number;
  /** True when a filtered search stopped at the backend result cap. */
  capped: boolean;
  /** Number of commits currently loaded in the list. */
  loaded: number;
}

/**
 * Footer label for the commits column. Never claims a hard "200+" cap: it
 * reports how much is loaded of the real total, or the match count.
 */
export function formatHistoryFooter(meta: HistoryMeta, filterActive: boolean): string {
  const { total, capped, loaded } = meta;

  if (filterActive) {
    if (total === 0) return "";
    const noun = total === 1 && !capped ? "match" : "matches";
    return `${total}${capped ? "+" : ""} ${noun}`;
  }

  if (loaded === 0) return "";
  if (total > 0 && loaded < total) {
    return `Showing ${loaded} of ${total} commits`;
  }
  return `${loaded} ${loaded === 1 ? "commit" : "commits"} on this branch`;
}

/** True when another page can still be fetched. */
export function hasMoreHistory(meta: HistoryMeta): boolean {
  return meta.loaded < meta.total;
}

/** True when the event target is a text field we must not hijack. */
export function isEditableTarget(target: EventTarget | null): boolean {
  if (typeof HTMLElement === "undefined" || !(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
}
