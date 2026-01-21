<script lang="ts">
  import { page } from '$app/stores';
  import { onMount, onDestroy } from 'svelte';
  import { createActor } from 'xstate';
  import { conversationMachine, connectionMachine } from '$lib/state';
  import { getApiClient } from '$lib/api';
  import { ApiError } from '$lib/api/types';
  import { createRealtimeClient, LoomWebSocketClient, type LlmEvent, type ToolEvent } from '$lib/realtime';
  import { i18n } from '$lib/i18n';
  import {
    MessageList,
    MessageInput,
    AgentStateTimeline,
    ToolExecutionPanel,
    ConnectionStatusIndicator,
    SupportAccessDenied,
  } from '$lib/components';
  import { AgentStateBadge, Skeleton, Card } from '$lib/ui';
  import { logger } from '$lib/logging';
  import type { CurrentUser } from '$lib/api/types';

  const threadId = $derived($page.params.threadId);

  const conversationActor = createActor(conversationMachine);
  const connectionActor = createActor(connectionMachine);
  
  let currentUser = $state<CurrentUser | null>(null);
  let accessDenied = $state(false);
  let isSupportUser = $derived(currentUser?.global_roles?.includes('support') ?? false);
  
  let conversationState = $state(conversationActor.getSnapshot());
  let connectionState = $state(connectionActor.getSnapshot());
  
  let realtimeClient = createRealtimeClient({
    serverUrl: import.meta.env.VITE_LOOM_SERVER_URL || '',
  });
  
  let loadingThreadId = $state<string | null>(null);

  realtimeClient.onStatus((status) => {
    if (status === 'connected') {
      connectionActor.send({ type: 'CONNECTED' });
    } else if (status === 'disconnected' || status === 'error') {
      connectionActor.send({ type: 'DISCONNECTED' });
    } else if (status === 'reconnecting') {
      connectionActor.send({ type: 'DISCONNECTED' });
      connectionActor.send({ type: 'RETRY' });
    }
  });

  conversationActor.subscribe((snapshot) => {
    conversationState = snapshot;
  });

  connectionActor.subscribe((snapshot) => {
    connectionState = snapshot;
  });

  async function loadThread(id: string) {
    if (loadingThreadId === id) return;
    loadingThreadId = id;
    
    logger.info('Loading thread', { threadId: id });
    conversationActor.send({ type: 'LOAD_THREAD', threadId: id });
    accessDenied = false;
    
    try {
      const api = getApiClient();
      const thread = await api.getThread(id);
      conversationActor.send({ type: 'THREAD_LOADED', thread });
      
      // Connect to realtime (status handler will update connectionActor)
      connectionActor.send({ type: 'CONNECT', sessionId: id });
      if (realtimeClient instanceof LoomWebSocketClient) {
        await realtimeClient.connect(id);
      }
    } catch (error) {
      logger.error('Failed to load thread', { threadId: id, error: String(error) });
      
      // Check if this is a 403 Forbidden error
      if (error instanceof ApiError && error.isForbidden) {
        accessDenied = true;
        conversationActor.send({ type: 'LOAD_FAILED', error: 'Access denied' });
        return;
      }
      
      conversationActor.send({ type: 'LOAD_FAILED', error: String(error) });
    }
  }
  
  async function loadCurrentUser() {
    try {
      const api = getApiClient();
      currentUser = await api.getCurrentUser();
    } catch (error) {
      logger.warn('Failed to load current user', { error: String(error) });
    }
  }

  function handleLlmEvent(event: LlmEvent) {
    switch (event.type) {
      case 'text_delta':
        conversationActor.send({ type: 'LLM_TEXT_DELTA', content: event.content });
        break;
      case 'tool_call_delta':
        conversationActor.send({
          type: 'LLM_TOOL_CALL_DELTA',
          callId: event.callId,
          toolName: event.toolName,
          argsFragment: event.argsFragment,
        });
        break;
      case 'completed':
        conversationActor.send({ type: 'LLM_COMPLETED', response: event.response });
        break;
      case 'error':
        conversationActor.send({ type: 'LLM_ERROR', error: event.error });
        break;
    }
  }

  function handleToolEvent(event: ToolEvent) {
    switch (event.type) {
      case 'tool_start':
        conversationActor.send({
          type: 'LLM_TOOL_CALL_DELTA',
          callId: event.callId,
          toolName: event.toolName || '',
          argsFragment: '',
        });
        break;
      case 'tool_progress':
        // Update tool progress in state (progress tracking can be added to state machine)
        break;
      case 'tool_done':
        conversationActor.send({
          type: 'TOOL_COMPLETED',
          callId: event.callId,
          outcome: {
            call_id: event.callId,
            success: true,
            result: event.output,
          },
        });
        break;
      case 'tool_error':
        conversationActor.send({
          type: 'TOOL_COMPLETED',
          callId: event.callId,
          outcome: {
            call_id: event.callId,
            success: false,
            error: event.error,
          },
        });
        break;
    }
  }

  function handleSendMessage(content: string) {
    if (!content.trim()) return;
    
    logger.info('Sending message', { threadId, content: content.slice(0, 50) });
    conversationActor.send({ type: 'USER_INPUT', content });
    realtimeClient.sendMessage(content);
  }

  onMount(() => {
    conversationActor.start();
    connectionActor.start();
    
    // Load current user to check for support role
    loadCurrentUser();
    
    if (threadId) {
      loadThread(threadId);
    }

    // Subscribe to realtime events
    const unsubscribeLlm = realtimeClient.onLlmEvent(handleLlmEvent);
    const unsubscribeTool = realtimeClient instanceof LoomWebSocketClient
      ? realtimeClient.onToolEvent(handleToolEvent)
      : undefined;
    
    return () => {
      unsubscribeLlm();
      unsubscribeTool?.();
    };
  });

  onDestroy(() => {
    conversationActor.stop();
    connectionActor.stop();
    realtimeClient.disconnect();
  });

  // Reload when threadId changes (guard against duplicate loads)
  $effect(() => {
    const currentThreadId = conversationState.context.thread?.id;
    if (threadId && threadId !== currentThreadId && threadId !== loadingThreadId) {
      loadThread(threadId);
    }
  });

  const ctx = $derived(conversationState.context);
  const isLoading = $derived(conversationState.value === 'loading');
  const isLoaded = $derived(typeof conversationState.value === 'object' && 'loaded' in conversationState.value);
  const canSend = $derived(ctx.currentAgentState === 'waiting_input');
</script>

<div class="flex flex-col h-full">
  <!-- Header -->
  <header class="border-b border-border p-4 flex items-center justify-between">
    <div class="flex items-center gap-3">
      <h2 class="font-semibold text-fg">
        {#if isLoading}
          <Skeleton width="200px" height="1.5rem" />
        {:else}
          {ctx.thread?.metadata?.title || `Thread ${threadId?.slice(0, 12)}...`}
        {/if}
      </h2>
      {#if isLoaded}
        <AgentStateBadge state={ctx.currentAgentState} />
      {/if}
    </div>
    <ConnectionStatusIndicator
      status={connectionState.value as import('$lib/realtime/types').ConnectionStatus}
      attemptCount={connectionState.context.retries}
      onreconnect={() => threadId && loadThread(threadId)}
    />
  </header>

  {#if isLoading}
    <div class="flex-1 flex items-center justify-center">
      <div class="animate-spin h-8 w-8 border-4 border-accent border-t-transparent rounded-full"></div>
    </div>
  {:else if accessDenied && isSupportUser && threadId}
    <!-- Support user denied access - show request access UI -->
    <SupportAccessDenied 
      threadId={threadId}
      onAccessRequested={() => {
        logger.info('Support access requested, user should wait for approval', { threadId });
      }}
    />
  {:else if conversationState.value === 'error'}
    <div class="flex-1 flex items-center justify-center">
      <Card padding="lg">
        <p class="text-error mb-4">{ctx.error}</p>
        <button
          class="text-accent hover:underline"
          onclick={() => threadId && loadThread(threadId)}
        >
          {i18n._('general.retry')}
        </button>
      </Card>
    </div>
  {:else if isLoaded}
    <!-- Agent state timeline -->
    <div class="p-4 border-b border-border">
      <AgentStateTimeline
        currentState={ctx.currentAgentState}
        retries={ctx.retries}
        pendingToolCalls={ctx.toolExecutions.filter(t => t.status !== 'completed').map(t => t.call_id)}
      />
    </div>

    <!-- Messages -->
    <MessageList
      messages={ctx.messages}
      streamingContent={ctx.streamingContent}
      isStreaming={ctx.currentAgentState === 'thinking' && ctx.streamingContent.length > 0}
    />

    <!-- Tool execution panel -->
    {#if ctx.toolExecutions.length > 0}
      <div class="p-4 border-t border-border">
        <ToolExecutionPanel executions={ctx.toolExecutions} />
      </div>
    {/if}

    <!-- Message input -->
    <MessageInput
      disabled={!canSend}
      onSubmit={handleSendMessage}
    />
  {/if}
</div>
