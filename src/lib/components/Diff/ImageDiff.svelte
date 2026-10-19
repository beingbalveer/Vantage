<script lang="ts">
  import Icon from "../common/Icon.svelte";
  import BinaryDiff from "./BinaryDiff.svelte";
  import { formatBytes, formatSizeChange } from "../../diff-content";

  interface Props {
    path: string;
    oldImage: string | null;
    newImage: string | null;
    oldSize: number | null;
    newSize: number | null;
    oldExists: boolean;
    newExists: boolean;
    oldTooLarge: boolean;
    newTooLarge: boolean;
    capBytes: number;
    status?: string;
    loading?: boolean;
    /** Metadata/blob reads failed; distinguish from oversized/undecodable. */
    error?: boolean;
  }

  let {
    path,
    oldImage,
    newImage,
    oldSize,
    newSize,
    oldExists,
    newExists,
    oldTooLarge,
    newTooLarge,
    capBytes,
    status = "modified",
    loading = false,
    error = false,
  }: Props = $props();

  let oldDims = $state<{ w: number; h: number } | null>(null);
  let newDims = $state<{ w: number; h: number } | null>(null);
  let oldBroken = $state(false);
  let newBroken = $state(false);

  // Reset decode state whenever the preview data changes.
  $effect(() => {
    void oldImage;
    void newImage;
    oldDims = null;
    newDims = null;
    oldBroken = false;
    newBroken = false;
  });

  let capLabel = $derived(formatBytes(capBytes));
  let oldShowable = $derived(!!oldImage && !oldBroken);
  let newShowable = $derived(!!newImage && !newBroken);
  let anyPreview = $derived(
    (oldExists && oldShowable) || (newExists && newShowable)
  );
  let identical = $derived(
    oldExists && newExists && !!oldImage && !!newImage && oldImage === newImage
  );
  let addedOnly = $derived(!oldExists && newExists);
  let removedOnly = $derived(oldExists && !newExists);

  let fallbackDetail = $derived(
    error
      ? "Couldn't read this file's metadata from Git."
      : oldTooLarge || newTooLarge
        ? `Preview is capped at ${capLabel} per side; oversized images are shown by size only.`
        : "This image could not be decoded in the app; sizes are from Git."
  );

  function onLoadOld(e: Event) {
    const img = e.currentTarget as HTMLImageElement;
    if (img.naturalWidth && img.naturalHeight) {
      oldDims = { w: img.naturalWidth, h: img.naturalHeight };
    }
  }
  function onLoadNew(e: Event) {
    const img = e.currentTarget as HTMLImageElement;
    if (img.naturalWidth && img.naturalHeight) {
      newDims = { w: img.naturalWidth, h: img.naturalHeight };
    }
  }
</script>

<div class="image-diff">
  <div class="image-toolbar text-xs">
    <span class="text-muted" title={path}>Image preview</span>
    <span class="text-faint">cap {capLabel} per side</span>
    {#if removedOnly}
      <span class="text-faint">removed</span>
    {:else if addedOnly}
      <span class="text-faint">added</span>
    {/if}
  </div>

  {#if loading}
    <div class="image-hint text-sm text-muted">Loading image previews…</div>
  {:else if identical}
    <div class="image-hint text-sm text-muted">
      <Icon name="check" size={14} strokeWidth={2} />
      Image content is identical on both sides ({formatSizeChange(oldSize, newSize, true, true)}).
    </div>
  {:else if !anyPreview}
    <!-- Oversized, undecodable, or unreadable: degrade to the honest binary state. -->
    <BinaryDiff
      {path}
      {oldSize}
      {newSize}
      {oldExists}
      {newExists}
      {status}
      subject="Image"
      detail={fallbackDetail}
    />
  {:else}
    <div class="image-panes" class:single={addedOnly || removedOnly}>
      {#if oldExists}
        <figure class="image-pane">
          <figcaption class="pane-caption text-xs">
            <span class="pane-side text-strong">Old</span>
            <span class="text-muted">{formatBytes(oldSize)}</span>
            {#if oldDims}
              <span class="text-faint">{oldDims.w} × {oldDims.h}</span>
            {/if}
          </figcaption>
          <div class="pane-canvas">
            {#if oldShowable}
              <img
                src={oldImage}
                alt="Old version of {path}"
                onload={onLoadOld}
                onerror={() => (oldBroken = true)}
              />
            {:else if oldTooLarge}
              <span class="pane-empty text-xs text-faint">Not loaded (over {capLabel})</span>
            {:else if oldBroken}
              <span class="pane-empty text-xs text-faint">Couldn't decode image</span>
            {:else}
              <span class="pane-empty text-xs text-faint">Preview unavailable</span>
            {/if}
          </div>
        </figure>
      {/if}

      {#if newExists}
        <figure class="image-pane">
          <figcaption class="pane-caption text-xs">
            <span class="pane-side text-strong">New</span>
            <span class="text-muted">{formatBytes(newSize)}</span>
            {#if newDims}
              <span class="text-faint">{newDims.w} × {newDims.h}</span>
            {/if}
          </figcaption>
          <div class="pane-canvas">
            {#if newShowable}
              <img
                src={newImage}
                alt="New version of {path}"
                onload={onLoadNew}
                onerror={() => (newBroken = true)}
              />
            {:else if newTooLarge}
              <span class="pane-empty text-xs text-faint">Not loaded (over {capLabel})</span>
            {:else if newBroken}
              <span class="pane-empty text-xs text-faint">Couldn't decode image</span>
            {:else}
              <span class="pane-empty text-xs text-faint">Preview unavailable</span>
            {/if}
          </div>
        </figure>
      {/if}
    </div>
  {/if}
</div>

<style>
  .image-diff {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
    padding: var(--s-3);
  }

  .image-toolbar {
    display: flex;
    align-items: center;
    gap: var(--s-2);
  }

  .image-panes {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: var(--s-2);
  }
  .image-panes.single {
    grid-template-columns: minmax(0, 1fr);
  }

  .image-pane {
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: var(--s-1);
    min-width: 0;
  }

  .pane-caption {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    padding: 0 var(--s-1);
  }

  .pane-side {
    color: var(--text);
  }

  .pane-canvas {
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 120px;
    padding: var(--s-3);
    background-color: var(--bg-inset);
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-md);
    overflow: hidden;
  }

  .pane-canvas img {
    max-width: 100%;
    max-height: 420px;
    object-fit: contain;
    display: block;
  }

  .pane-empty {
    text-align: center;
    padding: var(--s-4) 0;
  }

  .image-hint {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--s-2);
    padding: 40px var(--s-4);
    text-align: center;
    color: var(--text-muted);
  }
</style>
