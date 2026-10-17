<script lang="ts">
  interface Props {
    checked?: boolean;
    disabled?: boolean;
    label?: string;
    class?: string;
    onchange?: (checked: boolean) => void;
  }

  let {
    checked = $bindable(false),
    disabled = false,
    label = "",
    class: className = "",
    onchange,
  }: Props = $props();

  function handleChange(e: Event) {
    const target = e.target as HTMLInputElement;
    checked = target.checked;
    onchange?.(target.checked);
  }
</script>

<label class="relay-checkbox-wrap {disabled ? 'disabled' : ''} {className}">
  <input
    type="checkbox"
    class="relay-checkbox"
    {checked}
    {disabled}
    onchange={handleChange}
  />
  {#if label}
    <span class="relay-checkbox-label">{label}</span>
  {/if}
</label>

<style>
  .relay-checkbox-wrap {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
    user-select: none;
    font-size: 13px;
    color: var(--text);
  }

  .relay-checkbox-wrap.disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }

  .relay-checkbox {
    appearance: none;
    -webkit-appearance: none;
    width: 16px;
    height: 16px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background-color: var(--bg);
    cursor: pointer;
    display: grid;
    place-content: center;
    transition: background-color 80ms ease, border-color 80ms ease, box-shadow 80ms ease;
    margin: 0;
  }

  .relay-checkbox:hover:not(:disabled) {
    border-color: var(--accent);
  }

  .relay-checkbox:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .relay-checkbox:checked {
    background-color: var(--accent);
    border-color: var(--accent);
  }

  .relay-checkbox:checked::before {
    content: "";
    width: 9px;
    height: 5px;
    border-left: 2px solid #ffffff;
    border-bottom: 2px solid #ffffff;
    transform: rotate(-45deg) translate(0.5px, -0.5px);
  }

  .relay-checkbox-label {
    line-height: 1.4;
  }
</style>
