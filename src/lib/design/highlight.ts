import hljs from "highlight.js/lib/common";
import dart from "highlight.js/lib/languages/dart";
import elixir from "highlight.js/lib/languages/elixir";
import erlang from "highlight.js/lib/languages/erlang";
import haskell from "highlight.js/lib/languages/haskell";
import clojure from "highlight.js/lib/languages/clojure";
import fsharp from "highlight.js/lib/languages/fsharp";
import powershell from "highlight.js/lib/languages/powershell";
import dockerfile from "highlight.js/lib/languages/dockerfile";
import nginx from "highlight.js/lib/languages/nginx";
import protobuf from "highlight.js/lib/languages/protobuf";
import scala from "highlight.js/lib/languages/scala";
import julia from "highlight.js/lib/languages/julia";
import groovy from "highlight.js/lib/languages/groovy";
import cmake from "highlight.js/lib/languages/cmake";
import coffeescript from "highlight.js/lib/languages/coffeescript";
import http from "highlight.js/lib/languages/http";
import ocaml from "highlight.js/lib/languages/ocaml";
import glsl from "highlight.js/lib/languages/glsl";
import handlebars from "highlight.js/lib/languages/handlebars";

// `lib/common` already registers ~36 languages; these are the extra grammars
// that cover the rest of a typical multi-repo workspace.
const EXTRA_LANGUAGES = {
  dart,
  elixir,
  erlang,
  haskell,
  clojure,
  fsharp,
  powershell,
  dockerfile,
  nginx,
  protobuf,
  scala,
  julia,
  groovy,
  cmake,
  coffeescript,
  http,
  ocaml,
  glsl,
  handlebars,
};

for (const [name, definition] of Object.entries(EXTRA_LANGUAGES)) {
  if (!hljs.getLanguage(name)) {
    hljs.registerLanguage(name, definition as never);
  }
}

/** Files whose name alone identifies the grammar (no useful extension). */
const FILENAME_LANGUAGES: Record<string, string> = {
  dockerfile: "dockerfile",
  containerfile: "dockerfile",
  makefile: "makefile",
  gnumakefile: "makefile",
  "cmakelists.txt": "cmake",
  jenkinsfile: "groovy",
  gemfile: "ruby",
  rakefile: "ruby",
  vagrantfile: "ruby",
  brewfile: "ruby",
  podfile: "ruby",
  procfile: "plaintext",
  "go.mod": "go",
  "go.sum": "plaintext",
  ".gitignore": "plaintext",
  ".gitattributes": "plaintext",
  ".dockerignore": "plaintext",
  ".npmignore": "plaintext",
  ".eslintignore": "plaintext",
  ".prettierignore": "plaintext",
  ".editorconfig": "ini",
  ".gitconfig": "ini",
  ".gitmodules": "ini",
  ".bashrc": "bash",
  ".bash_profile": "bash",
  ".zshrc": "bash",
  ".profile": "bash",
  ".env": "ini",
  "nginx.conf": "nginx",
  "httpd.conf": "apache",
  ".htaccess": "apache",
};

/** Extension → grammar. Anything not listed falls back to highlight.js aliases. */
const EXTENSION_LANGUAGES: Record<string, string> = {
  js: "javascript",
  jsx: "javascript",
  mjs: "javascript",
  cjs: "javascript",
  ts: "typescript",
  tsx: "typescript",
  mts: "typescript",
  cts: "typescript",
  svelte: "xml",
  vue: "xml",
  astro: "xml",
  html: "xml",
  htm: "xml",
  xhtml: "xml",
  xml: "xml",
  svg: "xml",
  plist: "xml",
  xsl: "xml",
  rss: "xml",
  atom: "xml",
  rs: "rust",
  py: "python",
  pyw: "python",
  pyi: "python",
  go: "go",
  rb: "ruby",
  rake: "ruby",
  gemspec: "ruby",
  ru: "ruby",
  php: "php",
  php3: "php",
  php4: "php",
  php5: "php",
  php7: "php",
  php8: "php",
  phtml: "php",
  java: "java",
  kt: "kotlin",
  kts: "kotlin",
  swift: "swift",
  dart: "dart",
  cs: "csharp",
  fs: "fsharp",
  fsi: "fsharp",
  fsx: "fsharp",
  vb: "vbnet",
  c: "c",
  h: "c",
  cc: "cpp",
  cpp: "cpp",
  cxx: "cpp",
  hpp: "cpp",
  hh: "cpp",
  hxx: "cpp",
  m: "objectivec",
  mm: "objectivec",
  scala: "scala",
  sc: "scala",
  clj: "clojure",
  cljs: "clojure",
  cljc: "clojure",
  edn: "clojure",
  ex: "elixir",
  exs: "elixir",
  erl: "erlang",
  hrl: "erlang",
  hs: "haskell",
  lhs: "haskell",
  lua: "lua",
  pl: "perl",
  pm: "perl",
  r: "r",
  jl: "julia",
  groovy: "groovy",
  gradle: "groovy",
  sh: "bash",
  bash: "bash",
  zsh: "bash",
  fish: "bash",
  ksh: "bash",
  ps1: "powershell",
  psm1: "powershell",
  psd1: "powershell",
  sql: "sql",
  yaml: "yaml",
  yml: "yaml",
  toml: "ini",
  ini: "ini",
  cfg: "ini",
  conf: "ini",
  properties: "ini",
  env: "ini",
  json: "json",
  jsonc: "json",
  json5: "json",
  webmanifest: "json",
  md: "markdown",
  markdown: "markdown",
  mdx: "markdown",
  diff: "diff",
  patch: "diff",
  graphql: "graphql",
  gql: "graphql",
  proto: "protobuf",
  proto3: "protobuf",
  tf: "ini",
  tfvars: "ini",
  hcl: "ini",
  http: "http",
  rest: "http",
  wasm: "wasm",
  wat: "wasm",
  cmake: "cmake",
  mk: "makefile",
  coffee: "coffeescript",
  ml: "ocaml",
  mli: "ocaml",
  glsl: "glsl",
  vert: "glsl",
  frag: "glsl",
  hbs: "handlebars",
  handlebars: "handlebars",
  mustache: "handlebars",
  csv: "plaintext",
  tsv: "plaintext",
  log: "plaintext",
  txt: "plaintext",
  lock: "plaintext",
};

/**
 * Resolve the grammar for a path. Tries the exact filename first (Dockerfile,
 * Makefile, …), then compound extensions (`.d.ts`), then the extension table,
 * and finally lets highlight.js match its own aliases.
 */
export function getLanguageFromPath(path: string): string | null {
  if (!path) return null;
  const lower = path.toLowerCase();
  const name = lower.split("/").pop() ?? lower;

  const byName = FILENAME_LANGUAGES[name] ?? FILENAME_LANGUAGES[name.replace(/\..*$/, "")];
  if (byName) return byName;
  if (name.startsWith("dockerfile")) return "dockerfile";
  if (name.startsWith("makefile")) return "makefile";

  if (lower.endsWith(".d.ts") || lower.endsWith(".d.mts") || lower.endsWith(".d.cts")) {
    return "typescript";
  }
  if (lower.endsWith(".blade.php")) return "php";
  if (lower.endsWith(".min.js")) return "javascript";
  if (lower.endsWith(".min.css")) return "css";

  const dot = name.lastIndexOf(".");
  const ext = dot > -1 ? name.slice(dot + 1) : "";
  const byExt = EXTENSION_LANGUAGES[ext];
  if (byExt) return byExt;
  if (ext && hljs.getLanguage(ext)) return ext;
  return null;
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
