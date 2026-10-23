// Pure helpers for the clone-repository flow. The Rust backend is authoritative
// (see src-tauri/src/git/clone.rs); these mirror its rules so the dialog can
// preview a folder name, normalize progress lines, and refuse obvious mistakes
// before the request is sent.

export interface CloneDestinationState {
  exists: boolean;
  isDir: boolean;
  isEmpty: boolean;
  insideRepo: boolean;
}

/** A folder name that is safe to create on disk on every platform. */
export function isSafeFolderName(name: string): boolean {
  const trimmed = name.trim();
  if (!trimmed || trimmed === "." || trimmed === "..") return false;
  // Never create a repo folder that shadows git's own metadata directory.
  if (trimmed.toLowerCase() === ".git") return false;

  for (const ch of trimmed) {
    const code = ch.charCodeAt(0);
    if (ch === "/" || ch === "\\" || ch === "\0") return false;
    if (code < 0x20 || code === 0x7f) return false; // control characters
    if ('<>:"|?*'.includes(ch)) return false;
  }

  const last = trimmed[trimmed.length - 1];
  if (last === "." || last === " ") return false;
  return true;
}

/**
 * Derive a sensible local folder name from a remote URL (`https`, `ssh`,
 * scp-style `git@host:owner/repo`) or a local path.
 */
export function suggestFolderName(input: string): string | null {
  const trimmed = input.trim();
  if (!trimmed) return null;

  // Drop any query string or fragment (e.g. `?ref=main`).
  const withoutQuery = trimmed.split(/[?#]/)[0];

  // Drop trailing path separators.
  const withoutTrailing = withoutQuery.replace(/[\\/]+$/, "");
  if (!withoutTrailing) return null;

  // Last path segment, then the part after the final `:` (scp-style URLs).
  let segment = withoutTrailing.split(/[\\/]/).pop() ?? withoutTrailing;
  const colon = segment.lastIndexOf(":");
  if (colon !== -1) segment = segment.slice(colon + 1);
  segment = segment.trim();

  // Strip a trailing `.git` (case-insensitive).
  const stem =
    segment.toLowerCase().endsWith(".git") && segment.length > 4
      ? segment.slice(0, -4)
      : segment;
  const name = stem.replace(/[\\/]+$/, "").trim();

  return isSafeFolderName(name) ? name : null;
}

/**
 * Normalize a raw stderr chunk emitted by `git clone --progress` into a single
 * displayable line, or `null` when there is nothing worth showing.
 */
export function progressMessage(line: string): string | null {
  const cleaned = line.replace(/[\r\0]/g, "").trim();
  if (!cleaned) return null;
  return cleaned.replace(/ {2,}/g, " ");
}

/** Mirror of the backend destination guards. Returns an error message or null. */
export function validateDestination(state: CloneDestinationState): string | null {
  if (state.insideRepo) {
    return "That destination is inside an existing Git repository. Choose a folder outside any repo.";
  }
  if (state.exists) {
    if (!state.isDir) {
      return "A file already exists at that destination. Choose a different folder name or location.";
    }
    if (!state.isEmpty) {
      return "The destination folder already exists and is not empty. Choose a different folder name or location.";
    }
  }
  return null;
}
