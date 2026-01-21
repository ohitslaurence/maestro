<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    variant?: 'default' | 'accent' | 'success' | 'warning' | 'error' | 'info' | 'muted';
    size?: 'sm' | 'md';
    children: Snippet;
  }

  let {
    variant = 'default',
    size = 'md',
    children,
  }: Props = $props();
</script>

<span class="badge badge-{variant} badge-{size}">
  {@render children()}
</span>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    font-family: var(--font-mono);
    font-weight: 500;
    border-radius: var(--radius-md);
  }

  /* Sizes */
  .badge-sm {
    padding: var(--space-1) var(--space-2);
    font-size: var(--text-xs);
  }

  .badge-md {
    padding: var(--space-1) var(--space-3);
    font-size: var(--text-xs);
  }

  /* Variants */
  .badge-default {
    background: var(--color-bg-subtle);
    color: var(--color-fg);
  }

  .badge-accent {
    background: var(--color-accent-soft);
    color: var(--color-accent);
  }

  .badge-success {
    background: var(--color-success-soft);
    color: var(--color-success);
  }

  .badge-warning {
    background: var(--color-warning-soft);
    color: var(--color-warning);
  }

  .badge-error {
    background: var(--color-error-soft);
    color: var(--color-error);
  }

  .badge-info {
    background: var(--color-info-soft);
    color: var(--color-info);
  }

  .badge-muted {
    background: var(--color-bg-muted);
    color: var(--color-fg-muted);
  }
</style>
