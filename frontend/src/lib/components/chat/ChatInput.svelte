<script lang="ts">
  import { Send } from 'lucide-svelte';
  import { cn } from '$lib/utils/helpers.js';
  import { Button } from '$lib/components/ui/button/index.js';
  import { Textarea } from '$lib/components/ui/textarea/index.js';

  interface Props {
    onSend: (message: string) => void;
    disabled?: boolean;
    placeholder?: string;
    maxLength?: number;
    autoFocus?: boolean;
  }

  let {
    onSend,
    disabled = false,
    placeholder = 'Type your message here...',
    maxLength = 2000,
    autoFocus = false
  }: Props = $props();

  let message = $state('');

  function handleSubmit() {
    const trimmedMessage = message.trim();
    if (trimmedMessage && !disabled) {
      onSend(trimmedMessage);
      message = '';
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      handleSubmit();
    }
  }

  const canSend = $derived(message.trim().length > 0 && !disabled);
</script>

<div class="sticky bottom-0 bg-surface-elevated border-t border-decorative-border p-4 shadow-lg">
  <div class="max-w-4xl mx-auto">
    <div class="relative flex items-end gap-3">
      <!-- Text input -->
      <div class="flex-1 relative">
        <Textarea
          bind:value={message}
          onkeydown={handleKeydown}
          {placeholder}
          {disabled}
          maxlength={maxLength}
          rows={1}
          class={cn(
            'min-h-[48px] max-h-[120px] px-4 py-3 pr-12',
            'bg-surface border-decorative-border rounded-3xl',
            'text-body placeholder:text-text-secondary',
            'resize-none overflow-y-auto',
            'focus:bg-surface-elevated focus:border-accent-primary',
            'transition-all duration-fast'
          )}
        />

        <!-- Character count -->
        {#if maxLength}
          <div class="absolute bottom-1 right-14 text-caption text-text-secondary">
            {message.length}/{maxLength}
          </div>
        {/if}
      </div>

      <!-- Send button -->
      <div class="flex-shrink-0">
        <Button
          onclick={handleSubmit}
          disabled={!canSend}
          size="icon"
          class={cn(
            'h-12 w-12 rounded-full',
            'gradient-accent hover:shadow-lg hover:scale-105 active:scale-95',
            'transition-all duration-fast'
          )}
          aria-label="Send message"
        >
          <Send class="w-5 h-5" />
        </Button>
      </div>
    </div>

    <!-- Input hints -->
    <div class="mt-2 text-caption text-text-secondary text-center">
      Press <kbd class="px-1 py-0.5 bg-accent-tertiary rounded text-xs">Enter</kbd> to send, 
      <kbd class="px-1 py-0.5 bg-accent-tertiary rounded text-xs">Shift + Enter</kbd> for new line
    </div>
  </div>
</div>
