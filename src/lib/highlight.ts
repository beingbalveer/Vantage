import hljs from "highlight.js/lib/core";
import javascript from "highlight.js/lib/languages/javascript";
import typescript from "highlight.js/lib/languages/typescript";
import rust from "highlight.js/lib/languages/rust";
import python from "highlight.js/lib/languages/python";
import go from "highlight.js/lib/languages/go";
import json from "highlight.js/lib/languages/json";
import xml from "highlight.js/lib/languages/xml";
import css from "highlight.js/lib/languages/css";
import markdown from "highlight.js/lib/languages/markdown";
import bash from "highlight.js/lib/languages/bash";
import yaml from "highlight.js/lib/languages/yaml";

// Register languages
hljs.registerLanguage("javascript", javascript);
hljs.registerLanguage("js", javascript);
hljs.registerLanguage("typescript", typescript);
hljs.registerLanguage("ts", typescript);
hljs.registerLanguage("rust", rust);
hljs.registerLanguage("rs", rust);
hljs.registerLanguage("python", python);
hljs.registerLanguage("py", python);
hljs.registerLanguage("go", go);
hljs.registerLanguage("json", json);
hljs.registerLanguage("xml", xml);
hljs.registerLanguage("html", xml);
hljs.registerLanguage("css", css);
hljs.registerLanguage("markdown", markdown);
hljs.registerLanguage("md", markdown);
hljs.registerLanguage("bash", bash);
hljs.registerLanguage("sh", bash);
hljs.registerLanguage("yaml", yaml);
hljs.registerLanguage("yml", yaml);

export function getLanguageFromPath(path: string): string | null {
  const ext = path.split(".").pop()?.toLowerCase();
  switch (ext) {
    case "js":
    case "jsx":
    case "mjs":
    case "cjs":
      return "javascript";
    case "ts":
    case "tsx":
    case "mts":
      return "typescript";
    case "rs":
      return "rust";
    case "py":
      return "python";
    case "go":
      return "go";
    case "json":
      return "json";
    case "html":
    case "xml":
    case "svg":
      return "xml";
    case "css":
    case "scss":
      return "css";
    case "md":
    case "markdown":
      return "markdown";
    case "sh":
    case "bash":
    case "zsh":
      return "bash";
    case "yaml":
    case "yml":
      return "yaml";
    default:
      return null;
  }
}

/**
 * Highlights text and splits into lines, ensuring open spans are correctly
 * closed and re-opened across line breaks for virtualization.
 */
export function highlightCodeLines(code: string, language: string | null): string[] {
  // Budget rules: skip if code > 1MB or has lines > 2000 chars
  if (code.length > 1_000_000) {
    return code.split("\n").map(escapeHtml);
  }

  const rawLines = code.split("\n");
  if (rawLines.some((l) => l.length > 2000)) {
    return rawLines.map(escapeHtml);
  }

  if (!language || !hljs.getLanguage(language)) {
    return rawLines.map(escapeHtml);
  }

  try {
    const highlighted = hljs.highlight(code, { language, ignoreIllegals: true }).value;
    return splitHighlightedLines(highlighted);
  } catch {
    return rawLines.map(escapeHtml);
  }
}

function escapeHtml(str: string): string {
  return str
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#039;");
}

function splitHighlightedLines(highlightedHtml: string): string[] {
  const lines: string[] = [];
  const rawLines = highlightedHtml.split("\n");
  const openTags: string[] = [];

  for (const line of rawLines) {
    const prefix = openTags.join("");
    let currentLine = prefix + line;

    // Track tags opened and closed in this line
    const tagRegex = /<\/?([a-zA-Z0-9_-]+)(?:[^>]*)>/g;
    let match;
    while ((match = tagRegex.exec(line)) !== null) {
      const fullTag = match[0];
      const isClosing = fullTag.startsWith("</");
      if (isClosing) {
        openTags.pop();
      } else {
        openTags.push(fullTag);
      }
    }

    const suffix = openTags.slice().reverse().map((t) => `</${t.match(/<([a-zA-Z0-9_-]+)/)?.[1]}>`).join("");
    currentLine += suffix;
    lines.push(currentLine);
  }

  return lines;
}
