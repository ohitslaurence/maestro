<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->

<!--
  SupportAccessDenied component
  
  Displayed when a support role user attempts to view a thread they don't have access to.
  Provides a "Request Access" button to initiate the support access workflow.
-->

<script lang="ts">
  import { Card, Button, LoomFrame } from '$lib/ui';
  import { getApiClient } from '$lib/api';
  import { logger } from '$lib/logging';
  import { i18n } from '$lib/i18n';
  import type { SupportAccessRequest } from '$lib/api/types';

  interface Props {
    threadId: string;
    onAccessRequested?: (request: SupportAccessRequest) => void;
  }

  let { threadId, onAccessRequested }: Props = $props();

  let requestState = $state<'idle' | 'loading' | 'requested' | 'error'>('idle');
  let errorMessage = $state<string | null>(null);
  let pendingRequest = $state<SupportAccessRequest | null>(null);

  async function handleRequestAccess() {
    requestState = 'loading';
    errorMessage = null;

    try {
      const api = getApiClient();
      const request = await api.requestSupportAccess(threadId);
      
      pendingRequest = request;
      requestState = 'requested';
      
      logger.info('Support access requested', { threadId, requestId: request.request_id });
      onAccessRequested?.(request);
    } catch (error) {
      requestState = 'error';
      
      if (error instanceof Error) {
        try {
          const parsed = JSON.parse((error as { body?: string }).body || '{}');
          if (parsed.code === 'already_requested') {
            errorMessage = i18n.t('support.access.errorAlreadyRequested');
            requestState = 'requested';
          } else if (parsed.code === 'already_active') {
            errorMessage = i18n.t('support.access.errorAlreadyActive');
          } else {
            errorMessage = parsed.message || i18n.t('support.access.errorFailed');
          }
        } catch {
          errorMessage = i18n.t('support.access.errorFailed');
        }
      } else {
        errorMessage = i18n.t('support.access.errorUnexpected');
      }
      
      logger.error('Failed to request support access', { threadId, error: String(error) });
    }
  }
</script>

<div class="access-denied-wrapper">
  <LoomFrame variant="full">
    <div class="access-denied-content">
      {#if requestState === 'requested'}
        <div class="access-icon access-icon-success">
          <svg fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
        </div>
        <h2 class="access-title">{i18n.t('support.access.requested')}</h2>
        <p class="access-description">
          {i18n.t('support.access.requestSent')}
        </p>
        {#if pendingRequest}
          <p class="access-request-id">
            {i18n.t('support.access.requestId')} {pendingRequest.request_id.slice(0, 8)}...
          </p>
        {/if}
      {:else}
        <div class="access-icon access-icon-warning">
          <svg fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 15v2m0 0v2m0-2h2m-2 0H10m5-6a3 3 0 11-6 0 3 3 0 016 0zm-3 10a9 9 0 100-18 9 9 0 000 18z" />
          </svg>
        </div>
        <h2 class="access-title">{i18n.t('support.access.required')}</h2>
        <p class="access-description">
          {i18n.t('support.access.notShared')}
        </p>
        
        {#if errorMessage}
          <div class="access-error">
            {errorMessage}
          </div>
        {/if}
        
        <Button
          variant="primary"
          onclick={handleRequestAccess}
          disabled={requestState === 'loading'}
        >
          {#if requestState === 'loading'}
            <span class="access-loading">
              <svg class="access-spinner" fill="none" viewBox="0 0 24 24">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
              </svg>
              {i18n.t('support.access.requesting')}
            </span>
          {:else}
            {i18n.t('support.access.requestButton')}
          {/if}
        </Button>
        
        <p class="access-note">
          {i18n.t('support.access.note')}
        </p>
      {/if}
    </div>
  </LoomFrame>
</div>

<style>
  .access-denied-wrapper {
    display: flex;
    flex: 1;
    align-items: center;
    justify-content: center;
    padding: var(--space-8);
  }

  .access-denied-content {
    max-width: 400px;
    text-align: center;
    font-family: var(--font-mono);
  }

  .access-icon {
    margin: 0 auto var(--space-4);
    width: 48px;
    height: 48px;
  }

  .access-icon svg {
    width: 100%;
    height: 100%;
  }

  .access-icon-success {
    color: var(--color-success);
  }

  .access-icon-warning {
    color: var(--color-warning);
  }

  .access-title {
    font-size: var(--text-lg);
    font-weight: 600;
    color: var(--color-fg);
    margin-bottom: var(--space-2);
  }

  .access-description {
    font-size: var(--text-sm);
    color: var(--color-fg-muted);
    margin-bottom: var(--space-4);
    line-height: 1.6;
  }

  .access-request-id {
    font-size: var(--text-xs);
    color: var(--color-fg-subtle);
  }

  .access-error {
    margin-bottom: var(--space-4);
    padding: var(--space-3);
    background: var(--color-error-soft);
    color: var(--color-error);
    border-radius: var(--radius-md);
    font-size: var(--text-sm);
  }

  .access-loading {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .access-spinner {
    width: 16px;
    height: 16px;
    animation: spin 1s linear infinite;
  }

  .access-note {
    margin-top: var(--space-4);
    font-size: var(--text-xs);
    color: var(--color-fg-subtle);
    line-height: 1.5;
  }

  @keyframes spin {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }
</style>
