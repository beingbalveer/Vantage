<script lang="ts">
  import { formatKey } from "../../platform/keys";

  interface Props {
    shortcut: string;
  }

  let { shortcut }: Props = $props();
</script>

<kbd class="relay-kbd">{formatKey(shortcut)}</kbd>

<style>
  .relay-kbd {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0 5px;
    min-width: 18px;
    height: 18px;
    background-color: var(--bg-muted);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 500;
    color: var(--text-muted);
    line-height: 1;
    user-select: none;
  }
</style>
