<script lang="ts">
  // Calm empty state (DESIGN §7.6): quiet icon, a short title and one optional
  // next action. Used wherever a request succeeded but returned nothing.
  import Icon from "./Icon.svelte";
  import type { IconName } from "./iconName";

  interface Props {
    icon?: IconName | null;
    title: string;
    description?: string;
    actionLabel?: string;
    onaction?: () => void;
  }

  let {
    icon = null,
    title,
    description = "",
    actionLabel = "",
    onaction,
  }: Props = $props();
</script>

<div class="empty-state">
  {#if icon}
    <span class="empty-icon"><Icon name={icon} size={22} strokeWidth={1.6} /></span>
  {/if}
  <span class="empty-title">{title}</span>
  {#if description}
    <span class="empty-desc text-sm text-muted">{description}</span>
  {/if}
  {#if actionLabel}
    <button type="button" class="empty-action" onclick={onaction}>{actionLabel}</button>
  {/if}
</div>

<style>
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 32px 20px;
    text-align: center;
  }

  .empty-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    margin-bottom: 2px;
    border-radius: var(--r-sm);
    background-color: var(--bg-subtle);
    color: var(--text-faint);
  }

  .empty-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text);
  }

  .empty-desc {
    max-width: 320px;
    line-height: 18px;
  }

  .empty-action {
    margin-top: 6px;
    height: 26px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    background-color: var(--bg);
    color: var(--text);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
  }
  .empty-action:hover {
    background-color: var(--bg-muted);
  }
</style>
