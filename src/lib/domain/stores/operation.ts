import { writable } from "svelte/store";

export interface OperationState {
  status: "idle" | "running" | "success" | "error";
  message: string;
  repoId?: string | null;
  branch?: string | null;
  errorDetails?: string | null;
  startedAt?: number;
}

const initial: OperationState = {
  status: "idle",
  message: "Ready",
  repoId: null,
  branch: null,
  errorDetails: null,
};

export const operationStatus = writable<OperationState>(initial);

let autoResetTimer: any = null;

export function setOperationRunning(message: string, repoId?: string | null, branch?: string | null) {
  if (autoResetTimer) clearTimeout(autoResetTimer);
  operationStatus.set({
    status: "running",
    message,
    repoId: repoId || null,
    branch: branch || null,
    errorDetails: null,
    startedAt: Date.now(),
  });
}

export function setOperationSuccess(message: string) {
  if (autoResetTimer) clearTimeout(autoResetTimer);
  operationStatus.update((s) => ({
    ...s,
    status: "success",
    message,
    errorDetails: null,
  }));
  autoResetTimer = setTimeout(() => {
    operationStatus.set(initial);
  }, 4000);
}

export function setOperationError(message: string, errorDetails?: string | null) {
  if (autoResetTimer) clearTimeout(autoResetTimer);
  operationStatus.update((s) => ({
    ...s,
    status: "error",
    message,
    errorDetails: errorDetails || null,
  }));
}

export function clearOperation() {
  if (autoResetTimer) clearTimeout(autoResetTimer);
  operationStatus.set(initial);
}
