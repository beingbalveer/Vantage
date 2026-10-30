import type { DetectedApp, Settings } from "../platform/types";

/** Shared, framework-free validation helpers for the settings dialog. */

export function validateIdentityName(value: string): string | null {
  const name = value.trim();
  if (!name) return "Name cannot be empty.";
  if (name.length > 200) return "Name is too long.";
  // eslint-disable-next-line no-control-regex
  if (/[\u0000-\u001f\u007f]/.test(name)) return "Name cannot contain control characters.";
  return null;
}

export function validateIdentityEmail(value: string): string | null {
  const email = value.trim();
  if (!email) return "Email cannot be empty.";
  if (email.length > 320 || /\s/.test(email)) {
    return "Email cannot contain spaces.";
  }
  const parts = email.split("@");
  if (parts.length !== 2) return "Enter a valid email address (name@example.com).";
  const [local, domain] = parts;
  if (!local || !domain) return "Enter a valid email address (name@example.com).";
  if (local.startsWith(".") || local.endsWith(".")) {
    return "Enter a valid email address (name@example.com).";
  }
  if (!domain.includes(".") || domain.split(".").some((part) => !part)) {
    return "Enter a valid email address (name@example.com).";
  }
  return null;
}

export function validateBranchName(value: string): string | null {
  const name = value.trim();
  if (!name) return "Branch name cannot be empty.";
  if (name === "HEAD" || name === "@") return "Branch name cannot be HEAD or @.";
  if (name.startsWith("-") || name.startsWith("/") || name.endsWith("/") || name.endsWith(".")) {
    return "Invalid branch name.";
  }
  if (name.includes("..") || name.includes("@{") || name.includes("//")) {
    return "Branch name contains an invalid sequence.";
  }
  // eslint-disable-next-line no-control-regex
  if (/[\u0000-\u001f\u007f ~^:?*[\\]/.test(name)) {
    return "Branch name contains a character Git does not allow.";
  }
  if (name.split("/").some((part) => !part || part.startsWith(".") || part.endsWith(".lock"))) {
    return "Each branch path component must be valid.";
  }
  return null;
}

/**
 * Match a persisted editor command template to a detected option. A missing
 * setting means the system default app, which is the fallback option.
 */
export function matchEditorCommand(
  command: string | null | undefined,
  editors: DetectedApp[]
): DetectedApp | null {
  if (!command || !command.trim()) {
    return editors.find((editor) => editor.id === "system") || null;
  }
  return editors.find((editor) => editor.command === command) || null;
}

/** Match a persisted shell path to a detected shell option. */
export function matchShell(
  shell: string | null | undefined,
  shells: DetectedApp[]
): DetectedApp | null {
  if (!shell || !shell.trim()) return null;
  return shells.find((option) => option.command === shell) || null;
}

/** Clamp a numeric setting value, falling back when it is not a number. */
export function clampSetting(value: number, min: number, max: number, fallback: number): number {
  if (!Number.isFinite(value)) return fallback;
  return Math.max(min, Math.min(max, Math.round(value)));
}

/** Settings as sent to the backend, with numeric values clamped. */
export function normalizeSettings(settings: Settings): Settings {
  return {
    ...settings,
    diffMode: settings.diffMode === "split" ? "split" : "unified",
    contextLines: clampSetting(settings.contextLines, 0, 20, 3),
    autoFetchMinutes: clampSetting(settings.autoFetchMinutes, 0, 60, 5),
    terminalFontSize: clampSetting(settings.terminalFontSize, 10, 20, 13),
    codeFontSize: clampSetting(settings.codeFontSize, 10, 20, 12),
    treeFontSize: clampSetting(settings.treeFontSize, 10, 18, 13),
    tabSize: clampSetting(settings.tabSize, 1, 8, 4),
    uiScale: Number.isFinite(settings.uiScale)
      ? Math.max(0.75, Math.min(1.5, settings.uiScale))
      : 1,
    defaultBranch: settings.defaultBranch?.trim() || "main",
    shell: settings.shell?.trim() || null,
    externalEditorCmd: settings.externalEditorCmd?.trim() || null,
    commitMessageCmd: settings.commitMessageCmd?.trim() || null,
  };
}
