<script lang="ts">
  import { onMount, type Snippet } from 'svelte';
  import { loadCatalog, getPreferredLocale, getCurrentLocale, isRtl, type Locale } from './i18n';

  interface Props {
    children: Snippet;
  }

  let { children }: Props = $props();
  let loaded = $state(false);

  function updateDocumentDirection() {
    const locale = getCurrentLocale();
    document.documentElement.dir = isRtl(locale) ? 'rtl' : 'ltr';
    document.documentElement.lang = locale;
  }

  onMount(async () => {
    const locale = getPreferredLocale();
    await loadCatalog(locale);
    updateDocumentDirection();
    loaded = true;
  });

  $effect(() => {
    if (loaded) {
      updateDocumentDirection();
    }
  });
</script>

{#if loaded}
  {@render children()}
{:else}
  <div class="flex items-center justify-center h-screen">
    <div class="animate-spin h-8 w-8 border-4 border-accent border-t-transparent rounded-full"></div>
  </div>
{/if}
