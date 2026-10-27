<script lang="ts">
  import type { IconName } from "./iconName";

  interface Props {
    name: IconName;
    size?: number | string;
    class?: string;
    strokeWidth?: number;
  }

  let {
    name,
    size = 14,
    class: className = "",
    strokeWidth = 2,
  }: Props = $props();
</script>

<svg
  xmlns="http://www.w3.org/2000/svg"
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width={strokeWidth}
  stroke-linecap="round"
  stroke-linejoin="round"
  class="sf-icon {className}"
  aria-hidden="true"
>
  {#if name === "sidebar"}
    <!-- SF Symbol sidebar.left / panel layout -->
    <rect width="18" height="18" x="3" y="3" rx="3" />
    <path d="M9 3v18" />
  {:else if name === "git-branch"}
    <!-- SF Symbol arrow.triangle.branch / git branch -->
    <line x1="6" x2="6" y1="3" y2="15" />
    <circle cx="18" cy="6" r="3" />
    <circle cx="6" cy="18" r="3" />
    <path d="M18 9a9 9 0 0 1-9 9" />
  {:else if name === "chevron-down"}
    <!-- SF Symbol chevron.down -->
    <path d="m6 9 6 6 6-6" />
  {:else if name === "chevron-up"}
    <!-- SF Symbol chevron.up -->
    <path d="m18 15-6-6-6 6" />
  {:else if name === "chevron-right"}
    <!-- SF Symbol chevron.right -->
    <path d="m9 18 6-6-6-6" />
  {:else if name === "check"}
    <!-- SF Symbol checkmark -->
    <path d="M20 6 9 17l-5-5" />
  {:else if name === "arrow-up"}
    <!-- SF Symbol arrow.up -->
    <path d="m5 12 7-7 7 7" />
    <path d="M12 19V5" />
  {:else if name === "arrow-down"}
    <!-- SF Symbol arrow.down -->
    <path d="M12 5v14" />
    <path d="m19 12-7 7-7-7" />
  {:else if name === "arrow-up-down"}
    <!-- SF Symbol arrow.up.arrow.down -->
    <path d="m7 15 5 5 5-5" />
    <path d="m7 9 5-5 5 5" />
  {:else if name === "refresh"}
    <!-- SF Symbol arrow.clockwise -->
    <path d="M3 12a9 9 0 0 1 15-6.7L21 8" />
    <path d="M21 3v5h-5" />
    <path d="M21 12a9 9 0 0 1-15 6.7L3 16" />
    <path d="M3 21v-5h5" />
  {:else if name === "search"}
    <!-- SF Symbol magnifyingglass -->
    <circle cx="11" cy="11" r="7" />
    <path d="m21 21-4.35-4.35" />
  {:else if name === "file-text"}
    <!-- SF Symbol doc.text -->
    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
    <polyline points="14 2 14 8 20 8" />
    <line x1="16" x2="8" y1="13" y2="13" />
    <line x1="16" x2="8" y1="17" y2="17" />
    <line x1="10" x2="8" y1="9" y2="9" />
  {:else if name === "terminal"}
    <!-- SF Symbol terminal / chevron.left.forwardslash.chevron.right -->
    <polyline points="4 17 10 11 4 5" />
    <line x1="12" x2="20" y1="19" y2="19" />
  {:else if name === "gear"}
    <!-- SF Symbol gearshape -->
    <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
    <circle cx="12" cy="12" r="3" />
  {:else if name === "question"}
    <!-- SF Symbol questionmark.circle -->
    <circle cx="12" cy="12" r="10" />
    <path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3" />
    <line x1="12" x2="12.01" y1="17" y2="17" />
  {:else if name === "more-horizontal"}
    <!-- SF Symbol ellipsis -->
    <circle cx="12" cy="12" r="1.5" fill="currentColor" />
    <circle cx="19" cy="12" r="1.5" fill="currentColor" />
    <circle cx="5" cy="12" r="1.5" fill="currentColor" />
  {:else if name === "pin"}
    <!-- SF Symbol pin -->
    <line x1="12" x2="12" y1="17" y2="22" />
    <path d="M5 17h14v-1.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V6h1a1 1 0 0 0 0-2H8a1 1 0 0 0 0 2h1v4.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24Z" />
  {:else if name === "folder"}
    <!-- SF Symbol folder -->
    <path d="M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.93a2 2 0 0 1-1.66-.9l-.82-1.2A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z" />
  {:else if name === "external-link"}
    <!-- SF Symbol arrow.up.right.square -->
    <path d="M15 3h6v6" />
    <path d="M10 14 21 3" />
    <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
  {:else if name === "trash"}
    <!-- SF Symbol trash -->
    <path d="M3 6h18" />
    <path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6" />
    <path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" />
    <line x1="10" x2="10" y1="11" y2="17" />
    <line x1="14" x2="14" y1="11" y2="17" />
  {:else if name === "plus"}
    <!-- SF Symbol plus -->
    <path d="M5 12h14" />
    <path d="M12 5v14" />
  {:else if name === "minus"}
    <!-- SF Symbol minus -->
    <path d="M5 12h14" />
  {:else if name === "close"}
    <!-- SF Symbol xmark -->
    <path d="m18 6-12 12" />
    <path d="m6 6 12 12" />
  {:else if name === "commit"}
    <!-- SF Symbol circle.dot -->
    <circle cx="12" cy="12" r="3" fill="currentColor" />
    <circle cx="12" cy="12" r="8" />
  {:else if name === "message"}
    <!-- SF Symbol bubble.left -->
    <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
  {:else if name === "zap"}
    <!-- SF Symbol bolt.fill -->
    <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2" fill="currentColor" stroke="none" />
  {:else if name === "sparkles"}
    <path d="m12 3-1.9 5.8a2 2 0 0 1-1.3 1.3L3 12l5.8 1.9a2 2 0 0 1 1.3 1.3L12 21l1.9-5.8a2 2 0 0 1 1.3-1.3L21 12l-5.8-1.9a2 2 0 0 1-1.3-1.3Z" />
  {:else if name === "expand"}
    <path d="m15 3 6 6m0-6-6 6M9 21l-6-6m0 6 6-6M21 3v6h-6M3 21v-6h6" />
  {:else if name === "key"}
    <!-- SF Symbol key -->
    <circle cx="7.5" cy="15.5" r="5.5" />
    <path d="m21 2-9.6 9.6" />
    <path d="m15.5 7.5 3 3L22 7l-3-3" />
  {:else if name === "test-tube"}
    <!-- SF Symbol flask -->
    <path d="M10 2v7.31a2 2 0 0 1-.37 1.17l-5.26 7.89A2 2 0 0 0 6 21.5h12a2 2 0 0 0 1.63-3.13l-5.26-7.89a2 2 0 0 1-.37-1.17V2" />
    <path d="M8.5 2h7" />
  {:else if name === "package"}
    <!-- SF Symbol shippingbox -->
    <path d="M16.5 9.4 7.55 4.24" />
    <path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z" />
    <polyline points="3.29 7 12 12 20.71 7" />
    <line x1="12" x2="12" y1="22" y2="12" />
  {:else if name === "hard-drive"}
    <!-- SF Symbol internaldrive -->
    <line x1="22" x2="2" y1="12" y2="12" />
    <path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z" />
    <line x1="6" x2="6.01" y1="16" y2="16" />
    <line x1="10" x2="10.01" y1="16" y2="16" />
  {:else if name === "flag"}
    <!-- SF Symbol flag -->
    <path d="M4 15s1-1 4-1 5 2 8 2 4-1 4-1V3s-1 1-4 1-5-2-8-2-4 1-4 1z" />
    <line x1="4" x2="4" y1="22" y2="15" />
  {:else if name === "binary"}
    <!-- SF Symbol number / 01 -->
    <rect width="18" height="18" x="3" y="3" rx="2" />
    <path d="M9 8v8" />
    <circle cx="15" cy="12" r="3" />
  {:else if name === "clock"}
    <!-- SF Symbol clock / history -->
    <circle cx="12" cy="12" r="9" />
    <polyline points="12 7 12 12 15.5 14" />
  {:else if name === "copy"}
    <!-- SF Symbol doc.on.doc / copy -->
    <rect width="13" height="13" x="8" y="8" rx="2" ry="2" />
    <path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" />
  {:else if name === "unfold" || name === "chevrons-up-down"}
    <!-- Expand all / unfold (chevrons pointing outward) -->
    <path d="m7 9 5-5 5 5" />
    <path d="m7 15 5 5 5-5" />
  {:else if name === "fold" || name === "chevrons-down-up"}
    <!-- Collapse all / fold (chevrons pointing inward) -->
    <path d="m7 4 5 5 5-5" />
    <path d="m7 20 5-5 5 5" />
  {:else if name === "code"}
    <!-- Code brackets -->
    <path d="m8 7-5 5 5 5" />
    <path d="m16 7 5 5-5 5" />
    <path d="m13 4-2 16" />
  {:else if name === "grid"}
    <!-- Layout grid -->
    <rect x="3" y="3" width="7" height="7" rx="2" />
    <rect x="14" y="3" width="7" height="7" rx="2" />
    <rect x="3" y="14" width="7" height="7" rx="2" />
    <rect x="14" y="14" width="7" height="7" rx="2" />
  {:else if name === "worktree"}
    <!-- Worktree nodes -->
    <rect x="3" y="3" width="7" height="7" rx="1.5" />
    <rect x="14" y="14" width="7" height="7" rx="1.5" />
    <path d="M6.5 10v7.5H14" />
    <path d="M17.5 3v7M14 6.5h7" />
  {:else if name === "changes"}
    <!-- Uncommitted changes -->
    <circle cx="12" cy="12" r="8" stroke-dasharray="2 3" />
    <path d="M12 8v8m-4-4h8" />
  {:else if name === "pencil"}
    <!-- SF Symbol pencil -->
    <path d="M12 20h9" />
    <path d="M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4Z" />
  {:else if name === "undo"}
    <!-- SF Symbol arrow.uturn.backward -->
    <path d="M9 14 4 9l5-5" />
    <path d="M4 9h10.5a5.5 5.5 0 0 1 5.5 5.5 5.5 5.5 0 0 1-5.5 5.5H11" />
  {:else if name === "alert"}
    <!-- SF Symbol exclamationmark.triangle (warning) -->
    <path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" />
    <line x1="12" x2="12" y1="9" y2="13" />
    <line x1="12" x2="12.01" y1="17" y2="17" />
  {:else if name === "diff-unified"}
    <!-- Unified diff: a single pane with rows -->
    <rect x="3" y="5" width="18" height="14" rx="1" />
    <path d="M7 9.5h10M7 14.5h10" />
  {:else if name === "diff-split"}
    <!-- Side-by-side diff: one pane split by a rule -->
    <rect x="3" y="5" width="18" height="14" rx="1" />
    <path d="M12 5v14" />
  {:else if name === "wrap"}
    <!-- Word wrap / return arrow -->
    <path d="M4 6h16M4 12h12a3 3 0 0 1 3 3v0a3 3 0 0 1-3 3H11" />
    <path d="m14 15-3 3 3 3" />
    <path d="M4 18h3" />
  {:else if name === "image"}
    <!-- Picture: frame, sun, ridge -->
    <rect x="3" y="3" width="18" height="18" rx="2" />
    <circle cx="9" cy="9" r="1.6" />
    <path d="m21 15-4.5-4.5a2 2 0 0 0-2.8 0L6 21" />
  {:else if name === "file"}
    <!-- Document -->
    <path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z" />
    <path d="M14 3v6h6" />
  {:else if name === "download"}
    <!-- Clone / download (arrow into tray) -->
    <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
    <polyline points="7 10 12 15 17 10" />
    <line x1="12" x2="12" y1="15" y2="3" />
  {:else if name === "files"}
    <!-- SF Symbol doc.on.doc / stacked documents (large change) -->
    <path d="M20 7h-3a2 2 0 0 1-2-2V2" />
    <path d="M9 18a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h7l4 4v10a2 2 0 0 1-2 2Z" />
    <path d="M3 7.6v12.8A1.6 1.6 0 0 0 4.6 22h9.8" />

  {:else if name === "tag"}
    <!-- SF Symbol tag -->
    <path
      d="M12.59 2.59A2 2 0 0 0 11.17 2H4a2 2 0 0 0-2 2v7.17a2 2 0 0 0 .59 1.42l8.7 8.7a2.43 2.43 0 0 0 3.42 0l6.58-6.58a2.43 2.43 0 0 0 0-3.42z"
    />
    <circle cx="7.5" cy="7.5" r="1" fill="currentColor" stroke="none" />
  {/if}
</svg>

<style>
  .sf-icon {
    display: block;
    flex-shrink: 0;
  }
</style>
