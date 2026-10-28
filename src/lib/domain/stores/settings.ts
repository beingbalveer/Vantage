import { writable, get } from "svelte/store";
import type { Settings } from "../../platform/types";
import * as api from "../../platform/api";
import { addToast } from "../../util/toast";

export const settings = writable<Settings>({
  theme: "system",
  diffMode: "unified",
  ignoreWhitespace: false,
  contextLines: 3,
  autoFetchMinutes: 5,
  shell: null,
  externalEditorCmd: null,
  commitMessageCmd: null,
  terminalFontSize: 13,
  codeFontSize: 12,
  uiScale: 1.0,
  defaultBranch: "main",
  tabSize: 4,
  confirmTerminalRestart: true,
});

let systemMediaWatcher: MediaQueryList | null = null;

function setupSystemThemeWatcher() {
  if (typeof window === "undefined" || !window.matchMedia) return;
  if (!systemMediaWatcher) {
    systemMediaWatcher = window.matchMedia("(prefers-color-scheme: dark)");
    systemMediaWatcher.addEventListener("change", (e) => {
      const current = get(settings);
      if (current.theme === "system") {
        document.documentElement.setAttribute("data-theme", e.matches ? "dark" : "light");
      }
    });
  }
}

export function applyTheme(theme: "system" | "light" | "dark") {
  if (typeof document === "undefined") return;
  setupSystemThemeWatcher();
  const root = document.documentElement;

  if (theme === "system") {
    const isDark = window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches;
    root.setAttribute("data-theme", isDark ? "dark" : "light");
  } else {
    root.setAttribute("data-theme", theme);
  }
}

export function applyUiScale(scale: number) {
  if (typeof document === "undefined") return;
  const clamped = Math.max(0.75, Math.min(1.5, scale || 1.0));
  // Use CSS custom property or body zoom
  document.documentElement.style.setProperty("--ui-scale", `${clamped}`);
  (document.body.style as any).zoom = `${clamped}`;
}

/** Apply the shared code indentation width used by the diff and viewer. */
export function applyCodeTabSize(tabSize: number) {
  if (typeof document === "undefined") return;
  const clamped = Math.max(1, Math.min(8, Math.round(tabSize || 4)));
  document.documentElement.style.setProperty("--code-tab-size", `${clamped}`);
}

/** Apply the Files viewer's text size and the line height derived from it. */
export function applyCodeFontSize(size: number) {
  if (typeof document === "undefined") return;
  const clamped = Math.max(10, Math.min(20, Math.round(size || 12)));
  const root = document.documentElement;
  root.style.setProperty("--code-font-size", `${clamped}px`);
  root.style.setProperty("--code-line-height", `${Math.round(clamped * 1.6)}px`);
}

export async function loadSettings() {
  try {
    const s = await api.settingsGet();
    settings.set(s);
    applyTheme(s.theme);
    applyUiScale(s.uiScale);
    applyCodeTabSize(s.tabSize);
    applyCodeFontSize(s.codeFontSize);
  } catch (e: any) {
    console.error("Failed to load settings:", e);
    addToast({
      message: "Failed to load preferences from disk: " + (e?.message || e),
      kind: "error",
    });
  }
}

export async function updateSettings(newSettings: Settings) {
  const previous = get(settings);
  settings.set(newSettings);

  // Theme, zoom and tab width are global side effects; only touch the DOM for
  // the fields that actually changed so unrelated edits don't force a reflow.
  if (previous.theme !== newSettings.theme) applyTheme(newSettings.theme);
  if (previous.uiScale !== newSettings.uiScale) applyUiScale(newSettings.uiScale);
  if (previous.tabSize !== newSettings.tabSize) applyCodeTabSize(newSettings.tabSize);
  if (previous.codeFontSize !== newSettings.codeFontSize) {
    applyCodeFontSize(newSettings.codeFontSize);
  }

  try {
    await api.settingsSet(newSettings);
  } catch (e: any) {
    console.error("Failed to save settings:", e);
    settings.set(previous);
    applyTheme(previous.theme);
    applyUiScale(previous.uiScale);
    applyCodeTabSize(previous.tabSize);
    applyCodeFontSize(previous.codeFontSize);
    addToast({
      message: "Failed to save settings: " + (e?.message || e),
      kind: "error",
    });
  }
}
