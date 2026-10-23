import { describe, it, expect } from "vitest";
import type { TriageFlag } from "../src/lib/types";
import {
  flagLabel,
  formatFlagAriaLabel,
  formatFlagTooltip,
  sortFlagsBySeverity,
} from "../src/lib/triage";

const flag = (kind: TriageFlag["kind"], severity: TriageFlag["severity"], detail: string): TriageFlag => ({
  kind,
  severity,
  detail,
});

describe("triage flag presentation", () => {
  it("labels every known flag kind", () => {
    expect(flagLabel("secrets")).toBe("Secrets");
    expect(flagLabel("testsRemoved")).toBe("Tests removed");
    expect(flagLabel("dependencies")).toBe("Dependency change");
  });

  it("sorts flags by severity with warn first", () => {
    const sorted = sortFlagsBySeverity([
      flag("large", "caution", "20 files"),
      flag("binary", "info", "1 binary file"),
      flag("secrets", "warn", "Possible secret: ghp_…"),
    ]);
    expect(sorted.map((f) => f.severity)).toEqual(["warn", "caution", "info"]);
    expect(sorted.map((f) => f.kind)).toEqual(["secrets", "large", "binary"]);
  });

  it("is stable within a severity", () => {
    const sorted = sortFlagsBySeverity([
      flag("large", "caution", "a"),
      flag("config", "caution", "b"),
    ]);
    expect(sorted.map((f) => f.kind)).toEqual(["large", "config"]);
  });

  it("does not mutate the input array", () => {
    const input = [flag("binary", "info", "x"), flag("secrets", "warn", "y")];
    sortFlagsBySeverity(input);
    expect(input.map((f) => f.kind)).toEqual(["binary", "secrets"]);
  });

  it("builds a multi-line tooltip with rule names and details", () => {
    const tip = formatFlagTooltip([
      flag("secrets", "warn", "Possible secret: ghp_123"),
      flag("testsRemoved", "warn", "2 test files deleted"),
    ]);
    expect(tip).toBe("Secrets: Possible secret: ghp_123\nTests removed: 2 test files deleted");
  });

  it("falls back to the bare label when detail is empty", () => {
    expect(formatFlagTooltip([flag("config", "caution", "  ")])).toBe("Config or CI change");
  });

  it("summarizes the flag count and names for screen readers", () => {
    expect(formatFlagAriaLabel([])).toBe("No triage flags");
    expect(formatFlagAriaLabel([flag("secrets", "warn", "x")])).toBe("1 flag: Secrets");
    expect(
      formatFlagAriaLabel([flag("secrets", "warn", "x"), flag("large", "caution", "y")])
    ).toBe("2 flags: Secrets, Large change");
  });
});
