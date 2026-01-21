<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    padding?: 'none' | 'sm' | 'md' | 'lg';
    hover?: boolean;
    showDivider?: boolean;
    header?: Snippet;
    footer?: Snippet;
    children: Snippet;
  }

  let {
    padding = 'md',
    hover = false,
    showDivider = false,
    header,
    footer,
    children,
  }: Props = $props();
</script>

<div class="card" class:card-hover={hover}>
  {#if header}
    <div class="card-header">
      {@render header()}
    </div>
    {#if showDivider}
      <div class="thread-divider"></div>
    {/if}
  {/if}
  
  <div class="card-body card-padding-{padding}">
    {@render children()}
  </div>
  
  {#if footer}
    {#if showDivider}
      <div class="thread-divider"></div>
    {/if}
    <div class="card-footer">
      {@render footer()}
    </div>
  {/if}
</div>

<style>
  .card {
    background: var(--color-bg-muted);
    border: 1px solid var(--color-border-muted);
    border-radius: var(--radius-md);
  }

  .card-hover {
    transition: box-shadow 0.15s ease;
    cursor: pointer;
  }

  .card-hover:hover {
    box-shadow: var(--shadow-md);
  }

  .card-header {
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--color-border-muted);
  }

  .card-body {
    display: block;
  }

  .card-padding-none {
    padding: 0;
  }

  .card-padding-sm {
    padding: var(--space-3);
  }

  .card-padding-md {
    padding: var(--space-4);
  }

  .card-padding-lg {
    padding: var(--space-6);
  }

  .card-footer {
    padding: var(--space-3) var(--space-4);
    background: var(--color-bg-subtle);
    border-top: 1px solid var(--color-border-muted);
    border-radius: 0 0 var(--radius-md) var(--radius-md);
  }

  .thread-divider {
    height: 1px;
    background: linear-gradient(
      90deg,
      transparent 0%,
      var(--color-thread-muted) 10%,
      var(--color-thread-muted) 90%,
      transparent 100%
    );
    margin: 0;
  }
</style>
