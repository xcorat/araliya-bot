<script lang="ts">
  import { MessageSquare, Trash2 } from 'lucide-svelte';
  import { cn, truncateText, formatTimestamp } from '$lib/utils/helpers.js';
  import type { ChatSession } from '$lib/types/chat.js';
  import { Button } from '$lib/components/ui/button/index.js';

  interface Props {
    session: ChatSession;
    isActive: boolean;
    onClick: (sessionId: string) => void;
    onDelete?: (sessionId: string) => void;
  }

  let {
    session,
    isActive,
    onClick,
    onDelete
  }: Props = $props();

  let showDeleteButton = $state(false);

  function handleClick() {
    onClick(session.id);
  }

  function handleDelete(event: MouseEvent) {
    event.stopPropagation();
    onDelete?.(session.id);
  }
</script>

<div
  class={cn(
    'group relative flex items-center gap-3 p-3 mx-2 rounded-md cursor-pointer',
    'transition-all duration-fast hover:bg-accent-tertiary',
    isActive && 'bg-accent-tertiary border-l-3 border-l-accent-primary'
  )}
  onclick={handleClick}
  onmouseenter={() => showDeleteButton = true}
  onmouseleave={() => showDeleteButton = false}
  role="button"
  tabindex="0"
  aria-label={`Chat session: ${session.title}`}
  onkeydown={(e) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      handleClick();
    }
  }}
>
  <!-- Session icon -->
  <div class="flex-shrink-0">
    <MessageSquare class="w-4 h-4 text-text-secondary" />
  </div>

  <!-- Session content -->
  <div class="flex-1 min-w-0">
    <div class="flex items-center justify-between">
      <h3 class={cn(
        'text-body font-medium truncate',
        isActive ? 'text-text-primary' : 'text-text-primary'
      )}>
        {truncateText(session.title, 30)}
      </h3>
      
      {#if showDeleteButton && onDelete}
        <Button
          variant="ghost"
          size="sm"
          onclick={handleDelete}
          class="opacity-0 group-hover:opacity-100 h-6 w-6 p-0 hover:bg-semantic-error/10 hover:text-semantic-error"
          aria-label="Delete session"
        >
          <Trash2 class="w-3 h-3" />
        </Button>
      {/if}
    </div>

    <!-- Last message preview -->
    {#if session.lastMessage}
      <p class="text-body-small text-text-secondary truncate mt-1">
        {truncateText(session.lastMessage, 40)}
      </p>
    {/if}

    <!-- Session metadata -->
    <div class="flex items-center justify-between mt-1">
      <span class="text-caption text-text-secondary">
        {session.messageCount} message{session.messageCount !== 1 ? 's' : ''}
      </span>
      <span class="text-caption text-text-secondary">
        {formatTimestamp(session.updatedAt)}
      </span>
    </div>
  </div>

  <!-- Active indicator -->
  {#if isActive}
    <div class="absolute left-0 top-0 bottom-0 w-1 bg-gradient-accent rounded-r"></div>
  {/if}
</div>
