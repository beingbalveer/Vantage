import { describe, it, expect, beforeEach } from "vitest";
import {
  getAllFeatureFlags,
  isFeatureEnabled,
  setFeatureFlag,
  resetFeatureFlags,
  FEATURE_METADATA,
  type FeatureFlagKey,
} from "../src/lib/featureFlags.svelte";

describe("featureFlags system", () => {
  beforeEach(() => {
    resetFeatureFlags();
  });

  it("has metadata for all defined tasks", () => {
    expect(FEATURE_METADATA.length).toBeGreaterThanOrEqual(10);
    const keys = FEATURE_METADATA.map((m) => m.key);
    expect(keys).toContain("ALPH_144_MULTI_PANE");
    expect(keys).toContain("ALPH_156_CONFLICT_FLOW");
    expect(keys).toContain("ALPH_157_TAGS");
    expect(keys).toContain("ALPH_158_SPLIT_IMAGE_DIFF");
    expect(keys).toContain("ALPH_159_EXPAND_WHOLE_FILE");
    expect(keys).toContain("ALPH_160_LINE_PARTIAL_COMMIT");
    expect(keys).toContain("ALPH_161_SHORTCUT_REGISTRY");
    expect(keys).toContain("ALPH_162_SETTINGS_PARITY");
    expect(keys).toContain("ALPH_163_NATIVE_MENU");
    expect(keys).toContain("ALPH_164_ACCESSIBILITY");
    expect(keys).toContain("ALPH_165_BRANDING");
  });

  it("defaults all tasks to enabled", () => {
    for (const meta of FEATURE_METADATA) {
      expect(isFeatureEnabled(meta.key)).toBe(true);
    }
  });

  it("allows toggling a feature flag off and on", () => {
    const key: FeatureFlagKey = "ALPH_156_CONFLICT_FLOW";
    expect(isFeatureEnabled(key)).toBe(true);

    setFeatureFlag(key, false);
    expect(isFeatureEnabled(key)).toBe(false);
    expect(getAllFeatureFlags()[key]).toBe(false);

    setFeatureFlag(key, true);
    expect(isFeatureEnabled(key)).toBe(true);
  });

  it("resets all feature flags back to defaults", () => {
    setFeatureFlag("ALPH_144_MULTI_PANE", false);
    setFeatureFlag("ALPH_157_TAGS", false);
    expect(isFeatureEnabled("ALPH_144_MULTI_PANE")).toBe(false);
    expect(isFeatureEnabled("ALPH_157_TAGS")).toBe(false);

    resetFeatureFlags();
    expect(isFeatureEnabled("ALPH_144_MULTI_PANE")).toBe(true);
    expect(isFeatureEnabled("ALPH_157_TAGS")).toBe(true);
  });
});
