import { describe, it, expect } from "vitest";
import {
  COMMANDS,
  SHORTCUT_CATEGORIES,
  commandById,
  formatKeys,
  isEditableTarget,
  isTerminalTarget,
  matchShortcut,
  resolveCommand,
  findFlaggedCommitIndex,
  type Command,
  type KeyEventLike,
  type ShortcutContext,
} from "../src/lib/platform/keys";

function key(k: string, mods: Partial<KeyEventLike> = {}): KeyEventLike {
  return {
    key: k,
    metaKey: false,
    ctrlKey: false,
    shiftKey: false,
    altKey: false,
    ...mods,
  };
}

function fromShortcut(shortcut: string, mac: boolean): KeyEventLike {
  const parts = shortcut.split("+");
  const base = parts[parts.length - 1];
  const eventKey =
    base === "Esc" ? "Escape" : base === "{" || base === "}" || base === "?" ? base : base;
  return key(eventKey, {
    metaKey: mac ? parts.includes("Mod") : false,
    ctrlKey: mac ? parts.includes("Ctrl") : parts.includes("Mod") || parts.includes("Ctrl"),
    shiftKey: parts.includes("Shift") || base === "{" || base === "}" || base === "?",
    altKey: parts.includes("Alt"),
    code:
      base === "\\" ? "Backslash" : base === "`" ? "Backquote" : undefined,
  });
}

const history: ShortcutContext[] = ["global", "history", "diff"];
const files: ShortcutContext[] = ["global", "files"];

describe("command registry", () => {
  it("has unique command ids and non-empty labels", () => {
    const ids = COMMANDS.map((c) => c.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const command of COMMANDS) {
      expect(command.label.trim().length, command.id).toBeGreaterThan(0);
      expect(SHORTCUT_CATEGORIES, command.id).toContain(command.category);
    }
  });

  it("has no key bound twice inside the same context (c means one thing)", () => {
    for (let i = 0; i < COMMANDS.length; i++) {
      for (let j = i + 1; j < COMMANDS.length; j++) {
        const a = COMMANDS[i];
        const b = COMMANDS[j];
        const overlap = a.contexts.some((context) => b.contexts.includes(context));
        if (!overlap) continue;
        const conflict = a.keys.filter((k) => b.keys.includes(k));
        expect(conflict, `${a.id} and ${b.id} both bind ${conflict.join(", ")}`).toEqual([]);
      }
    }
  });

  it("resolves every advertised binding to its owning command", () => {
    for (const command of COMMANDS) {
      for (const shortcut of command.keys) {
        const specific =
          command.contexts.find((c) => c !== "global" && c !== "dialog-open") ?? "global";
        const contexts: ShortcutContext[] = command.contexts.includes("dialog-open")
          ? ["global", "dialog-open"]
          : ["global", specific];
        const resolved = resolveCommand(COMMANDS, fromShortcut(shortcut, true), {
          activeContexts: contexts,
          mac: true,
        });
        expect(resolved?.command.id, `${shortcut} for ${command.id}`).toBe(command.id);
      }
    }
  });

  it("formats multiple bindings", () => {
    expect(formatKeys(["Mod+K", "Ctrl+Shift+P"], true)).toBe("⌘+K · ⌃+⇧+P");
  });
});

describe("matchShortcut", () => {
  it("matches Mod on macOS and Ctrl on Windows/Linux", () => {
    expect(matchShortcut(key("k", { metaKey: true }), "Mod+K", true)).toBe(true);
    expect(matchShortcut(key("k", { ctrlKey: true }), "Mod+K", false)).toBe(true);
    expect(matchShortcut(key("K", { metaKey: true, shiftKey: true }), "Mod+K", true)).toBe(false);
    expect(matchShortcut(key("k", { metaKey: true, shiftKey: true }), "Mod+Shift+K", true)).toBe(true);
  });

  it("supports the explicit Ctrl chord used by the palette", () => {
    expect(matchShortcut(key("P", { ctrlKey: true, shiftKey: true }), "Ctrl+Shift+P", true)).toBe(true);
    expect(matchShortcut(key("P", { metaKey: true, shiftKey: true }), "Ctrl+Shift+P", true)).toBe(false);
  });

  it("handles punctuation that carries Shift implicitly", () => {
    expect(matchShortcut(key("{", { shiftKey: true }), "{", true)).toBe(true);
    expect(matchShortcut(key("}", { shiftKey: true }), "}", true)).toBe(true);
    expect(matchShortcut(key("?", { shiftKey: true }), "?", true)).toBe(true);
    expect(matchShortcut(key("j", { shiftKey: true }), "j", true)).toBe(false);
    expect(matchShortcut(key("J", { shiftKey: true }), "Shift+j", true)).toBe(true);
  });

  it("handles Escape, Enter, backslash and backtick", () => {
    expect(matchShortcut(key("Escape"), "Esc", true)).toBe(true);
    expect(matchShortcut(key("Enter", { metaKey: true }), "Mod+Enter", true)).toBe(true);
    expect(matchShortcut(key("\\", { metaKey: true }), "Mod+\\", true)).toBe(true);
    expect(matchShortcut(key("`", { metaKey: true }), "Mod+`", true)).toBe(true);
  });
});

describe("typing / terminal guards", () => {
  const editableTargets: unknown[] = [
    { tagName: "INPUT", closest: () => null },
    { tagName: "TEXTAREA", closest: () => null },
    { tagName: "SELECT", closest: () => null },
    { tagName: "DIV", isContentEditable: true, closest: () => null },
    { tagName: "DIV", closest: (sel: string) => (sel.includes("contenteditable") ? {} : null) },
    { tagName: "TEXTAREA", closest: (sel: string) => (sel === ".xterm" ? {} : null) },
  ];

  it("detects form controls, contentEditable and the terminal", () => {
    for (const target of editableTargets) {
      expect(isEditableTarget(target), String((target as any).tagName)).toBe(true);
    }
    expect(isEditableTarget({ tagName: "DIV", closest: () => null })).toBe(false);
    expect(isEditableTarget(null)).toBe(false);
    expect(isTerminalTarget({ tagName: "TEXTAREA", closest: (s: string) => (s === ".xterm" ? {} : null) })).toBe(true);
    expect(isTerminalTarget({ tagName: "DIV", closest: () => null })).toBe(false);
  });

  it("never fires a single-letter shortcut while typing", () => {
    for (const event of [key("j"), key("c"), key("r"), key("e"), key("n")]) {
      const resolved = resolveCommand(COMMANDS, event, {
        activeContexts: history,
        editable: true,
        mac: true,
      });
      expect(resolved, event.key).toBeNull();
    }
  });

  it("does not intercept ordinary modifier typing except explicit commands", () => {
    expect(
      resolveCommand(COMMANDS, key("k", { metaKey: true }), {
        activeContexts: history,
        editable: true,
        mac: true,
      })
    ).toBeNull();
    expect(
      resolveCommand(COMMANDS, key("Escape"), {
        activeContexts: history,
        editable: true,
        mac: true,
      })?.command.id
    ).toBe("escape");
    expect(
      resolveCommand(COMMANDS, key("Enter", { metaKey: true }), {
        activeContexts: history,
        editable: true,
        mac: true,
      })?.command.id
    ).toBe("commit");
  });

  it("sends everything to the terminal unless the command opts in", () => {
    const typingInShell = {
      activeContexts: [...history, "terminal"] as ShortcutContext[],
      editable: true,
      inTerminal: true,
      mac: false,
    };
    expect(resolveCommand(COMMANDS, key("j"), typingInShell)).toBeNull();
    expect(resolveCommand(COMMANDS, key("c"), typingInShell)).toBeNull();
    expect(resolveCommand(COMMANDS, key("Escape"), typingInShell)).toBeNull();
    // Ctrl+K belongs to the shell on Windows/Linux.
    expect(resolveCommand(COMMANDS, key("k", { ctrlKey: true }), typingInShell)).toBeNull();
    expect(
      resolveCommand(COMMANDS, key("P", { ctrlKey: true, shiftKey: true }), typingInShell)?.command.id
    ).toBe("palette");
    expect(
      resolveCommand(COMMANDS, key("`", { ctrlKey: true }), typingInShell)?.command.id
    ).toBe("toggle_terminal");
  });

  it("allows Mod+K in the macOS terminal", () => {
    const resolved = resolveCommand(COMMANDS, key("k", { metaKey: true }), {
      activeContexts: [...history, "terminal"],
      editable: true,
      inTerminal: true,
      mac: true,
    });
    expect(resolved?.command.id).toBe("palette");
  });
});

describe("context resolution", () => {
  it("makes c mean exactly one thing per context", () => {
    const inHistory = resolveCommand(COMMANDS, key("c"), {
      activeContexts: history,
      mac: true,
    });
    expect(inHistory?.command.id).toBe("copy_sha");
    expect(inHistory?.context).toBe("history");

    const inFiles = resolveCommand(COMMANDS, key("c"), {
      activeContexts: files,
      mac: true,
    });
    expect(inFiles?.command.id).toBe("copy_path");
    expect(inFiles?.context).toBe("files");

    // No surface means no c binding at all.
    expect(
      resolveCommand(COMMANDS, key("c"), { activeContexts: ["global"], mac: true })
    ).toBeNull();
  });

  it("prefers a specific context over global for a duplicate key", () => {
    const commands: Command[] = [
      { id: "global_x", label: "Global", keys: ["x"], contexts: ["global"], category: "Navigation" },
      { id: "history_x", label: "History", keys: ["x"], contexts: ["history"], category: "Navigation" },
    ];
    const resolved = resolveCommand(commands, key("x"), {
      activeContexts: ["global", "history"],
      mac: true,
    });
    expect(resolved?.command.id).toBe("history_x");
    expect(resolved?.context).toBe("history");
  });

  it("is deterministic when two commands share a key and context", () => {
    const commands: Command[] = [
      { id: "first", label: "First", keys: ["x"], contexts: ["history"], category: "Navigation" },
      { id: "second", label: "Second", keys: ["x"], contexts: ["history"], category: "Navigation" },
    ];
    const resolved = resolveCommand(commands, key("x"), {
      activeContexts: ["global", "history"],
      mac: true,
    });
    expect(resolved?.command.id).toBe("first");
  });

  it("locks out normal shortcuts while a dialog is open", () => {
    const dialog: ShortcutContext[] = ["global", "dialog-open"];
    expect(resolveCommand(COMMANDS, key("j"), { activeContexts: dialog, mac: true })).toBeNull();
    expect(resolveCommand(COMMANDS, key("c"), { activeContexts: dialog, mac: true })).toBeNull();
    expect(
      resolveCommand(COMMANDS, key("k", { metaKey: true }), { activeContexts: dialog, mac: true })
        ?.command.id
    ).toBe("palette");
    expect(
      resolveCommand(COMMANDS, key("Escape"), { activeContexts: dialog, mac: true })?.command.id
    ).toBe("escape");
  });

  it("exposes palette and help commands by id", () => {
    expect(commandById("palette")?.keys).toContain("Mod+K");
    expect(commandById("shortcuts_help")?.label).toMatch(/shortcut/i);
  });
});

describe("file tab shortcuts", () => {
  it("closes a file tab in Files and a pane elsewhere", () => {
    const inFiles = resolveCommand(COMMANDS, fromShortcut("Mod+W", true), {
      activeContexts: files,
      mac: true,
    });
    expect(inFiles?.command.id).toBe("close_file");
    expect(inFiles?.context).toBe("files");

    const inHistory = resolveCommand(COMMANDS, fromShortcut("Mod+W", true), {
      activeContexts: history,
      mac: true,
    });
    expect(inHistory?.command.id).toBe("close_pane");
    expect(inHistory?.context).toBe("global");
  });

  it("closes all tabs and walks the strip", () => {
    const closeAll = resolveCommand(COMMANDS, fromShortcut("Mod+Shift+W", true), {
      activeContexts: files,
      mac: true,
    });
    expect(closeAll?.command.id).toBe("close_all_files");

    const next = resolveCommand(COMMANDS, fromShortcut("Ctrl+Tab", true), {
      activeContexts: files,
      mac: true,
    });
    expect(next?.command.id).toBe("next_file_tab");

    const prev = resolveCommand(COMMANDS, fromShortcut("Ctrl+Shift+Tab", true), {
      activeContexts: files,
      mac: true,
    });
    expect(prev?.command.id).toBe("prev_file_tab");
  });

  it("uses Mod+B for file search in the Files context only", () => {
    const inFiles = resolveCommand(COMMANDS, fromShortcut("Mod+B", true), {
      activeContexts: files,
      mac: true,
    });
    expect(inFiles?.command.id).toBe("focus_file_search");
    expect(inFiles?.context).toBe("files");

    const inHistory = resolveCommand(COMMANDS, fromShortcut("Mod+B", true), {
      activeContexts: history,
      mac: true,
    });
    expect(inHistory).toBeNull();
  });

  it("supports the Mod+Shift+E explorer alias", () => {
    const resolved = resolveCommand(COMMANDS, fromShortcut("Mod+Shift+E", true), {
      activeContexts: files,
      mac: true,
    });
    expect(resolved?.command.id).toBe("focus_file_search");
  });
});

describe("flagged commit navigation", () => {
  const commits = [
    { sha: "a", flags: [] },
    { sha: "b", flags: [{ kind: "large" }] },
    { sha: "c", flags: [] },
    { sha: "d", flags: [{ kind: "secrets" }] },
  ];

  it("finds the next and previous flagged commit", () => {
    expect(findFlaggedCommitIndex(commits, -1, "next")).toBe(1);
    expect(findFlaggedCommitIndex(commits, 1, "next")).toBe(3);
    expect(findFlaggedCommitIndex(commits, 3, "prev")).toBe(1);
  });

  it("returns the current index when nothing else is flagged", () => {
    expect(findFlaggedCommitIndex(commits, 3, "next")).toBe(3);
    expect(findFlaggedCommitIndex(commits, 1, "prev")).toBe(1);
  });
});
