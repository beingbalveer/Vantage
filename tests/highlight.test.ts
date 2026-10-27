import { describe, expect, it } from "vitest";
import { getLanguageFromPath, highlightCodeLines } from "../src/lib/design/highlight";

describe("getLanguageFromPath", () => {
  it("maps common extensions", () => {
    expect(getLanguageFromPath("src/app.ts")).toBe("typescript");
    expect(getLanguageFromPath("src/app.tsx")).toBe("typescript");
    expect(getLanguageFromPath("src/lib.rs")).toBe("rust");
    expect(getLanguageFromPath("scripts/deploy.sh")).toBe("bash");
    expect(getLanguageFromPath("config/app.yaml")).toBe("yaml");
    expect(getLanguageFromPath("Cargo.toml")).toBe("ini");
    expect(getLanguageFromPath("web/index.html")).toBe("xml");
    expect(getLanguageFromPath("web/App.svelte")).toBe("xml");
  });

  it("recognises file names without a useful extension", () => {
    expect(getLanguageFromPath("Dockerfile")).toBe("dockerfile");
    expect(getLanguageFromPath("services/api/Dockerfile.dev")).toBe("dockerfile");
    expect(getLanguageFromPath("Makefile")).toBe("makefile");
    expect(getLanguageFromPath("CMakeLists.txt")).toBe("cmake");
    expect(getLanguageFromPath("Jenkinsfile")).toBe("groovy");
    expect(getLanguageFromPath(".env")).toBe("ini");
    expect(getLanguageFromPath("nginx.conf")).toBe("nginx");
  });

  it("handles compound extensions", () => {
    expect(getLanguageFromPath("src/types.d.ts")).toBe("typescript");
    expect(getLanguageFromPath("views/form.blade.php")).toBe("php");
  });

  it("covers languages outside lib/common", () => {
    expect(getLanguageFromPath("main.dart")).toBe("dart");
    expect(getLanguageFromPath("app.ex")).toBe("elixir");
    expect(getLanguageFromPath("build.gradle")).toBe("groovy");
    expect(getLanguageFromPath("schema.proto")).toBe("protobuf");
    expect(getLanguageFromPath("deploy.ps1")).toBe("powershell");
  });

  it("returns null for unknown or extension-less files", () => {
    expect(getLanguageFromPath("LICENSE")).toBeNull();
    expect(getLanguageFromPath("assets/logo.bin")).toBeNull();
    expect(getLanguageFromPath("")).toBeNull();
  });
});

describe("highlightCodeLines", () => {
  it("emits highlight.js spans for supported languages", () => {
    const [line] = highlightCodeLines("const answer = 42;", "typescript");
    expect(line).toContain("hljs-");
  });

  it("highlights the extra grammars, not just the common bundle", () => {
    const dart = highlightCodeLines("void main() { print('hi'); }", "dart").join("\n");
    expect(dart).toContain("hljs-");

    const php = highlightCodeLines("<?php echo $name; ?>", "php").join("\n");
    expect(php).toContain("hljs-");

    const docker = highlightCodeLines("FROM alpine:3", "dockerfile").join("\n");
    expect(docker).toContain("hljs-");

    const scala = highlightCodeLines("object Main extends App { println(1) }", "scala").join("\n");
    expect(scala).toContain("hljs-");
  });

  it("falls back to escaped text for unknown languages", () => {
    const [line] = highlightCodeLines("<script>alert(1)</script>", null);
    expect(line).toContain("&lt;script&gt;");
    expect(line).not.toContain("<script>");
  });

  it("splits highlighted output per line", () => {
    const lines = highlightCodeLines("const a = 1;\nconst b = 2;", "typescript");
    expect(lines).toHaveLength(2);
    expect(lines[0]).toContain("hljs-keyword");
    expect(lines[1]).toContain("hljs-keyword");
  });
});
