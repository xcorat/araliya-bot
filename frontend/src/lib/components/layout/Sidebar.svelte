<script lang="ts">
  import { Plus, MessageSquare, X } from 'lucide-svelte';
  import { cn } from '$lib/utils/helpers.js';
  import type { ChatSession } from '$lib/types/chat.js';
  import { Button } from '$lib/components/ui/button/index.js';
  import SessionListItem from '$lib/components/session/SessionListItem.svelte';

  interface Props {
    sessions: ChatSession[];
    activeSessionId: string | null;
    isOpen: boolean;
    onNewChat: () => void;
    onSessionSelect: (sessionId: string) => void;
    onSessionDelete: (sessionId: string) => void;
    onClose: () => void;
  }

  let {
    sessions,
    activeSessionId,
    isOpen,
    onNewChat,
    onSessionSelect,
    onSessionDelete,
    onClose
  }: Props = $props();
</script>

<!-- Overlay for mobile -->
{#if isOpen}
  <div 
    class="fixed inset-0 bg-black/50 z-40 md:hidden"
    onclick={onClose}
    onkeydown={(e) => {
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        onClose();
      }
    }}
    role="button"
    tabindex="0"
    aria-label="Close sidebar"
  ></div>
{/if}

<!-- Sidebar -->
<aside
  class={cn(
    'fixed left-0 top-0 z-50 h-full w-80 bg-surface border-r border-decorative-border',
    'transform transition-transform duration-base ease-out',
    'md:relative md:translate-x-0 md:z-auto',
    isOpen ? 'translate-x-0 sidebar-appear' : '-translate-x-full'
  )}
  role="navigation"
  aria-label="Chat sessions"
>
  <div class="flex flex-col h-full">
    <!-- Header -->
    <div class="flex items-center justify-between p-4 border-b border-decorative-border">
      <h2 class="text-heading font-semibold text-text-primary">Araliya</h2>
      
      <!-- Close button (mobile only) -->
      <Button
        variant="ghost"
        size="sm"
        onclick={onClose}
        class="md:hidden h-8 w-8 p-0"
        aria-label="Close sidebar"
      >
        <X class="w-4 h-4" />
      </Button>
    </div>

    <!-- New Chat Button -->
    <div class="p-4">
      <Button
        variant="outline"
        onclick={onNewChat}
        class="w-full justify-start gap-2 h-10"
      >
        <Plus class="w-4 h-4" />
        New Chat
      </Button>
    </div>

    <!-- Sessions List -->
    <div class="flex-1 overflow-y-auto px-2">
      {#if sessions.length === 0}
        <div class="flex flex-col items-center justify-center h-32 text-center px-4">
          <MessageSquare class="w-8 h-8 text-text-secondary mb-2" />
          <p class="text-body text-text-secondary">No conversations yet</p>
          <p class="text-body-small text-text-secondary mt-1">Start a new chat to begin</p>
        </div>
      {:else}
        <div class="space-y-1 pb-4">
          {#each sessions as session (session.id)}
            <SessionListItem
              {session}
              isActive={session.id === activeSessionId}
              onClick={onSessionSelect}
              onDelete={onSessionDelete}
            />
          {/each}
        </div>
      {/if}
    </div>

    <!-- Footer -->
    <div class="p-4 border-t border-decorative-border">
      <div class="text-caption text-text-secondary text-center">
        <p>Araliya Bot v1.0</p>
        <p class="mt-1">Powered by Graph-RAG</p>
      </div>
    </div>
  </div>
</aside>
