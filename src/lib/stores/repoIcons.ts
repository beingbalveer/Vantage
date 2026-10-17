import { writable, get } from "svelte/store";
import type { RepoStackInfo } from "../types";
import { repoDetectStack } from "../api";

/**
 * Repository icon metadata, persisted by repository-entry id so that different
 * branch views of the same working copy can show different icons (RELAY-UI ·
 * UI-03/UI-07).
 */
export const appIcons = [
  "code",
  "git-branch",
  "grid",
  "terminal",
  "folder",
  "worktree",
] as const;

export type AppIcon = (typeof appIcons)[number];

export type RepoIcon =
  | { kind: "app"; value: AppIcon }
  | { kind: "image"; value: string }
  | { kind: "tech"; value: string }
  | { kind: "role"; value: string }
  | { kind: "emoji"; value: string }
  | { kind: "letter"; value: string; color?: string }
  | { kind: "auto" };

export const techIcons = [
  { id: "rust", label: "Rust" },
  { id: "react", label: "React" },
  { id: "nextjs", label: "Next.js" },
  { id: "vue", label: "Vue" },
  { id: "svelte", label: "Svelte" },
  { id: "angular", label: "Angular" },
  { id: "typescript", label: "TypeScript" },
  { id: "nodejs", label: "Node.js" },
  { id: "python", label: "Python" },
  { id: "go", label: "Go" },
  { id: "flutter", label: "Flutter" },
  { id: "swift", label: "Swift" },
  { id: "java", label: "Java / Kotlin" },
  { id: "docker", label: "Docker" },
  { id: "ruby", label: "Ruby" },
  { id: "php", label: "PHP" },
] as const;

export const roleIcons = [
  { id: "server", label: "Server / Backend" },
  { id: "globe", label: "Web / Frontend" },
  { id: "smartphone", label: "Mobile" },
  { id: "database", label: "Database" },
  { id: "cloud", label: "Cloud / Infra" },
  { id: "package", label: "Package / Lib" },
  { id: "terminal", label: "CLI / Tool" },
  { id: "git-branch", label: "Git / VCS" },
] as const;

export const presetEmojis = [
  "🚀", "⚡", "📦", "🔥", "💻", "🌐", "🛠️", "🎯",
  "🧠", "⚙️", "🧪", "🎨", "🔒", "📱", "📊", "🤖",
  "⭐", "✨", "🧩", "💎", "🏷️", "📁", "☕", "🦀",
] as const;

const STORAGE_KEY = "vantage_repo_icons_v1";
const LEGACY_STORAGE_KEY = "relay_repo_icons_v1";
const DETECTED_KEY = "vantage_detected_stacks_v1";
const LEGACY_DETECTED_KEY = "relay_detected_stacks_v1";
const MAX_IMAGE_BYTES = 512 * 1024;

function isAppIcon(value: unknown): value is AppIcon {
  return typeof value === "string" && (appIcons as readonly string[]).includes(value);
}

function isDataImage(value: unknown): value is string {
  return typeof value === "string" && value.startsWith("data:image/");
}

function sanitize(raw: unknown): Record<string, RepoIcon> {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return {};
  const out: Record<string, RepoIcon> = {};
  for (const [id, value] of Object.entries(raw as Record<string, unknown>)) {
    if (!value || typeof value !== "object") continue;
    const icon = value as { kind?: unknown; value?: unknown; color?: unknown };
    if (icon.kind === "app" && isAppIcon(icon.value)) {
      out[id] = { kind: "app", value: icon.value };
    } else if (icon.kind === "image" && isDataImage(icon.value)) {
      out[id] = { kind: "image", value: icon.value };
    } else if (icon.kind === "tech" && typeof icon.value === "string") {
      out[id] = { kind: "tech", value: icon.value };
    } else if (icon.kind === "role" && typeof icon.value === "string") {
      out[id] = { kind: "role", value: icon.value };
    } else if (icon.kind === "emoji" && typeof icon.value === "string") {
      out[id] = { kind: "emoji", value: icon.value };
    } else if (icon.kind === "letter" && typeof icon.value === "string") {
      out[id] = {
        kind: "letter",
        value: icon.value,
        color: typeof icon.color === "string" ? icon.color : undefined,
      };
    } else if (icon.kind === "auto") {
      out[id] = { kind: "auto" };
    }
  }
  return out;
}

function load(): Record<string, RepoIcon> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY) ?? localStorage.getItem(LEGACY_STORAGE_KEY);
    if (!raw) return {};
    return sanitize(JSON.parse(raw));
  } catch {
    // Corrupt metadata must not break startup.
    return {};
  }
}

export const repoIcons = writable<Record<string, RepoIcon>>(load());

/**
 * Persist an icon choice. Returns false (and leaves existing metadata intact)
 * when storage is unavailable or the value cannot be written.
 */
export function saveRepoIcon(id: string, icon: RepoIcon): boolean {
  if (!id) return false;
  if (icon.kind === "image" && !isDataImage(icon.value)) return false;
  if (icon.kind === "app" && !isAppIcon(icon.value)) return false;
  const next = { ...get(repoIcons), [id]: icon };
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
    repoIcons.set(next);
    return true;
  } catch {
    return false;
  }
}

/**
 * Clear custom icon override for a repo, resetting it back to auto-detected.
 */
export function clearRepoIcon(id: string): boolean {
  if (!id) return false;
  const current = get(repoIcons);
  if (!current[id]) return true;
  const next = { ...current };
  delete next[id];
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
    repoIcons.set(next);
    return true;
  } catch {
    return false;
  }
}

function loadDetectedStacks(): Record<string, RepoStackInfo> {
  try {
    const raw = localStorage.getItem(DETECTED_KEY) ?? localStorage.getItem(LEGACY_DETECTED_KEY);
    return raw ? JSON.parse(raw) : {};
  } catch {
    return {};
  }
}

export const detectedStacks = writable<Record<string, RepoStackInfo>>(loadDetectedStacks());

export async function detectAndCacheStack(
  repoId: string,
  path: string
): Promise<RepoStackInfo> {
  if (!repoId || !path) return {};
  const current = get(detectedStacks);
  if (current[repoId] && current[repoId].detected) {
    return current[repoId];
  }
  try {
    const info = await repoDetectStack(path);
    if (info) {
      detectedStacks.update((prev) => {
        const next = { ...prev, [repoId]: info };
        try {
          localStorage.setItem(DETECTED_KEY, JSON.stringify(next));
        } catch {}
        return next;
      });
      return info;
    }
  } catch {
    // Best effort detection failure
  }
  return {};
}

/** Read a user-selected file as a data URL, validating type and size. */
export function readIconFile(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    if (!file.type.startsWith("image/")) {
      reject(new Error("Choose an image file (PNG, JPEG, GIF, or WebP)."));
      return;
    }
    if (file.size > MAX_IMAGE_BYTES) {
      reject(new Error("Image is too large. Choose a file under 512 KB."));
      return;
    }
    const reader = new FileReader();
    reader.onerror = () => reject(new Error("Could not read that image."));
    reader.onload = () => {
      const result = reader.result;
      if (typeof result === "string" && isDataImage(result)) resolve(result);
      else reject(new Error("Could not read that image."));
    };
    reader.readAsDataURL(file);
  });
}

/**
 * Deterministic fallback for entries with no saved metadata. Hashing the id
 * keeps the icon stable across list reordering and restarts.
 */
export function defaultAppIcon(id: string): AppIcon {
  let hash = 0;
  const key = id || "vantage";
  for (let i = 0; i < key.length; i++) {
    hash = (hash * 31 + key.charCodeAt(i)) >>> 0;
  }
  return appIcons[hash % appIcons.length];
}

/** Compact, human-readable branch label used in the collapsed dock. */
export function shortBranch(branch: string | null | undefined): string {
  if (!branch) return "HEAD";
  const trimmed = branch.trim();
  if (!trimmed) return "HEAD";
  const tail = trimmed.split("/").filter(Boolean).pop() || trimmed;
  if (tail.length <= 6) return tail;
  const clipped = tail.slice(0, 6).replace(/[^a-zA-Z0-9]+$/, "");
  return clipped || tail.slice(0, 6);
}
