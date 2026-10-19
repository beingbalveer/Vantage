<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    value?: string;
    type?: "text" | "password" | "search" | "number" | "email";
    placeholder?: string;
    disabled?: boolean;
    readonly?: boolean;
    autofocus?: boolean;
    size?: "sm" | "md" | "lg";
    error?: string | boolean;
    fullWidth?: boolean;
    class?: string;
    inputClass?: string;
    id?: string;
    min?: number | string;
    max?: number | string;
    ariaLabel?: string;
    leading?: Snippet;
    trailing?: Snippet;
    onkeydown?: (e: KeyboardEvent) => void;
    oninput?: (e: Event) => void;
    onchange?: (e: Event) => void;
    onfocus?: (e: FocusEvent) => void;
    onblur?: (e: FocusEvent) => void;
  }

  let {
    value = $bindable(""),
    type = "text",
    placeholder = "",
    disabled = false,
    readonly = false,
    autofocus = false,
    size = "md",
    error = false,
    fullWidth = true,
    class: className = "",
    inputClass = "",
    id,
    min,
    max,
    ariaLabel,
    leading,
    trailing,
    onkeydown,
    oninput,
    onchange,
    onfocus,
    onblur,
  }: Props = $props();

  let inputEl = $state<HTMLInputElement | null>(null);

  $effect(() => {
    if (autofocus && inputEl) {
      inputEl.focus();
    }
  });
</script>

<div
  class="relay-input-wrap {size} {fullWidth ? 'full-width' : ''} {error ? 'has-error' : ''} {disabled ? 'disabled' : ''} {className}"
>
  {#if leading}
    <div class="input-leading">
      {@render leading()}
    </div>
  {/if}

  <input
    bind:this={inputEl}
    {id}
    {type}
    bind:value
    {placeholder}
    {disabled}
    {readonly}
    {min}
    {max}
    aria-label={ariaLabel}
    class="relay-input-field {inputClass}"
    {onkeydown}
    {oninput}
    {onchange}
    {onfocus}
    {onblur}
  />

  {#if trailing}
    <div class="input-trailing">
      {@render trailing()}
    </div>
  {/if}
</div>

<style>
  .relay-input-wrap {
    display: inline-flex;
    align-items: center;
    background-color: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    transition: border-color 80ms ease, box-shadow 80ms ease;
    box-sizing: border-box;
    position: relative;
    overflow: hidden;
  }

  .relay-input-wrap.full-width {
    width: 100%;
  }

  /* Sizes */
  .relay-input-wrap.sm {
    height: 24px;
    padding: 0 6px;
    font-size: 11px;
  }

  .relay-input-wrap.md {
    height: 28px;
    padding: 0 8px;
    font-size: 12px;
  }

  .relay-input-wrap.lg {
    height: 34px;
    padding: 0 10px;
    font-size: 13px;
  }

  /* States */
  .relay-input-wrap:focus-within {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }

  .relay-input-wrap.has-error {
    border-color: var(--danger);
  }

  .relay-input-wrap.has-error:focus-within {
    box-shadow: 0 0 0 1px var(--danger);
  }

  .relay-input-wrap.disabled {
    background-color: var(--bg-subtle);
    opacity: 0.6;
    cursor: not-allowed;
  }

  .input-leading {
    display: flex;
    align-items: center;
    justify-content: center;
    margin-right: 6px;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .input-trailing {
    display: flex;
    align-items: center;
    justify-content: center;
    margin-left: 6px;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .relay-input-field {
    flex: 1;
    min-width: 0;
    height: 100%;
    border: none;
    outline: none;
    background: transparent;
    color: var(--text);
    font-family: inherit;
    font-size: inherit;
    padding: 0;
    margin: 0;
  }

  .relay-input-field::placeholder {
    color: var(--text-faint);
  }

  .relay-input-field:disabled {
    cursor: not-allowed;
  }
</style>
