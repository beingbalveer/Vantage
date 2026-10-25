import { describe, expect, it } from "vitest";
import { readdirSync, readFileSync } from "node:fs";
import { extname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

/**
 * Branding guard (ALPH-165).
 *
 * The product used to be prototyped under the working name "Relay". That name
 * must never show up in user-visible text again, so this test scans the
 * front-end source for the retired brand and fails if it reappears.
 *
 * A few uses of the *string* "relay" are load-bearing and must NOT be renamed:
 * they are identifiers, not copy. Those are listed in ALLOWLIST below so the
 * guard stays precise and only catches real UI strings. If a legitimate
 * historical mention of the old name needs to exist, add it here with a reason.
 */
const ROOT = fileURLToPath(new URL("..", import.meta.url));

/** Matches the retired brand as a word, regardless of case. */
const RETIRED_BRAND = /\brelay\b/i;

const ALLOWLIST: Array<{ pattern: RegExp; reason: string }> = [
  {
    pattern: /\brelay-[a-z0-9-]+/gi,
    reason: "CSS class prefix (relay-btn, relay-modal-card, …) — styling only",
  },
  {
    pattern: /\brelay_[a-z0-9_]+/gi,
    reason:
      "legacy persisted storage key read as a fallback (relay_sidebar_collapsed, …) — renaming breaks saved user state",
  },
  {
    pattern: /\bRELAY-UI\b/gi,
    reason: "design-review comment tag (RELAY-UI · UI-xx) — not user-visible",
  },
];

/** Text extensions that can hold user-visible copy. */
const SCANNED_EXTENSIONS = new Set([".svelte", ".ts", ".js", ".css", ".html"]);

function collectSourceFiles(dir: string): string[] {
  const files: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === "node_modules" || entry.name === "dist") continue;
    const full = join(dir, entry.name);
    if (entry.isDirectory()) files.push(...collectSourceFiles(full));
    else if (SCANNED_EXTENSIONS.has(extname(entry.name))) files.push(full);
  }
  return files;
}

/** Remove allow-listed, non-visible uses so only real copy remains. */
function scrubAllowed(line: string): string {
  return ALLOWLIST.reduce(
    (text, { pattern }) => text.replace(new RegExp(pattern.source, pattern.flags), ""),
    line,
  );
}

describe("branding guard", () => {
  it("never shows the retired product name 'Relay' in user-visible source", () => {
    const files = [...collectSourceFiles(join(ROOT, "src")), join(ROOT, "index.html")];
    const offenders: string[] = [];

    for (const file of files) {
      const lines = readFileSync(file, "utf8").split(/\r?\n/);
      lines.forEach((line, index) => {
        if (RETIRED_BRAND.test(scrubAllowed(line))) {
          offenders.push(`${relative(ROOT, file)}:${index + 1}: ${line.trim()}`);
        }
      });
    }

    expect(
      offenders,
      `The retired product name "Relay" leaked back into UI source:\n${offenders.join("\n")}`,
    ).toEqual([]);
  });

  it("keeps the app name as 'Vantage' in the HTML title and Tauri window config", () => {
    const html = readFileSync(join(ROOT, "index.html"), "utf8");
    const htmlTitle = html.match(/<title>([^<]*)<\/title>/)?.[1]?.trim();
    expect(htmlTitle).toBe("Vantage");

    const conf = JSON.parse(
      readFileSync(join(ROOT, "src-tauri", "tauri.conf.json"), "utf8"),
    ) as { productName?: string; app?: { windows?: Array<{ title?: string }> } };
    expect(conf.productName).toBe("Vantage");
    expect(conf.app?.windows?.[0]?.title).toBe("Vantage");
  });
});
