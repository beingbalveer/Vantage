<script lang="ts">
  import { shortcutsHelpOpen } from "../../app/ui";
  import { COMMANDS, SHORTCUT_CATEGORIES } from "../../platform/keys";
  import { Modal, Kbd } from "../common";

  let categories = SHORTCUT_CATEGORIES;
  let shortcuts = COMMANDS.filter((c) => c.keys.length > 0);
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
            {#each shortcuts.filter((s) => s.category === cat) as s}
              <div class="shortcut-row">
                <span class="desc text-sm">{s.label}</span>
                <span class="keys">
                  {#each s.keys as k}<Kbd shortcut={k} />{/each}
                </span>
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

  .keys {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }
</style>
