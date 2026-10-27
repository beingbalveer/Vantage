<script lang="ts">
  import Icon from "../common/Icon.svelte";
  import TechIcon from "../common/TechIcon.svelte";
  import {
    repoIcons,
    detectedStacks,
    detectAndCacheStack,
    type RepoIcon as RepoIconValue,
  } from "../../domain/stores/repoIcons";

  interface Props {
    id?: string;
    repoName?: string;
    repoPath?: string;
    size?: number;
    choice?: RepoIconValue;
    strokeWidth?: number;
  }

  let {
    id = "",
    repoName = "",
    repoPath = "",
    size = 20,
    choice,
    strokeWidth = 1.7,
  }: Props = $props();

  let customChoice = $derived(choice ?? (id ? $repoIcons[id] : undefined));

  // If repoPath is known and no custom icon is set, automatically trigger detection if not cached yet
  $effect(() => {
    if (id && repoPath && (!customChoice || customChoice.kind === "auto")) {
      if (!$detectedStacks[id]) {
        detectAndCacheStack(id, repoPath);
      }
    }
  });

  let detectedInfo = $derived(id ? $detectedStacks[id] : undefined);

  // Resolved presentation
  let mode = $derived.by(() => {
    if (customChoice) {
      if (customChoice.kind === "image") return { type: "image", val: customChoice.value };
      if (customChoice.kind === "emoji") return { type: "emoji", val: customChoice.value };
      if (customChoice.kind === "tech") return { type: "tech", val: customChoice.value };
      if (customChoice.kind === "role") return { type: "role", val: customChoice.value };
      if (customChoice.kind === "app") return { type: "app", val: customChoice.value };
      if (customChoice.kind === "letter")
        return { type: "letter", val: customChoice.value, color: customChoice.color };
      // if kind === "auto", fall through to auto-detect
    }

    // Auto-detect
    if (detectedInfo?.detected) {
      return { type: "tech", val: detectedInfo.detected };
    }

    // Fallback: first letter of repoName or id
    const letter = (repoName || id || "R").trim().charAt(0).toUpperCase() || "R";
    return { type: "letter", val: letter };
  });
</script>

<span class="repo-icon-inner" style="width:{size}px;height:{size}px">
  {#if mode.type === "image"}
    <img src={mode.val} alt="" draggable="false" />
  {:else if mode.type === "emoji"}
    <span class="repo-emoji" style="font-size:{size * 0.85}px">{mode.val}</span>
  {:else if mode.type === "tech" || mode.type === "role"}
    <TechIcon name={mode.val} {size} />
  {:else if mode.type === "app"}
    <Icon name={mode.val as any} {size} {strokeWidth} />
  {:else if mode.type === "letter"}
    <span
      class="repo-letter"
      style="font-size:{Math.max(12, size * 0.65)}px;{mode.color ? `color:${mode.color}` : ''}"
    >
      {mode.val}
    </span>
  {/if}
</span>

<style>
  .repo-icon-inner {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    line-height: 1;
    overflow: hidden;
  }

  .repo-icon-inner img {
    width: 100%;
    height: 100%;
    object-fit: contain;
    border-radius: var(--r-sm);
  }

  .repo-emoji {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    line-height: 1;
    user-select: none;
  }

  .repo-letter {
    font-weight: 600;
    line-height: 1;
    font-family: var(--font-ui, system-ui, sans-serif);
    text-transform: uppercase;
    color: #1a1c1f;
    user-select: none;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  :global([data-theme="dark"]) .repo-letter,
  :global(:root[data-theme="dark"]) .repo-letter {
    color: #e6edf3;
  }
</style>
