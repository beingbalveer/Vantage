import { describe, it, expect } from "vitest";
import {
  classifyDiffContent,
  formatBytes,
  formatSizeChange,
  imageMime,
  isImagePath,
} from "../src/lib/domain/diff-content";

describe("diff content classification", () => {
  it("recognises previewable image extensions", () => {
    expect(imageMime("assets/logo.png")).toBe("image/png");
    expect(imageMime("photo.JPG")).toBe("image/jpeg");
    expect(imageMime("photo.jpeg")).toBe("image/jpeg");
    expect(imageMime("anim.gif")).toBe("image/gif");
    expect(imageMime("next.webp")).toBe("image/webp");
    expect(imageMime("icon.svg")).toBe("image/svg+xml");
    expect(isImagePath("docs/diagram.svg")).toBe(true);
  });

  it("rejects non-image paths and malformed extensions", () => {
    expect(imageMime("src/main.rs")).toBeNull();
    expect(imageMime("Makefile")).toBeNull();
    expect(imageMime("archive.tar.gz")).toBeNull();
    expect(imageMime(".gitignore")).toBeNull();
    expect(imageMime("weird.png/extra")).toBeNull();
    expect(imageMime("trailingdot.")).toBeNull();
    expect(isImagePath("README.md")).toBe(false);
  });

  it("treats images as images even when git flags them binary", () => {
    // A PNG is binary to git, but we can preview it.
    expect(classifyDiffContent("assets/logo.png", true)).toBe("image");
    // SVG is text to git, but still previewable as an image.
    expect(classifyDiffContent("assets/logo.svg", false)).toBe("image");
  });

  it("uses git's binary flag only for non-images", () => {
    expect(classifyDiffContent("src/main.rs", false)).toBe("text");
    expect(classifyDiffContent("build/app.bin", true)).toBe("binary");
  });
});

describe("diff size formatting", () => {
  it("formats byte sizes with binary units", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1023)).toBe("1023 B");
    expect(formatBytes(1024)).toBe("1 KB");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(10 * 1024)).toBe("10 KB");
    expect(formatBytes(1024 * 1024)).toBe("1 MB");
    expect(formatBytes(1_500_000)).toBe("1.4 MB");
    expect(formatBytes(4 * 1024 * 1024)).toBe("4 MB");
  });

  it("returns unknown for missing or invalid sizes", () => {
    expect(formatBytes(null)).toBe("unknown");
    expect(formatBytes(undefined)).toBe("unknown");
    expect(formatBytes(-1)).toBe("unknown");
    expect(formatBytes(Number.NaN)).toBe("unknown");
  });

  it("collapses added and removed files to one side", () => {
    expect(formatSizeChange(100, 200, true, true)).toBe("100 B → 200 B");
    expect(formatSizeChange(null, 2048, false, true)).toBe("2 KB");
    expect(formatSizeChange(4096, null, true, false)).toBe("4 KB");
    expect(formatSizeChange(null, null, true, true)).toBe("unknown → unknown");
  });
});
