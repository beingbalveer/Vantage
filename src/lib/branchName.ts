// Client-side branch name validation.
//
// This mirrors the rules `git check-ref-format --branch` enforces so the UI can
// disable the primary action and explain the problem instantly. The backend is
// still the authority: it re-validates every name with git itself and surfaces
// git's real error text, so a name that slips through here is still rejected.

// ASCII control characters, space, DEL and the characters git forbids in a ref.
const INVALID_CHARS = /[\x00-\x20\x7f~^:?*\[\\]/;

/**
 * Returns a human-readable reason when `name` is not a valid branch name, or
 * `null` when it looks valid.
 */
export function validateBranchName(name: string): string | null {
  const n = (name ?? "").trim();

  if (!n) return "Branch name cannot be empty.";
  if (n === "@") return "'@' is not a valid branch name.";
  if (n.startsWith("-")) return "Branch name cannot start with '-'.";
  if (n.startsWith("/") || n.endsWith("/")) {
    return "Branch name cannot start or end with '/'.";
  }
  if (n.endsWith(".")) return "Branch name cannot end with '.'.";
  if (n.endsWith(".lock")) return "Branch name cannot end with '.lock'.";
  if (n.includes("..")) return "Branch name cannot contain '..'.";
  if (n.includes("@{")) return "Branch name cannot contain '@{'.";
  if (n.includes("//")) return "Branch name cannot contain '//'.";
  if (INVALID_CHARS.test(n)) {
    return "Branch name cannot contain spaces or any of ~ ^ : ? * [ \\.";
  }

  for (const component of n.split("/")) {
    if (!component) return "Branch name cannot contain an empty path segment.";
    if (component.startsWith(".")) {
      return "Branch name segments cannot start with '.'.";
    }
    if (component.endsWith(".lock")) {
      return "Branch name segments cannot end with '.lock'.";
    }
  }

  return null;
}

/** True when the name can safely be sent to the backend. */
export function isValidBranchName(name: string): boolean {
  return validateBranchName(name) === null;
}

/**
 * A short label for a start point, used in dialog copy. "HEAD" is shown as the
 * current branch so the dialog reads naturally.
 */
export function describeStartPoint(startPoint: string, currentBranch?: string | null): string {
  const s = (startPoint ?? "").trim();
  if (!s || s === "HEAD") {
    return currentBranch ? `HEAD (${currentBranch})` : "HEAD";
  }
  return s;
}
