// Loading / empty / error state helpers (ALPH-150).
//
// The pane runs three async reads (history, changed-file list, file diff).
// Historically every outcome rendered the same "No changes" copy and failures
// only reached `console.error`. These helpers keep the classification of a
// failure (and the copy for a genuinely empty result) in one place so the UI
// can tell "still loading", "genuinely empty" and "failed" apart.

export type ErrorKind =
  | "git-missing"
  | "not-a-repo"
  | "path-missing"
  | "not-found"
  | "auth"
  | "timeout"
  | "conflict"
  | "dirty"
  | "file-locked"
  | "too-large"
  | "io"
  | "unknown";

export interface NormalizedError {
  kind: ErrorKind;
  code: string;
  message: string;
  stderr: string | null;
  /** Whether retrying the request could plausibly succeed. */
  retryable: boolean;
}

const NON_RETRYABLE: ReadonlySet<ErrorKind> = new Set<ErrorKind>([
  "git-missing",
  "not-a-repo",
  "path-missing",
  "not-found",
]);

const DEFAULT_MESSAGES: Record<ErrorKind, string> = {
  "git-missing": "Git was not found on PATH. Install Git, then retry.",
  "not-a-repo": "That folder isn't a Git repository.",
  "path-missing": "The repository folder no longer exists.",
  "not-found": "The repository is no longer registered.",
  auth: "Git needs credentials. Run `git fetch` in the terminal to sign in.",
  timeout: "Git took too long to respond. Try again in a moment.",
  conflict: "There is a merge conflict to resolve.",
  dirty: "Commit or discard your changes first.",
  "file-locked": "Another Git process is running. Try again in a moment.",
  "too-large": "This file is too large to show a diff.",
  io: "A file-system error occurred.",
  unknown: "Something went wrong.",
};

/** Map the backend error code (or a status message) onto a UI category. */
export function classifyError(code: string, message: string): ErrorKind {
  const c = (code || "").toLowerCase();
  const m = (message || "").toLowerCase();

  if (
    c === "gitmissing" ||
    m.includes("git executable was not found") ||
    m.includes("git was not found")
  ) {
    return "git-missing";
  }
  if (c === "notarepo" || m.includes("not a git repository")) return "not-a-repo";
  if (c === "pathmissing" || m.includes("does not exist")) return "path-missing";
  if (c === "notfound") return "not-found";
  if (c === "authfailed") return "auth";
  if (c === "timeout") return "timeout";
  if (c === "conflict") return "conflict";
  if (c === "dirty") return "dirty";
  if (m.includes("index.lock") || m.includes("another git process")) return "file-locked";
  if (c === "toolarge" || m.includes("too large")) return "too-large";
  if (c === "io" || c === "serialization") return "io";
  return "unknown";
}

/**
 * Tauri rejects `invoke` with the serialized `AppError`, but some paths throw a
 * plain string or an `Error`. Normalize all of them into one renderable shape.
 */
export function normalizeError(err: unknown): NormalizedError {
  let code = "Unknown";
  let rawMessage = "";
  let stderr: string | null = null;

  if (typeof err === "string") {
    rawMessage = err;
  } else if (err instanceof Error) {
    rawMessage = err.message;
    const extra = err as Error & { code?: unknown; stderr?: unknown };
    if (typeof extra.code === "string") code = extra.code;
    if (typeof extra.stderr === "string") stderr = extra.stderr;
  } else if (err && typeof err === "object") {
    const e = err as Record<string, unknown>;
    if (typeof e.code === "string") code = e.code;
    if (typeof e.message === "string") rawMessage = e.message;
    if (typeof e.stderr === "string") stderr = e.stderr;
  }

  const kind = classifyError(code, rawMessage);
  const message = rawMessage.trim() || DEFAULT_MESSAGES[kind];

  return {
    kind,
    code,
    message,
    stderr: stderr && stderr.trim() ? stderr : null,
    retryable: !NON_RETRYABLE.has(kind),
  };
}

export interface DelayedToggle {
  /** A request started. */
  begin(): void;
  /** A request finished (success or failure). */
  end(): void;
  /** Drop every pending request, e.g. on repository switch. */
  cancel(): void;
}

/**
 * Reveal a pending treatment only once work has taken longer than `delayMs`,
 * and keep it up while any tracked request is still in flight. This is the
 * DESIGN §7.7 rule ("show only if loading exceeds 150ms") with overlap safety:
 * a superseded request finishing early can never hide the current request's
 * pending state.
 */
export function createDelayedToggle(
  delayMs: number,
  onChange: (visible: boolean) => void
): DelayedToggle {
  let count = 0;
  let visible = false;
  let timer: ReturnType<typeof setTimeout> | null = null;

  function clearTimer() {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
  }
  function show() {
    if (!visible && count > 0) {
      visible = true;
      onChange(true);
    }
  }
  function hide() {
    if (visible) {
      visible = false;
      onChange(false);
    }
  }

  return {
    begin() {
      count += 1;
      clearTimer();
      timer = setTimeout(() => {
        timer = null;
        show();
      }, delayMs);
    },
    end() {
      if (count > 0) count -= 1;
      if (count === 0) {
        clearTimer();
        hide();
      }
    },
    cancel() {
      count = 0;
      clearTimer();
      hide();
    },
  };
}

export type ScopeKind = "commit" | "range" | "working";

/** Copy for a successfully-loaded but empty changed-file list. */
export function emptyFileListMessage(
  scope: ScopeKind,
  dirtyCount: number
): { title: string; description: string } {
  if (scope === "working") {
    return dirtyCount > 0
      ? {
          title: "No changed files",
          description: "Nothing to show for the current selection.",
        }
      : {
          title: "Working tree clean",
          description: "No uncommitted changes in this repository.",
        };
  }
  return {
    title: "No changes in this range",
    description: "This selection doesn't touch any files.",
  };
}

/** Copy for a successfully-loaded diff that has no hunks to render. */
export function emptyDiffMessage(binary: boolean): { title: string; description: string } {
  return binary
    ? {
        title: "Binary file",
        description: "A text diff isn't available for this file.",
      }
    : {
        title: "No changes in this file",
        description: "This file is identical in both revisions.",
      };
}
