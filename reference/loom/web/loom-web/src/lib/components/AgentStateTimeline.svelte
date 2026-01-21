<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
  import type { AgentStateKind } from '../api/types';
  import { ThreadDivider } from '$lib/ui';
  import { i18n } from '$lib/i18n';

  interface Props {
    currentState: AgentStateKind;
    retries?: number;
    pendingToolCalls?: string[];
    weaverColor?: string;
  }

  let { currentState, retries = 0, pendingToolCalls = [], weaverColor = 'var(--weaver-indigo)' }: Props = $props();

  const states: { key: AgentStateKind; labelKey: string }[] = [
    { key: 'waiting_input', labelKey: 'state.idle' },
    { key: 'thinking', labelKey: 'state.weaving' },
    { key: 'streaming', labelKey: 'state.threading' },
    { key: 'tool_executing', labelKey: 'state.shuttlePass' },
  ];

  function isActive(stateKey: AgentStateKind): boolean {
    return stateKey === currentState;
  }

  function isPast(stateKey: AgentStateKind): boolean {
    const stateOrder: AgentStateKind[] = [
      'waiting_input',
      'thinking',
      'streaming',
      'tool_executing',
    ];
    const currentIndex = stateOrder.indexOf(currentState);
    const stateIndex = stateOrder.indexOf(stateKey);
    return stateIndex < currentIndex && currentIndex !== -1;
  }
</script>

<div class="timeline" style="--weaver-color: {weaverColor}">
  {#each states as state, i}
    <div class="timeline-step">
      <div
        class="timeline-dot"
        class:timeline-dot-active={isActive(state.key)}
        class:timeline-dot-past={isPast(state.key)}
        class:timeline-dot-weaving={isActive(state.key) && (state.key === 'thinking' || state.key === 'streaming')}
      ></div>
      <span
        class="timeline-label"
        class:timeline-label-active={isActive(state.key)}
      >
        {i18n.t(state.labelKey)}
        {#if isActive(state.key) && state.key === 'tool_executing' && pendingToolCalls.length > 0}
          <span class="timeline-count">({pendingToolCalls.length})</span>
        {/if}
      </span>
    </div>
    
    {#if i < states.length - 1}
      <div
        class="timeline-thread"
        class:timeline-thread-active={isPast(states[i + 1].key) || isActive(states[i + 1].key)}
      ></div>
    {/if}
  {/each}
  
  {#if currentState === 'error'}
    <div class="timeline-error">
      <span class="timeline-error-dot"></span>
      <span class="timeline-error-label">{i18n.t('state.brokenThread', { retries })}</span>
    </div>
  {/if}
</div>

<style>
  .timeline {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-3);
    background: var(--color-bg-muted);
    border-radius: var(--radius-md);
    font-family: var(--font-mono);
  }

  .timeline-step {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .timeline-dot {
    width: 8px;
    height: 8px;
    border-radius: var(--radius-full);
    background: var(--color-bg-subtle);
    border: 1px solid var(--color-border);
    transition: all 0.2s ease;
  }

  .timeline-dot-active {
    background: var(--weaver-color, var(--weaver-indigo));
    border-color: var(--weaver-color, var(--weaver-indigo));
    box-shadow: 0 0 8px color-mix(in srgb, var(--weaver-color, var(--weaver-indigo)) 50%, transparent);
  }

  .timeline-dot-past {
    background: var(--color-success);
    border-color: var(--color-success);
  }

  .timeline-dot-weaving {
    animation: thread-weaving-dot 1.5s ease-in-out infinite;
  }

  .timeline-label {
    font-size: var(--text-sm);
    color: var(--color-fg-muted);
    transition: color 0.2s ease;
  }

  .timeline-label-active {
    color: var(--weaver-color, var(--weaver-indigo));
    font-weight: 500;
  }

  .timeline-count {
    font-size: var(--text-xs);
    color: var(--color-fg-subtle);
  }

  .timeline-thread {
    flex: 1;
    height: 1px;
    margin: 0 var(--space-2);
    background: var(--color-border);
    transition: background 0.2s ease;
  }

  .timeline-thread-active {
    background: var(--weaver-color, var(--weaver-indigo));
  }

  .timeline-error {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-left: var(--space-4);
    padding-left: var(--space-4);
    border-left: 1px solid var(--color-border);
  }

  .timeline-error-dot {
    width: 8px;
    height: 8px;
    border-radius: var(--radius-full);
    background: var(--color-error);
    animation: thread-snap 0.5s ease-out forwards;
  }

  .timeline-error-label {
    font-size: var(--text-sm);
    color: var(--color-error);
  }

  @keyframes thread-weaving-dot {
    0%, 100% {
      opacity: 1;
      transform: scale(1);
    }
    50% {
      opacity: 0.7;
      transform: scale(1.2);
    }
  }

  @keyframes thread-snap {
    0% {
      transform: scale(1);
      opacity: 1;
    }
    20% {
      transform: scale(1.2);
    }
    40% {
      transform: scale(0.8);
      opacity: 0.8;
    }
    100% {
      transform: scale(1);
      opacity: 1;
    }
  }
</style>
