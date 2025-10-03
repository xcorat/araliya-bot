<script lang="ts">
  import { Copy, Bot } from 'lucide-svelte';
  import { cn, formatTimestamp, copyToClipboard } from '$lib/utils/helpers.js';
  import type { ChatMessage } from '$lib/types/chat.js';
  import { Button } from '$lib/components/ui/button/index.js';

  interface Props {
    message: ChatMessage;
    showAvatar?: boolean;
    showTimestamp?: boolean;
    onCopy?: (content: string) => void;
  }

  let {
    message,
    showAvatar = true,
    showTimestamp = true,
    onCopy
  }: Props = $props();

  let showCopyButton = $state(false);

  async function handleCopy() {
    try {
      await copyToClipboard(message.content);
      onCopy?.(message.content);
    } catch (error) {
      console.error('Failed to copy message:', error);
    }
  }

  const isUser = message.role === 'user';
</script>

<div 
  class={cn(
    'flex w-full gap-3 message-appear',
    isUser ? 'justify-end' : 'justify-start'
  )}
  role="article"
  aria-label={`${isUser ? 'Your' : 'Assistant'} message`}
>
  <!-- Avatar for AI messages -->
  {#if !isUser && showAvatar}
    <div class="flex-shrink-0 mt-1">
      <div class="w-8 h-8 rounded-full bg-accent-primary flex items-center justify-center">
        <Bot class="w-4 h-4 text-white" />
      </div>
    </div>
  {/if}

  <!-- Message content -->
  <div 
    class={cn(
      'relative group max-w-[75%] md:max-w-[85%]',
      isUser && 'max-w-[75%]'
    )}
    onmouseenter={() => showCopyButton = true}
    onmouseleave={() => showCopyButton = false}
    role="group"
    aria-label="Message with actions"
  >
    <!-- Message bubble -->
    <div
      class={cn(
        'px-4 py-3 text-body leading-relaxed break-words',
        isUser
          ? 'bg-primary-bg text-text-light rounded-lg rounded-br-sm shadow-md'
          : 'bg-surface-elevated text-text-primary border border-decorative-border rounded-lg rounded-bl-sm shadow-sm'
      )}
    >
      {message.content}
    </div>

    <!-- Copy button for AI messages -->
    {#if !isUser && showCopyButton}
      <div class="absolute top-2 right-2 opacity-0 group-hover:opacity-100 transition-opacity duration-fast">
        <Button
          variant="ghost"
          size="sm"
          onclick={handleCopy}
          class="h-6 w-6 p-0 hover:bg-accent-tertiary"
          aria-label="Copy message"
        >
          <Copy class="w-3 h-3" />
        </Button>
      </div>
    {/if}

    <!-- Timestamp -->
    {#if showTimestamp}
      <div 
        class={cn(
          'mt-1 text-caption text-text-secondary',
          isUser ? 'text-right' : 'text-left'
        )}
      >
        {formatTimestamp(message.timestamp)}
      </div>
    {/if}

    <!-- Metadata for AI messages -->
    {#if !isUser && message.metadata}
      <div class="mt-1 text-caption text-text-secondary">
        {#if message.metadata.processingTime}
          <span>Processed in {message.metadata.processingTime}ms</span>
        {/if}
        {#if message.metadata.tokens}
          <span class="ml-2">• {message.metadata.tokens} tokens</span>
        {/if}
      </div>
    {/if}
  </div>
</div>
