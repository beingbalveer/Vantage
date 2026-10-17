<script lang="ts">
  import { shortcutsHelpOpen } from "../../stores/ui";
  import { SHORTCUTS } from "../../keys";
  import { Modal, Kbd } from "../common";

  let categories = ["Navigation", "Review", "Diff", "Panes", "Terminal"] as const;
</script>

{#if $shortcutsHelpOpen}
  <Modal
    open={$shortcutsHelpOpen}
    title="Keyboard Shortcuts"
    width="640px"
    onclose={() => ($shortcutsHelpOpen = false)}
  >
    <div class="shortcuts-grid">
      {#each categories as cat}
        <div class="cat-section">
          <h4 class="cat-title text-strong text-xs">{cat}</h4>
          <div class="shortcut-list">
            {#each SHORTCUTS.filter((s) => s.category === cat) as s}
              <div class="shortcut-row">
                <span class="desc text-sm">{s.description}</span>
                <Kbd shortcut={s.key} />
              </div>
            {/each}
          </div>
        </div>
      {/each}
    </div>
  </Modal>
{/if}

<style>
  .shortcuts-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px 24px;
    max-height: 65vh;
    overflow-y: auto;
  }

  .cat-title {
    color: var(--text-faint);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    margin-bottom: 8px;
  }

  .shortcut-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .shortcut-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .desc {
    color: var(--text);
  }
</style>
