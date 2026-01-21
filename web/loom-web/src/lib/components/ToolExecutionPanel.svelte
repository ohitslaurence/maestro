<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
  import type { ToolExecutionStatus } from '../api/types';
  import { ToolStatusBadge, Card, ThreadDivider } from '../ui';
  import ToolExecutionRow from './ToolExecutionRow.svelte';
  import { i18n } from '$lib/i18n';

  interface Props {
    executions: ToolExecutionStatus[];
    expanded?: boolean;
  }

  let { executions, expanded = false }: Props = $props();

  let isExpanded = $state<boolean | undefined>(undefined);
  const effectiveExpanded = $derived(isExpanded ?? expanded);
</script>

{#if executions.length > 0}
  <Card padding="none">
    <button
      type="button"
      class="panel-header"
      onclick={() => isExpanded = !effectiveExpanded}
    >
      <span class="panel-title">
        {i18n._('tool.shuttlePasses')} ({executions.length})
      </span>
      <span class="panel-toggle" class:panel-toggle-open={effectiveExpanded}>
        ▼
      </span>
    </button>
    
    {#if effectiveExpanded}
      <div class="panel-content">
        {#each executions as execution, i (execution.call_id)}
          <ToolExecutionRow {execution} />
          {#if i < executions.length - 1}
            <ThreadDivider variant="simple" class="divider" />
          {/if}
        {/each}
      </div>
    {/if}
  </Card>
{/if}

<style>
  .panel-header {
    width: 100%;
    padding: var(--space-3);
    display: flex;
    align-items: center;
    justify-content: space-between;
    background: transparent;
    border: none;
    cursor: pointer;
    font-family: var(--font-mono);
    transition: background 0.15s ease;
  }

  .panel-header:hover {
    background: var(--color-bg-muted);
  }

  .panel-title {
    font-weight: 500;
    font-size: var(--text-sm);
    color: var(--color-fg);
  }

  .panel-toggle {
    color: var(--color-fg-muted);
    font-size: var(--text-xs);
    transition: transform 0.2s ease;
  }

  .panel-toggle-open {
    transform: rotate(180deg);
  }

  .panel-content {
    border-top: 1px solid var(--color-border);
    padding: var(--space-2);
  }

  .panel-content :global(.divider) {
    margin: var(--space-2) 0;
  }
</style>
