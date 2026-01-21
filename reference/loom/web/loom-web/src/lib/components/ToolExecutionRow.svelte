<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
  import type { ToolExecutionStatus } from '../api/types';
  import { ToolStatusBadge } from '../ui';
  import { i18n } from '$lib/i18n';

  interface Props {
    execution: ToolExecutionStatus;
  }

  let { execution }: Props = $props();
</script>

<div class="execution-row">
  <div class="execution-header">
    <div class="execution-info">
      <span class="execution-name">{execution.tool_name}</span>
      <ToolStatusBadge status={execution} />
    </div>
    <span class="execution-id">
      {execution.call_id.slice(0, 8)}...
    </span>
  </div>
  
  {#if execution.status === 'completed' || execution.status === 'failed'}
    <details class="execution-details">
      <summary class="execution-summary">
        {execution.error ? i18n.t('tool.showError') : i18n.t('tool.showOutput')}
      </summary>
      <pre class="execution-output" class:execution-output-error={execution.error}>{execution.error ?? JSON.stringify(execution.result, null, 2)}</pre>
    </details>
  {/if}
</div>

<style>
  .execution-row {
    padding: var(--space-3);
    font-family: var(--font-mono);
  }

  .execution-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .execution-info {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .execution-name {
    font-size: var(--text-sm);
    color: var(--color-fg);
    font-weight: 500;
  }

  .execution-id {
    font-size: var(--text-xs);
    color: var(--color-fg-subtle);
  }

  .execution-details {
    margin-top: var(--space-2);
  }

  .execution-summary {
    font-size: var(--text-xs);
    color: var(--color-fg-muted);
    cursor: pointer;
    transition: color 0.15s ease;
  }

  .execution-summary:hover {
    color: var(--color-fg);
  }

  .execution-output {
    margin-top: var(--space-2);
    padding: var(--space-2);
    background: var(--color-bg-subtle);
    border-radius: var(--radius-md);
    font-size: var(--text-xs);
    color: var(--color-fg-muted);
    overflow-x: auto;
    max-height: 8rem;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .execution-output-error {
    background: var(--color-error-soft);
    color: var(--color-error);
  }
</style>
