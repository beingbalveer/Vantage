<script lang="ts">
  import Icon from "./Icon.svelte";

  interface Props {
    filename: string;
    isDir?: boolean;
    isOpen?: boolean;
    size?: number;
  }

  let {
    filename,
    isDir = false,
    isOpen = false,
    size = 14,
  }: Props = $props();

  interface IconStyle {
    kind: "folder" | "badge" | "icon";
    text?: string;
    iconName?: any;
    color: string;
    textColor?: string;
  }

  let iconStyle = $derived.by<IconStyle>(() => {
    if (isDir) {
      return {
        kind: "folder",
        color: isOpen ? "#e5a93b" : "#dcb67a",
      };
    }

    const lower = filename.toLowerCase();

    // Special exact files
    if (lower === ".gitignore" || lower === ".gitattributes" || lower === ".gitmodules") {
      return { kind: "badge", text: "GIT", color: "#f05032", textColor: "#ffffff" };
    }
    if (lower === "cargo.lock" || lower === "package-lock.json" || lower === "pnpm-lock.yaml" || lower.endsWith(".lock")) {
      return { kind: "icon", iconName: "key", color: "#8a99a8" };
    }
    if (lower.startsWith(".env")) {
      return { kind: "icon", iconName: "key", color: "#ecc94b" };
    }
    if (lower === "cargo.toml") {
      return { kind: "badge", text: "RS", color: "#d97706", textColor: "#ffffff" };
    }
    if (lower === "package.json") {
      return { kind: "badge", text: "{}", color: "#48bb78", textColor: "#ffffff" };
    }
    if (lower === "readme.md" || lower.endsWith(".md") || lower.endsWith(".markdown") || lower.endsWith(".mdx")) {
      return { kind: "badge", text: "M↓", color: "#4299e1", textColor: "#ffffff" };
    }

    const ext = lower.includes(".") ? lower.split(".").pop() || "" : "";

    switch (ext) {
      case "svelte":
        return { kind: "badge", text: "S", color: "#ff3e00", textColor: "#ffffff" };
      case "ts":
      case "mts":
      case "cts":
        return { kind: "badge", text: "TS", color: "#3178c6", textColor: "#ffffff" };
      case "tsx":
        return { kind: "badge", text: "TSX", color: "#2b7489", textColor: "#ffffff" };
      case "js":
      case "mjs":
      case "cjs":
        return { kind: "badge", text: "JS", color: "#f7df1e", textColor: "#1a1a1a" };
      case "jsx":
        return { kind: "badge", text: "JSX", color: "#f7df1e", textColor: "#1a1a1a" };
      case "rs":
        return { kind: "badge", text: "RS", color: "#e65100", textColor: "#ffffff" };
      case "json":
      case "jsonc":
        return { kind: "badge", text: "{}", color: "#ecc94b", textColor: "#1a1a1a" };
      case "css":
      case "scss":
      case "sass":
      case "less":
        return { kind: "badge", text: "#", color: "#38bdf8", textColor: "#ffffff" };
      case "html":
      case "htm":
        return { kind: "badge", text: "<>", color: "#f97316", textColor: "#ffffff" };
      case "toml":
      case "yaml":
      case "yml":
        return { kind: "badge", text: "⚙", color: "#a855f7", textColor: "#ffffff" };
      case "png":
      case "jpg":
      case "jpeg":
      case "gif":
      case "svg":
      case "ico":
      case "webp":
        return { kind: "icon", iconName: "image", color: "#10b981" };
      case "sh":
      case "bash":
      case "zsh":
        return { kind: "badge", text: "$_", color: "#22c55e", textColor: "#ffffff" };
      case "py":
        return { kind: "badge", text: "PY", color: "#3b82f6", textColor: "#ffffff" };
      case "go":
        return { kind: "badge", text: "GO", color: "#00add8", textColor: "#ffffff" };
      case "c":
      case "h":
        return { kind: "badge", text: "C", color: "#6366f1", textColor: "#ffffff" };
      case "cpp":
      case "hpp":
      case "cc":
        return { kind: "badge", text: "C++", color: "#6366f1", textColor: "#ffffff" };
      default:
        return { kind: "icon", iconName: "file-text", color: "currentColor" };
    }
  });
</script>

<span class="file-icon-wrapper" style="width: {size}px; height: {size}px;">
  {#if iconStyle.kind === "folder"}
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill={iconStyle.color}
      stroke={iconStyle.color}
      stroke-width="1.5"
      stroke-linecap="round"
      stroke-linejoin="round"
      class="folder-svg"
      aria-hidden="true"
    >
      {#if isOpen}
        <!-- Open folder -->
        <path d="M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.93a2 2 0 0 1-1.66-.9l-.82-1.2A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z" fill-opacity="0.3" />
        <path d="m2 10 3 10h16l3-10z" fill={iconStyle.color} />
      {:else}
        <!-- Closed folder -->
        <path d="M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.93a2 2 0 0 1-1.66-.9l-.82-1.2A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z" fill-opacity="0.75" />
      {/if}
    </svg>
  {:else if iconStyle.kind === "badge"}
    <span
      class="type-badge"
      style="background: {iconStyle.color}; color: {iconStyle.textColor}; font-size: {size <= 14 ? '9px' : '10px'};"
    >
      {iconStyle.text}
    </span>
  {:else if iconStyle.iconName === "image"}
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke={iconStyle.color}
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      <rect width="18" height="18" x="3" y="3" rx="2" ry="2" />
      <circle cx="9" cy="9" r="2" />
      <path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21" />
    </svg>
  {:else}
    <Icon name={iconStyle.iconName || "file-text"} size={size} />
  {/if}
</span>

<style>
  .file-icon-wrapper {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    line-height: 1;
    user-select: none;
  }

  .folder-svg {
    filter: drop-shadow(0 0.5px 0.5px rgba(0, 0, 0, 0.1));
  }

  .type-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 13px;
    height: 13px;
    padding: 0 2px;
    border-radius: 3px;
    font-weight: 700;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    line-height: 1;
    letter-spacing: -0.5px;
    text-transform: uppercase;
  }
</style>
