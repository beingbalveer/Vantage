/**
 * Latest-wins request tracking for view components.
 *
 * Tauri commands cannot be aborted once sent, so cancellation is cooperative:
 * the tracker invalidates stale generations and exposes an `AbortSignal` for
 * callers that can abort (fetch, timers). Always check `isCurrent()` before
 * writing the result into component state.
 */
export interface RequestHandle {
  readonly signal: AbortSignal;
  /** True while this handle is still the newest request for the tracker. */
  isCurrent(): boolean;
}

export class RequestTracker {
  private controller: AbortController | null = null;
  private generation = 0;
  private disposed = false;

  begin(): RequestHandle {
    if (this.disposed) {
      const controller = new AbortController();
      controller.abort("disposed");
      return { signal: controller.signal, isCurrent: () => false };
    }
    this.controller?.abort("superseded");
    this.controller = new AbortController();
    const generation = ++this.generation;
    const signal = this.controller.signal;
    return {
      signal,
      isCurrent: () =>
        !this.disposed && generation === this.generation && !signal.aborted,
    };
  }

  /** Abort the in-flight request and invalidate its generation. */
  cancel(reason: string = "cancelled"): void {
    this.generation++;
    this.controller?.abort(reason);
    this.controller = null;
  }

  /** Cancel for good; further `begin()` handles are never current. */
  dispose(): void {
    this.disposed = true;
    this.cancel("disposed");
  }
}

/** True when an error came from an aborted request rather than a real failure. */
export function isAbortError(error: unknown): boolean {
  return (
    typeof error === "object" &&
    error !== null &&
    "name" in error &&
    (error as { name?: unknown }).name === "AbortError"
  );
}
