#!/usr/bin/env node
/**
 * Layer dependency guard.
 *
 * `src/lib/README.md` defines a one-way dependency rule for the frontend
 * layers. This script turns that contract into a CI check: every import that
 * points from a higher layer to a lower one is fine, every upward edge fails.
 *
 * It also keeps the platform boundary honest: only `platform/` may import
 * `@tauri-apps/*` at runtime. Components may import *types* from those packages
 * (e.g. `import type { UnlistenFn } from "@tauri-apps/api/event"`).
 *
 * Run: `npm run lint:layers`
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, extname, join, relative, resolve, sep } from "node:path";

const ROOT = process.cwd();
const SRC = join(ROOT, "src");
const LIB = join(SRC, "lib");

/** Low -> high. A layer may import itself and everything before it. */
const LAYERS = ["util", "config", "design", "platform", "domain", "app", "components"];
const ROOT_LAYER = "root"; // src/App.svelte + src/main.ts (composition root)

const SOURCE_EXTENSIONS = new Set([".ts", ".js", ".mjs", ".svelte", ".css"]);
const RESOLVE_EXTENSIONS = [".ts", ".js", ".mjs", ".svelte", ".css", ".json"];

const IMPORT_RE =
  /\bfrom\s*["'](?<from>[^"']+)["']|\bimport\s*\(\s*["'](?<dynamic>[^"']+)["']\s*\)|^\s*import\s*["'](?<side>[^"']+)["']/gm;

function walk(dir) {
  const files = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      files.push(...walk(full));
    } else if (SOURCE_EXTENSIONS.has(extname(full)) && !full.endsWith(".d.ts")) {
      files.push(full);
    }
  }
  return files;
}

function layerOf(path) {
  const rel = relative(LIB, path);
  if (rel.startsWith("..") || rel === "") return ROOT_LAYER;
  const [first] = rel.split(sep);
  return LAYERS.includes(first) ? first : ROOT_LAYER;
}

function rank(layer) {
  return layer === ROOT_LAYER ? LAYERS.length : LAYERS.indexOf(layer);
}

function resolveImport(spec, importer) {
  const base = resolve(dirname(importer), spec);
  const candidates = [
    base,
    ...RESOLVE_EXTENSIONS.map((ext) => base + ext),
    ...RESOLVE_EXTENSIONS.map((ext) => join(base, `index${ext}`)),
  ];
  for (const candidate of candidates) {
    try {
      if (statSync(candidate).isFile()) return candidate;
    } catch {
      // Candidate does not exist; keep looking.
    }
  }
  return null;
}

function isTypeOnlyImport(text, index) {
  const start = text.lastIndexOf("import", index);
  if (start === -1) return false;
  return /^import\s+type\b/.test(text.slice(start, index + 6));
}

const files = walk(SRC);
const violations = [];

for (const file of files) {
  const text = readFileSync(file, "utf8");
  const importerLayer = layerOf(file);
  const relFile = relative(ROOT, file);

  for (const match of text.matchAll(IMPORT_RE)) {
    const spec = match.groups.from ?? match.groups.dynamic ?? match.groups.side;
    if (!spec) continue;

    if (spec.startsWith("@tauri-apps/")) {
      if (importerLayer !== "platform" && !isTypeOnlyImport(text, match.index ?? 0)) {
        violations.push({
          file: relFile,
          importerLayer,
          spec,
          targetLayer: "external (@tauri-apps)",
          reason: "runtime Tauri imports belong in platform/",
        });
      }
      continue;
    }

    if (!spec.startsWith(".")) continue;

    const target = resolveImport(spec, file);
    if (!target) {
      violations.push({
        file: relFile,
        importerLayer,
        spec,
        targetLayer: "unresolved",
        reason: "cannot resolve relative import",
      });
      continue;
    }

    const targetLayer = layerOf(target);
    if (rank(targetLayer) > rank(importerLayer)) {
      violations.push({
        file: relFile,
        importerLayer,
        spec,
        targetLayer,
        reason: "upward layer import",
        target: relative(ROOT, target),
      });
    }
  }
}

if (violations.length === 0) {
  console.log(`layer guard: ${files.length} files checked, no upward edges`);
  process.exit(0);
}

console.error(`layer guard: ${violations.length} violation(s) in ${files.length} files\n`);
for (const violation of violations) {
  const arrow = `${violation.importerLayer} -> ${violation.targetLayer}`;
  console.error(`  ${violation.file}: ${violation.spec}  [${arrow}]`);
  console.error(`    ${violation.reason}`);
}
console.error(
  "\nLayers (low -> high): " +
    LAYERS.join(" -> ") +
    ". See src/lib/README.md for what belongs where."
);
process.exit(1);
