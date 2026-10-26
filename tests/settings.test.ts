import { describe, expect, it } from "vitest";
import {
  clampSetting,
  matchEditorCommand,
  matchShell,
  normalizeSettings,
  validateBranchName,
  validateIdentityEmail,
  validateIdentityName,
} from "../src/lib/settings";
import type { DetectedApp, Settings } from "../src/lib/types";

const editors: DetectedApp[] = [
  { id: "vscode", label: "VS Code", command: "code -g {full}:{line}" },
  { id: "system", label: "System default app", command: "open {full}" },
];

const shells: DetectedApp[] = [
  { id: "/bin/zsh", label: "zsh", command: "/bin/zsh" },
  { id: "/bin/bash", label: "bash", command: "/bin/bash" },
];

const base: Settings = {
  theme: "system",
  ignoreWhitespace: false,
  contextLines: 3,
  autoFetchMinutes: 5,
  shell: null,
  externalEditorCmd: null,
  commitMessageCmd: null,
  terminalFontSize: 13,
  uiScale: 1,
  defaultBranch: "main",
  tabSize: 4,
  confirmTerminalClose: true,
  confirmTerminalRestart: true,
};

describe("identity validation", () => {
  it("accepts valid names and emails", () => {
    expect(validateIdentityName("Ada Lovelace")).toBeNull();
    expect(validateIdentityEmail("ada@example.com")).toBeNull();
    expect(validateIdentityEmail("a.b+tag@sub.example.co.uk")).toBeNull();
  });

  it("rejects empty and malformed values", () => {
    expect(validateIdentityName("   ")).not.toBeNull();
    expect(validateIdentityName("a\nb")).not.toBeNull();
    for (const bad of ["", "ada", "ada@", "@example.com", "ada@example", "a b@example.com", "a@@example.com", "ada@.com", "ada@example..com"]) {
      expect(validateIdentityEmail(bad), bad).not.toBeNull();
    }
  });
});

describe("branch validation", () => {
  it("accepts normal branch names", () => {
    for (const name of ["main", "feature/ALPH-162", "release-1.0"]) {
      expect(validateBranchName(name), name).toBeNull();
    }
  });

  it("rejects invalid branch names", () => {
    for (const name of ["", " ", "HEAD", "@", "-x", "/x", "x/", "x.", "a..b", "a b", "a~b", "a:b", "a?", ".hidden", "x.lock"]) {
      expect(validateBranchName(name), name).not.toBeNull();
    }
  });
});

describe("detected option matching", () => {
  it("matches the configured editor command", () => {
    expect(matchEditorCommand("code -g {full}:{line}", editors)?.id).toBe("vscode");
    expect(matchEditorCommand("vim {full}", editors)).toBeNull();
  });

  it("treats an unset editor as the system default fallback", () => {
    expect(matchEditorCommand(null, editors)?.id).toBe("system");
  });

  it("matches detected shells and leaves unknown paths custom", () => {
    expect(matchShell("/bin/zsh", shells)?.label).toBe("zsh");
    expect(matchShell(null, shells)).toBeNull();
    expect(matchShell("/opt/custom/fish", shells)).toBeNull();
  });
});

describe("settings normalization", () => {
  it("clamps numeric values into supported ranges", () => {
    expect(clampSetting(999, 0, 60, 5)).toBe(60);
    expect(clampSetting(-1, 0, 60, 5)).toBe(0);
    expect(clampSetting(Number.NaN, 0, 60, 5)).toBe(5);
  });

  it("normalizes a whole settings object", () => {
    const normalized = normalizeSettings({
      ...base,
      contextLines: 99,
      autoFetchMinutes: -5,
      terminalFontSize: 4,
      tabSize: 40,
      uiScale: Number.NaN,
      defaultBranch: "   ",
    });
    expect(normalized.contextLines).toBe(20);
    expect(normalized.autoFetchMinutes).toBe(0);
    expect(normalized.terminalFontSize).toBe(10);
    expect(normalized.tabSize).toBe(8);
    expect(normalized.uiScale).toBe(1);
    expect(normalized.defaultBranch).toBe("main");
  });
});
