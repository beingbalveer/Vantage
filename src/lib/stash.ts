// Formatting helpers for stash entries (PRODUCT_SPEC: parse machine output only,
// so the backend hands us structured entries; this module only shapes display text).

import type { StashEntry } from "./types";

/** The canonical human selector for a stash: `stash@{0}`. */
export function formatStashIndex(index: number): string {
  return `stash@{${index}}`;
}

/** `stash@{0} · On main: fix auth` — used in toasts and confirmations. */
export function formatStashLabel(entry: StashEntry): string {
  return `${formatStashIndex(entry.index)} · ${entry.subject}`;
}

/**
 * Make a raw stash subject friendlier without losing meaning.
 *
 * - `WIP on main: 7f3a2c1 add auth` -> `On main: add auth`
 * - `On feature/api: my message`     -> `On feature/api: my message`
 * - `On main: 7f3a2c1` (auto, no body) -> `On main`
 * - anything else is returned unchanged
 */
export function summarizeStashSubject(subject: string): string {
  const raw = subject.trim();
  if (!raw) return raw;

  const wip = /^WIP on ([^:]+):\s*([0-9a-fA-F]{4,40})?\s*(.*)$/.exec(raw);
  if (wip) {
    const branch = wip[1].trim();
    const rest = wip[3].trim();
    return rest ? `On ${branch}: ${rest}` : `On ${branch}`;
  }

  const on = /^On ([^:]+):\s*(.*)$/.exec(raw);
  if (on) {
    const branch = on[1].trim();
    const rest = on[2].trim();
    return rest ? `On ${branch}: ${rest}` : `On ${branch}`;
  }

  return raw;
}
