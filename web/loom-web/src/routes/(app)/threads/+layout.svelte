<script lang="ts">
  import type { Snippet } from 'svelte';
  import { page } from '$app/stores';
  import { ThreadListPane } from '$lib/components';
  import { LocaleSwitcher } from '$lib/i18n';
  import { themeStore, Button } from '$lib/ui';
  import { goto } from '$app/navigation';

  interface Props {
    children: Snippet;
  }

  let { children }: Props = $props();

  let selectedThreadId = $derived($page.params.threadId || null);

  function handleSelectThread(threadId: string) {
    goto(`/threads/${threadId}`);
  }

  function handleNewThread() {
    // For now, just navigate to threads list
    // In the future, this could create a new thread
    goto('/threads');
  }
</script>

<div class="flex h-screen bg-bg">
  <!-- Sidebar -->
  <aside class="w-80 border-r border-border flex flex-col bg-bg">
    <header class="p-4 border-b border-border flex items-center justify-between">
      <h1 class="text-xl font-bold text-fg">Loom</h1>
      <div class="flex items-center gap-2">
        <LocaleSwitcher />
        <Button variant="ghost" size="sm" onclick={() => themeStore.toggle()}>
          🌓
        </Button>
      </div>
    </header>
    
    <div class="p-2">
      <Button variant="primary" onclick={handleNewThread} class="w-full">
        + New Thread
      </Button>
    </div>
    
    <div class="flex-1 overflow-hidden">
      <ThreadListPane
        {selectedThreadId}
        onSelectThread={handleSelectThread}
      />
    </div>
  </aside>
  
  <!-- Main content -->
  <main class="flex-1 flex flex-col overflow-hidden">
    {@render children()}
  </main>
</div>
