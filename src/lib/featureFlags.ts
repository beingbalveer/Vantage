/**
 * Simple Feature Flag System for Vantage
 *
 * Allows selectively enabling or disabling each ticket/task at runtime:
 * - Stored in localStorage ('vantage:feature-flags')
 * - Default: true for all merged features (can be toggled individually)
 * - Accessible in Svelte stores, via helper functions, and on window.__FEATURE_FLAGS__
 */

import { writable, get } from "svelte/store";

export type FeatureFlagKey =
  | "ALPH_144_MULTI_PANE"
  | "ALPH_156_CONFLICT_FLOW"
  | "ALPH_157_TAGS"
  | "ALPH_158_SPLIT_IMAGE_DIFF"
  | "ALPH_159_EXPAND_WHOLE_FILE"
  | "ALPH_160_LINE_PARTIAL_COMMIT"
  | "ALPH_161_SHORTCUT_REGISTRY"
  | "ALPH_162_SETTINGS_PARITY"
  | "ALPH_163_NATIVE_MENU"
  | "ALPH_164_ACCESSIBILITY"
  | "ALPH_165_BRANDING";

export interface FeatureFlagMeta {
  key: FeatureFlagKey;
  ticket: string;
  name: string;
  description: string;
  defaultValue: boolean;
}

export const FEATURE_METADATA: FeatureFlagMeta[] = [
  {
    key: "ALPH_144_MULTI_PANE",
    ticket: "ALPH-144",
    name: "Multi-Pane Workspace",
    description: "Render 1-3 independent panes side-by-side with hotkey cycling (⌘1/⌘2/⌘3)",
    defaultValue: true,
  },
  {
    key: "ALPH_156_CONFLICT_FLOW",
    ticket: "ALPH-156",
    name: "Merge Conflict Flow",
    description: "Detect merge conflicts in progress and show resolution banner with Continue/Abort",
    defaultValue: true,
  },
  {
    key: "ALPH_157_TAGS",
    ticket: "ALPH-157",
    name: "Git Tags Support",
    description: "Create, view details, push, and filter Git tags in commit rows and headers",
    defaultValue: true,
  },
  {
    key: "ALPH_158_SPLIT_IMAGE_DIFF",
    ticket: "ALPH-158",
    name: "Split & Image Diff",
    description: "Side-by-side split diff toggle and visual image / binary diff views",
    defaultValue: true,
  },
  {
    key: "ALPH_159_EXPAND_WHOLE_FILE",
    ticket: "ALPH-159",
    name: "Expand Whole File",
    description: "Expand diff view to show full context of the entire file around changes",
    defaultValue: true,
  },
  {
    key: "ALPH_160_LINE_PARTIAL_COMMIT",
    ticket: "ALPH-160",
    name: "Line-Level Staging & Discard",
    description: "Stage or discard individual lines and hunks directly from the diff view",
    defaultValue: true,
  },
  {
    key: "ALPH_161_SHORTCUT_REGISTRY",
    ticket: "ALPH-161",
    name: "Context Shortcut Registry",
    description: "Context-aware global and panel keyboard shortcut system with help viewer",
    defaultValue: true,
  },
  {
    key: "ALPH_162_SETTINGS_PARITY",
    ticket: "ALPH-162",
    name: "Full Settings Parity",
    description: "Deep Git config detection (Git identity, editor, shell, auto-fetch interval)",
    defaultValue: true,
  },
  {
    key: "ALPH_163_NATIVE_MENU",
    ticket: "ALPH-163",
    name: "Native Menu & Window Restore",
    description: "macOS application menu bar and geometry restoration across restarts",
    defaultValue: true,
  },
  {
    key: "ALPH_164_ACCESSIBILITY",
    ticket: "ALPH-164",
    name: "Accessibility & Reduced Motion",
    description: "High contrast focus rings, ARIA landmarks, dialog focus traps and reduced motion",
    defaultValue: true,
  },
  {
    key: "ALPH_165_BRANDING",
    ticket: "ALPH-165",
    name: "Vantage Branding Consistency",
    description: "Consistent Vantage product name, dialog titles, homebrew, and documentation",
    defaultValue: true,
  },
];

const STORAGE_KEY = "vantage:feature-flags";

function getDefaultFlags(): Record<FeatureFlagKey, boolean> {
  const flags = {} as Record<FeatureFlagKey, boolean>;
  for (const meta of FEATURE_METADATA) {
    flags[meta.key] = meta.defaultValue;
  }
  return flags;
}

function loadStoredFlags(): Record<FeatureFlagKey, boolean> {
  const defaults = getDefaultFlags();
  if (typeof window === "undefined" || !window.localStorage) {
    return defaults;
  }
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return defaults;
    const parsed = JSON.parse(raw);
    return { ...defaults, ...parsed };
  } catch (e) {
    console.warn("Failed to load feature flags from localStorage:", e);
    return defaults;
  }
}

export const featureFlags = writable<Record<FeatureFlagKey, boolean>>(loadStoredFlags());

export function isFeatureEnabled(key: FeatureFlagKey): boolean {
  return get(featureFlags)[key] ?? true;
}

export function setFeatureFlag(key: FeatureFlagKey, enabled: boolean): void {
  featureFlags.update((flags) => {
    const next = { ...flags, [key]: enabled };
    if (typeof window !== "undefined" && window.localStorage) {
      try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
      } catch (e) {
        console.warn("Failed to save feature flag to localStorage:", e);
      }
    }
    return next;
  });
}

export function resetFeatureFlags(): void {
  const defaults = getDefaultFlags();
  featureFlags.set(defaults);
  if (typeof window !== "undefined" && window.localStorage) {
    try {
      localStorage.removeItem(STORAGE_KEY);
    } catch {}
  }
}

// Attach to window for convenient debugging in console
if (typeof window !== "undefined") {
  (window as any).__FEATURE_FLAGS__ = {
    get: isFeatureEnabled,
    set: setFeatureFlag,
    reset: resetFeatureFlags,
    getAll: () => get(featureFlags),
    metadata: FEATURE_METADATA,
    toggle: (key: FeatureFlagKey) => setFeatureFlag(key, !isFeatureEnabled(key)),
  };
}
