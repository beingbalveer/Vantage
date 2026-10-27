<script lang="ts">
  // Monochrome file glyphs for the explorer and file tabs. Colour is carried
  // by the row (currentColor); the shape alone says what kind of file it is,
  // which keeps the chrome quiet the way Zed's does.
  import Icon from "./Icon.svelte";
  import type { IconName } from "./iconName";

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

  const CODE_EXTENSIONS = new Set([
    "ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs",
    "svelte", "vue", "astro", "rs", "go", "py", "rb", "php",
    "c", "h", "cc", "cpp", "hpp", "cs", "java", "kt", "swift",
    "sh", "bash", "zsh", "fish", "css", "scss", "sass", "less",
    "html", "htm", "xml", "json", "jsonc", "toml", "yaml", "yml",
    "sql", "graphql", "proto",
  ]);

  const TEXT_EXTENSIONS = new Set(["md", "markdown", "mdx", "txt", "rst", "adoc"]);

  const IMAGE_EXTENSIONS = new Set(["png", "jpg", "jpeg", "gif", "svg", "ico", "webp", "avif", "bmp"]);

  let extension = $derived.by(() => {
    const lower = filename.toLowerCase();
    const dot = lower.lastIndexOf(".");
    return dot > -1 ? lower.slice(dot + 1) : "";
  });

  let glyph = $derived.by<IconName>(() => {
    if (CODE_EXTENSIONS.has(extension)) return "code";
    if (TEXT_EXTENSIONS.has(extension)) return "file-text";
    if (IMAGE_EXTENSIONS.has(extension)) return "image";
    return "file";
  });
</script>

<span class="file-icon-wrapper" style="width: {size}px; height: {size}px;">
  {#if isDir}
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill={isOpen ? "currentColor" : "none"}
      fill-opacity={isOpen ? 0.18 : 0}
      stroke="currentColor"
      stroke-width="1.7"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      <path
        d="M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.93a2 2 0 0 1-1.66-.9l-.82-1.2A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z"
      />
    </svg>
  {:else}
    <Icon name={glyph} size={size} />
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

  /* Glyphs sit a step quieter than their label, like Zed's tree. */
  .file-icon-wrapper :global(svg) {
    display: block;
    opacity: 0.62;
  }
</style>
