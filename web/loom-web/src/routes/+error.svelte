<script lang="ts">
  import { page } from '$app/stores';
  import { Button, LoomFrame } from '$lib/ui';
  import { i18n } from '$lib/i18n';
</script>

<div class="error-container">
  <LoomFrame variant="full">
    <div class="error-content">
      <h1 class="error-code">
        {$page.status}
      </h1>
      <p class="error-message">
        {$page.error?.message || i18n.t('general.error')}
      </p>
      <Button variant="primary" onclick={() => window.location.href = '/'}>
        {i18n.t('error.goHome')}
      </Button>
    </div>
  </LoomFrame>
</div>

<style>
  .error-container {
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 100vh;
    background: var(--color-bg);
    padding: var(--space-4);
  }

  .error-content {
    text-align: center;
    min-width: 280px;
  }

  .error-code {
    font-size: var(--text-2xl);
    font-weight: 600;
    color: var(--color-error);
    margin-bottom: var(--space-2);
    font-family: var(--font-mono);
  }

  .error-message {
    color: var(--color-fg-muted);
    margin-bottom: var(--space-6);
    font-size: var(--text-base);
    font-family: var(--font-mono);
  }
</style>
