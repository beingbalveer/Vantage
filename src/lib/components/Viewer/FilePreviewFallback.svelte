<script lang="ts">
  // Honest fallback for files the in-app viewer cannot render (binaries,
  // oversized blobs, undecodable images, PDFs, media, archives). Instead of a
  // blank body, show what the file is and hand it to the OS when asked.
  import Icon from "../common/Icon.svelte";
  import FileIcon from "../common/FileIcon.svelte";
  import { formatBytes } from "../../domain/diff-content";

  type FallbackKind = "binary" | "too-large" | "image-error" | "unsupported";

  interface Props {
    path: string;
    size?: number | null;
    kind?: FallbackKind;
    onopenexternal?: () => void;
  }

  let {
    path,
    size = null,
    kind = "unsupported",
    onopenexternal,
  }: Props = $props();

  const IMAGE_EXTENSIONS = new Set(["png", "jpg", "jpeg", "gif", "webp", "svg", "ico", "bmp", "avif"]);
  const VIDEO_EXTENSIONS = new Set(["mp4", "mov", "m4v", "webm", "mkv", "avi", "wmv", "flv"]);
  const AUDIO_EXTENSIONS = new Set(["mp3", "wav", "flac", "m4a", "aac", "ogg", "opus", "aiff"]);
  const ARCHIVE_EXTENSIONS = new Set(["zip", "tar", "gz", "tgz", "bz2", "xz", "rar", "7z", "dmg", "iso"]);
  const DOCUMENT_EXTENSIONS = new Set([
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "pages", "numbers", "key", "epub",
  ]);

  let fileName = $derived(path.split("/").pop() || path);

  let extension = $derived.by(() => {
    const lower = fileName.toLowerCase();
    const dot = lower.lastIndexOf(".");
    return dot > -1 ? lower.slice(dot + 1) : "";
  });

  let typeLabel = $derived.by(() => {
    if (kind === "image-error" || IMAGE_EXTENSIONS.has(extension)) return "Image";
    if (extension === "pdf") return "PDF document";
    if (VIDEO_EXTENSIONS.has(extension)) return "Video";
    if (AUDIO_EXTENSIONS.has(extension)) return "Audio";
    if (ARCHIVE_EXTENSIONS.has(extension)) return "Archive";
    if (DOCUMENT_EXTENSIONS.has(extension)) return "Document";
    if (kind === "too-large") return "Large file";
    if (kind === "binary") return "Binary file";
    return extension ? `${extension.toUpperCase()} file` : "File";
  });

  let description = $derived.by(() => {
    if (kind === "too-large") {
      return "This file is too large to preview in the app. Open it externally to inspect it.";
    }
    if (kind === "image-error") {
      return "This image could not be decoded in the app.";
    }
    if (kind === "binary") {
      if (
        DOCUMENT_EXTENSIONS.has(extension) ||
        VIDEO_EXTENSIONS.has(extension) ||
        AUDIO_EXTENSIONS.has(extension) ||
        ARCHIVE_EXTENSIONS.has(extension)
      ) {
        return "This file type can't be previewed in the app. Open it externally to inspect it.";
      }
      return "This file is binary, so it can't be shown as text.";
    }
    return "This file type can't be previewed in the app.";
  });

  let sizeLabel = $derived(size == null ? "" : formatBytes(size));
</script>

<div class="file-preview-fallback">
  <div class="fallback-card">
    <div class="fallback-icon" aria-hidden="true">
      <FileIcon filename={path} size={44} />
    </div>

    <div class="fallback-copy">
      <span class="fallback-name text-strong" title={path}>{fileName}</span>
      <span class="fallback-meta text-xs">
        {typeLabel}{sizeLabel ? ` · ${sizeLabel}` : ""}
      </span>
      <span class="fallback-desc text-sm">{description}</span>
    </div>

    {#if onopenexternal}
      <button
        type="button"
        class="fallback-open text-sm"
        onclick={onopenexternal}
        title="Open with default application"
        aria-label="Open {fileName} with the default application"
      >
        <Icon name="external-link" size={15} />
        <span>Open externally</span>
      </button>
    {/if}
  </div>
</div>

<style>
  .file-preview-fallback {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    width: 100%;
    padding: 40px 24px;
    overflow: auto;
  }

  .fallback-card {
    display: flex;
    align-items: center;
    gap: var(--s-4);
    max-width: 520px;
    width: 100%;
    padding: var(--s-5);
    background-color: var(--bg-subtle);
    border: 1px solid var(--border);
    border-radius: var(--r-lg);
    color: var(--text-muted);
  }

  .fallback-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    width: 64px;
    height: 64px;
    background-color: var(--bg);
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-md);
  }

  .fallback-copy {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
    flex: 1;
  }

  .fallback-name {
    color: var(--text);
    overflow-wrap: anywhere;
  }

  .fallback-meta {
    color: var(--text-muted);
  }

  .fallback-desc {
    color: var(--text-faint);
  }

  .fallback-open {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
    height: 28px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background-color: var(--bg);
    color: var(--text);
    cursor: pointer;
    transition: background-color 120ms ease, border-color 120ms ease;
  }

  .fallback-open:hover {
    background-color: var(--bg-muted);
    border-color: var(--accent);
    color: var(--accent);
  }

  .fallback-open:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  @media (max-width: 520px) {
    .fallback-card {
      flex-direction: column;
      text-align: center;
    }

    .fallback-copy {
      align-items: center;
    }
  }
</style>
