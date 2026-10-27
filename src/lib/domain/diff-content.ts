// Content classification + human formatting for the diff viewer (ALPH-158).
//
// Pure helpers so the binary/image/text decision can be unit tested away from
// the DOM. Git already flags binary blobs (`git diff --numstat` prints `-`/`-`);
// images are additionally recognised by extension so they can be previewed.

export type DiffContentKind = "text" | "binary" | "image";

const IMAGE_MIME: Record<string, string> = {
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  gif: "image/gif",
  webp: "image/webp",
  svg: "image/svg+xml",
};

/** MIME type for a previewable image path, or `null`. Mirrors the Rust helper. */
export function imageMime(path: string): string | null {
  const dot = path.lastIndexOf(".");
  if (dot < 0 || dot === path.length - 1) return null;
  const ext = path.slice(dot + 1);
  // Reject anything that is not a bare extension (e.g. "png/extra").
  if (ext.includes("/") || ext.includes("\\")) return null;
  return IMAGE_MIME[ext.toLowerCase()] ?? null;
}

export function isImagePath(path: string): boolean {
  return imageMime(path) !== null;
}

/**
 * Decide how to render a changed file. Images win by extension (an SVG is text
 * to git but should still get an image comparison); otherwise git's binary flag
 * decides. `gitBinary` comes from `FileChange.binary` / the parsed diff.
 */
export function classifyDiffContent(path: string, gitBinary: boolean): DiffContentKind {
  if (isImagePath(path)) return "image";
  return gitBinary ? "binary" : "text";
}

/** `"120 KB"`, `"1.4 MB"`, `"512 B"`; `"unknown"` when the size is unavailable. */
export function formatBytes(bytes: number | null | undefined): string {
  if (bytes == null || !Number.isFinite(bytes) || bytes < 0) return "unknown";
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  const rounded = value >= 10 ? Math.round(value) : Math.round(value * 10) / 10;
  return `${rounded} ${units[unit]}`;
}

/** `"120 KB → 134 KB"`, collapsing to one side for added/removed files. */
export function formatSizeChange(
  oldSize: number | null | undefined,
  newSize: number | null | undefined,
  oldExists: boolean,
  newExists: boolean
): string {
  if (!oldExists && newExists) return formatBytes(newSize);
  if (oldExists && !newExists) return formatBytes(oldSize);
  return `${formatBytes(oldSize)} → ${formatBytes(newSize)}`;
}

