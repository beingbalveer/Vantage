import { writable, get } from "svelte/store";
import type { Settings } from "../types";
import * as api from "../api";
import { addToast } from "./ui";

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
  uiScale: 1.0,
  defaultBranch: "main",
  tabSize: 4,
  confirmTerminalClose: true,
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

export async function loadSettings() {
  try {
    const s = await api.settingsGet();
    settings.set(s);
    applyTheme(s.theme);
    applyUiScale(s.uiScale);
    applyCodeTabSize(s.tabSize);
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

  try {
    await api.settingsSet(newSettings);
  } catch (e: any) {
    console.error("Failed to save settings:", e);
    settings.set(previous);
    applyTheme(previous.theme);
    applyUiScale(previous.uiScale);
    applyCodeTabSize(previous.tabSize);
    addToast({
      message: "Failed to save settings: " + (e?.message || e),
      kind: "error",
    });
  }
}
