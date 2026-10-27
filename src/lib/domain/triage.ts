import type { FlagKind, Severity, TriageFlag } from "../platform/types";

// Human-readable rule names, matching the heuristic in src-tauri/src/triage.rs.
const FLAG_LABELS: Record<FlagKind, string> = {
  large: "Large change",
  deletionHeavy: "Deletion-heavy",
  dependencies: "Dependency change",
  config: "Config or CI change",
  testsRemoved: "Tests removed",
  secrets: "Secrets",
  bigFile: "Large file",
  binary: "Binary file",
};

export function flagLabel(kind: FlagKind): string {
  return FLAG_LABELS[kind] ?? "Flag";
}

// Order so the most urgent rule surfaces first when a row can only show a few
// icons (DESIGN.md §5.3): warn > caution > info, stable within a severity.
export function sortFlagsBySeverity(flags: TriageFlag[]): TriageFlag[] {
  const rank: Record<Severity, number> = { warn: 0, caution: 1, info: 2 };
  return [...flags].sort((a, b) => rank[a.severity] - rank[b.severity]);
}

// Multi-line tooltip: every rule with its backend detail.
export function formatFlagTooltip(flags: TriageFlag[]): string {
  return flags
    .map((f) => {
      const detail = (f.detail ?? "").trim();
      return detail ? `${flagLabel(f.kind)}: ${detail}` : flagLabel(f.kind);
    })
    .join("\n");
}

// Single-line label for screen readers: "3 flags: Possible secret, Tests removed".
export function formatFlagAriaLabel(flags: TriageFlag[]): string {
  if (flags.length === 0) return "No triage flags";
  const names = flags.map((f) => flagLabel(f.kind)).join(", ");
  return `${flags.length} ${flags.length === 1 ? "flag" : "flags"}: ${names}`;
}
