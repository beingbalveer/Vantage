#!/usr/bin/env node
/**
 * Design-system ratchet.
 *
 * The UI has a token layer (`src/lib/design/styles/tokens.css`) but a lot of
 * legacy component CSS still hardcodes colors and type sizes. A hard ban would
 * fail on day one, so this guard records the current state in
 * `scripts/design-baseline.json` and fails only when a file gets *worse*.
 *
 * Run `node scripts/design-guard.mjs --update` after intentionally reducing
 * violations, and treat the baseline as a budget that should shrink over time.
 */
import { readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { join, relative } from "node:path";

const ROOT = process.cwd();
const SCAN_DIRS = ["src"];
const BASELINE_PATH = join(ROOT, "scripts", "design-baseline.json");

const RULES = [
  { key: "hex", label: "raw hex colors", regex: /#[0-9a-fA-F]{3,8}\b/g },
  { key: "rgb", label: "rgb()/hsl() literals", regex: /\b(?:rgb|rgba|hsl|hsla)\(/g },
  { key: "fontSize", label: "hardcoded font sizes", regex: /font-size\s*:\s*\d+(?:\.\d+)?px/g },
  { key: "transitionAll", label: "transition: all", regex: /transition\s*:\s*all\b/g },
  { key: "important", label: "!important", regex: /!important/g },
];

function walk(dir) {
  const out = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      out.push(...walk(full));
    } else if (full.endsWith(".svelte")) {
      out.push(full);
    }
  }
  return out;
}

const files = SCAN_DIRS.flatMap((dir) => walk(join(ROOT, dir)));
const current = {};

for (const file of files) {
  const rel = relative(ROOT, file).split("\\").join("/");
  const text = readFileSync(file, "utf8");
  const counts = {};
  for (const rule of RULES) {
    const matches = text.match(rule.regex);
    if (matches) counts[rule.key] = matches.length;
  }
  if (Object.keys(counts).length > 0) current[rel] = counts;
}

const update = process.argv.includes("--update");
if (update) {
  writeFileSync(BASELINE_PATH, JSON.stringify(current, null, 2) + "\n");
  console.log(`design guard: baseline updated (${Object.keys(current).length} files)`);
  process.exit(0);
}

let baseline;
try {
  baseline = JSON.parse(readFileSync(BASELINE_PATH, "utf8"));
} catch {
  console.error("design guard: missing scripts/design-baseline.json (run with --update)");
  process.exit(1);
}

const regressions = [];
const totals = Object.fromEntries(RULES.map((rule) => [rule.key, 0]));

for (const [file, counts] of Object.entries(current)) {
  const allowed = baseline[file] ?? {};
  for (const rule of RULES) {
    const count = counts[rule.key] ?? 0;
    totals[rule.key] += count;
    const budget = allowed[rule.key] ?? 0;
    if (count > budget) {
      regressions.push({ file, rule, count, budget });
    }
  }
}

console.log(`design guard: ${files.length} Svelte files scanned`);
for (const rule of RULES) {
  console.log(`  ${rule.label.padEnd(28)} ${String(totals[rule.key]).padStart(4)}`);
}

if (regressions.length > 0) {
  console.error("\ndesign guard: new design-system violations");
  for (const { file, rule, count, budget } of regressions) {
    console.error(`  ${file}: ${rule.label} ${budget} -> ${count}`);
  }
  console.error(
    "\nUse design tokens instead: colors from src/lib/design/styles/tokens.css, " +
      "type sizes from the documented scale."
  );
  process.exit(1);
}

console.log("design guard: no regressions");
