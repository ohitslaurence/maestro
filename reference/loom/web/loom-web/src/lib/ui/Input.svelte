<script lang="ts">
  interface Props {
    value?: string;
    placeholder?: string;
    type?: 'text' | 'email' | 'password' | 'search';
    disabled?: boolean;
    error?: string;
    label?: string;
    id?: string;
    class?: string;
    oninput?: (event: Event) => void;
    onkeydown?: (event: KeyboardEvent) => void;
  }

  let {
    value = $bindable(''),
    placeholder = '',
    type = 'text',
    disabled = false,
    error,
    label,
    id,
    class: className,
    oninput,
    onkeydown,
  }: Props = $props();

  const fallbackId = `input-${Math.random().toString(36).slice(2)}`;
  const inputId = $derived(id || fallbackId);
</script>

<div class="input-wrapper {className ?? ''}">
  {#if label}
    <label for={inputId} class="input-label">
      {label}
    </label>
  {/if}
  
  <input
    {type}
    id={inputId}
    bind:value
    {placeholder}
    {disabled}
    class="input"
    class:input-error={error}
    oninput={oninput}
    onkeydown={onkeydown}
  />
  
  {#if error}
    <p class="input-error-message">{error}</p>
  {/if}
</div>

<style>
  .input-wrapper {
    width: 100%;
  }

  .input-label {
    display: block;
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    font-weight: 500;
    color: var(--color-fg);
    margin-bottom: var(--space-1);
  }

  .input {
    width: 100%;
    height: 40px;
    padding: 0 var(--space-3);
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    background: var(--color-bg-muted);
    color: var(--color-fg);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    transition: border-color 0.15s ease, box-shadow 0.15s ease;
  }

  .input::placeholder {
    color: var(--color-fg-subtle);
  }

  .input:focus {
    outline: none;
    border-color: var(--color-accent);
    box-shadow: 0 0 0 2px var(--color-accent-soft);
  }

  .input:disabled {
    cursor: not-allowed;
    opacity: 0.5;
  }

  .input-error {
    border-color: var(--color-error);
  }

  .input-error:focus {
    border-color: var(--color-error);
    box-shadow: 0 0 0 2px var(--color-error-soft);
  }

  .input-error-message {
    margin-top: var(--space-1);
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    color: var(--color-error);
  }
</style>
