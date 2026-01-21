<script lang="ts">
  import '../app.css';
  import { ThemeProvider } from '$lib/ui';
  import { I18nProvider } from '$lib/i18n';
  import type { Snippet } from 'svelte';

  interface Props {
    children: Snippet;
  }

  let { children }: Props = $props();
</script>

<ThemeProvider>
  <I18nProvider>
    <div class="app-root">
      {@render children()}
    </div>
  </I18nProvider>
</ThemeProvider>

<style>
  .app-root {
    min-height: 100vh;
    background: var(--color-bg);
    color: var(--color-fg);
    font-family: var(--font-mono);
  }
</style>
