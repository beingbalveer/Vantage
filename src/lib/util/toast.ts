import { writable } from "svelte/store";

/**
 * Toast notifications are a leaf service: domain code needs to report failures
 * without depending on the app shell. UI-only state lives in `app/ui.ts`.
 */
export interface ToastMessage {
  id: string;
  message: string;
  kind?: "info" | "error" | "success";
  actionLabel?: string;
  action?: () => void;
  /** Optional secondary link rendered next to the action (e.g. "Recovery"). */
  linkLabel?: string;
  linkAction?: () => void;
  details?: string;
}

export const toasts = writable<ToastMessage[]>([]);

const toastTimers = new Map<string, { timeoutId: any; remaining: number; startedAt: number }>();

export function addToast(toast: Omit<ToastMessage, "id">) {
  const id = "toast-" + Math.random().toString(36).substring(2, 9);
  const newToast: ToastMessage = { ...toast, id };

  toasts.update((list) => [...list.slice(-2), newToast]); // max 3 stacked

  const duration = toast.actionLabel || toast.linkLabel || toast.details ? 8000 : 5000;
  const timerData = {
    timeoutId: setTimeout(() => {
      removeToast(id);
    }, duration),
    remaining: duration,
    startedAt: Date.now(),
  };
  toastTimers.set(id, timerData);

  return id;
}

export function pauseToastTimer(id: string) {
  const t = toastTimers.get(id);
  if (t) {
    clearTimeout(t.timeoutId);
    const elapsed = Date.now() - t.startedAt;
    t.remaining = Math.max(1500, t.remaining - elapsed);
  }
}

export function resumeToastTimer(id: string) {
  const t = toastTimers.get(id);
  if (t) {
    t.startedAt = Date.now();
    t.timeoutId = setTimeout(() => {
      removeToast(id);
    }, t.remaining);
  }
}

export function removeToast(id: string) {
  const t = toastTimers.get(id);
  if (t) {
    clearTimeout(t.timeoutId);
    toastTimers.delete(id);
  }
  toasts.update((list) => list.filter((item) => item.id !== id));
}
