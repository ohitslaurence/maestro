<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    variant?: 'primary' | 'secondary' | 'ghost' | 'danger' | 'warning' | 'success';
    size?: 'sm' | 'md' | 'lg';
    disabled?: boolean;
    loading?: boolean;
    type?: 'button' | 'submit' | 'reset';
    onclick?: (event: MouseEvent) => void;
    class?: string;
    children: Snippet;
  }

  let {
    variant = 'primary',
    size = 'md',
    disabled = false,
    loading = false,
    type = 'button',
    onclick,
    class: className = '',
    children,
  }: Props = $props();
</script>

<button
  {type}
  {disabled}
  class="btn btn-{variant} btn-{size} {className}"
  onclick={onclick}
>
  {#if loading}
    <svg class="spinner" viewBox="0 0 24 24" fill="none">
      <circle class="spinner-track" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
      <path class="spinner-head" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
    </svg>
  {/if}
  {@render children()}
</button>

<style>
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-family: var(--font-mono);
    font-weight: 500;
    border-radius: var(--radius-md);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .btn:focus-visible {
    outline: none;
    box-shadow: 0 0 0 2px var(--color-bg), 0 0 0 4px var(--color-accent);
  }

  .btn:disabled {
    pointer-events: none;
    opacity: 0.5;
  }

  /* Sizes */
  .btn-sm {
    height: 32px;
    padding: 0 var(--space-3);
    font-size: var(--text-sm);
  }

  .btn-md {
    height: 40px;
    padding: var(--space-2) var(--space-4);
    font-size: var(--text-sm);
  }

  .btn-lg {
    height: 48px;
    padding: 0 var(--space-6);
    font-size: var(--text-base);
    border-radius: var(--radius-lg);
  }

  /* Variants */
  .btn-primary {
    background: var(--color-accent);
    color: var(--color-bg);
    border: 1px solid var(--color-accent);
  }

  .btn-primary:hover {
    background: var(--color-accent-hover);
    border-color: var(--color-accent-hover);
  }

  .btn-secondary {
    background: var(--color-bg-muted);
    color: var(--color-fg);
    border: 1px solid var(--color-border);
  }

  .btn-secondary:hover {
    background: var(--color-bg-subtle);
  }

  .btn-ghost {
    background: transparent;
    color: var(--color-fg-muted);
    border: 1px solid transparent;
  }

  .btn-ghost:hover {
    color: var(--color-fg);
    background: var(--color-bg-subtle);
  }

  .btn-danger {
    background: var(--color-error);
    color: var(--color-bg);
    border: 1px solid var(--color-error);
  }

  .btn-danger:hover {
    opacity: 0.9;
  }

  .btn-warning {
    background: var(--color-warning);
    color: var(--color-bg);
    border: 1px solid var(--color-warning);
  }

  .btn-warning:hover {
    opacity: 0.9;
  }

  .btn-success {
    background: var(--color-success);
    color: var(--color-bg);
    border: 1px solid var(--color-success);
  }

  .btn-success:hover {
    opacity: 0.9;
  }

  /* Loading spinner */
  .spinner {
    width: 16px;
    height: 16px;
    margin-right: var(--space-2);
    animation: spin 1s linear infinite;
  }

  .spinner-track {
    opacity: 0.25;
  }

  .spinner-head {
    opacity: 0.75;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }
</style>
