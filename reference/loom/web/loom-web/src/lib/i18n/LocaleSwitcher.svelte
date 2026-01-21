<script lang="ts">
  import { locales, localeNames, getCurrentLocale, setLocale, isRtl, type Locale } from './i18n';

  let currentLocale = $state<Locale>(getCurrentLocale());

  function updateDocumentDirection(locale: Locale) {
    if (typeof document !== 'undefined') {
      document.documentElement.dir = isRtl(locale) ? 'rtl' : 'ltr';
      document.documentElement.lang = locale;
    }
  }

  function handleChange(event: Event) {
    const target = event.target as HTMLSelectElement;
    const newLocale = target.value as Locale;
    currentLocale = newLocale;
    setLocale(newLocale);
    updateDocumentDirection(newLocale);
  }

  $effect(() => {
    updateDocumentDirection(currentLocale);
  });
</script>

<select
  value={currentLocale}
  onchange={handleChange}
  class="h-9 px-2 rounded-md border border-border bg-bg text-fg text-sm focus:outline-none focus:ring-2 focus:ring-accent"
  dir="auto"
>
  {#each locales as locale}
    <option value={locale} dir={isRtl(locale) ? 'rtl' : 'ltr'}>{localeNames[locale]}</option>
  {/each}
</select>
