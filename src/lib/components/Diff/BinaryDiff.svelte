<script lang="ts">
  import Icon from "../common/Icon.svelte";
  import { formatSizeChange } from "../../domain/diff-content";

  interface Props {
    path: string;
    oldSize: number | null;
    newSize: number | null;
    oldExists: boolean;
    newExists: boolean;
    /** "added" | "deleted" | "modified" | ... drives the wording. */
    status?: string;
    loading?: boolean;
    /** Noun used in the headline, e.g. "Binary file" or "Image". */
    subject?: string;
    /** Optional explanatory line under the sizes. */
    detail?: string;
  }

  let {
    path,
    oldSize,
    newSize,
    oldExists,
    newExists,
    status = "modified",
    loading = false,
    subject = "Binary file",
    detail = "Contents aren't previewable, so none were loaded.",
  }: Props = $props();

  let sizeText = $derived(formatSizeChange(oldSize, newSize, oldExists, newExists));

  let verb = $derived(
    status === "added" || (!oldExists && newExists)
      ? `${subject} added`
      : status === "deleted" || (oldExists && !newExists)
        ? `${subject} removed`
        : `${subject} changed`
  );
</script>

<div class="binary-diff">
  <div class="binary-card">
    <Icon name="binary" size={20} strokeWidth={1.6} />
    <div class="binary-text">
      <span class="binary-title text-strong">{verb}</span>
      {#if loading}
        <span class="binary-detail text-xs text-faint">Reading sizes…</span>
      {:else}
        <span class="binary-detail text-sm text-muted" title={path}>{sizeText}</span>
        {#if detail}
          <span class="binary-detail text-xs text-faint">{detail}</span>
        {/if}
      {/if}
    </div>
  </div>
</div>

<style>
  .binary-diff {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 40px 16px;
  }

  .binary-card {
    display: flex;
    align-items: center;
    gap: var(--s-3);
    max-width: 480px;
    padding: var(--s-4) var(--s-5);
    background-color: var(--bg-subtle);
    border: 1px solid var(--border);
    border-radius: var(--r-lg);
    color: var(--text-muted);
  }

  .binary-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .binary-title {
    color: var(--text);
  }
</style>
