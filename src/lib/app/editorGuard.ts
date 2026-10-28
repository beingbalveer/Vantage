// Unsaved-edit guard shared by the code viewer and the navigation surfaces.
//
// The viewer owns the draft and the confirm dialog; navigation (file tabs,
// pane view switch, repo switch) asks this module whether it may leave.
// Guards are keyed by pane so a multi-pane layout cannot answer for the wrong
// editor.

export interface EditorGuard {
  path: string;
  dirty: boolean;
  /** Resolves true when the viewer may be replaced (saved or discarded). */
  confirmLeave: () => Promise<boolean>;
}

const guards = new Map<number, EditorGuard>();

export function setEditorGuard(paneIndex: number, guard: EditorGuard | null) {
  if (guard) guards.set(paneIndex, guard);
  else guards.delete(paneIndex);
}

export function hasDirtyEditor(paneIndex: number): boolean {
  return guards.get(paneIndex)?.dirty ?? false;
}

/** Resolve before replacing the pane's content; false means "stay put". */
export async function confirmLeaveEditor(paneIndex: number): Promise<boolean> {
  const guard = guards.get(paneIndex);
  return guard ? guard.confirmLeave() : true;
}
