<script lang="ts">
	import type { ChatMessage as ChatMessageType } from '$lib/types';
	import ChatMessage from './ChatMessage.svelte';
	import { getIsLoading } from '$lib/state.svelte';

	let { messages }: { messages: ChatMessageType[] } = $props();

	let scrollContainer = $state<HTMLDivElement | null>(null);

	$effect(() => {
		messages.length;
		if (scrollContainer) {
			requestAnimationFrame(() => {
				if (scrollContainer) {
					scrollContainer.scrollTop = scrollContainer.scrollHeight;
				}
			});
		}
	});
</script>

<div bind:this={scrollContainer} class="flex-1 space-y-4 overflow-y-auto px-4 py-4">
	{#if messages.length === 0}
		<div
			class="flex h-full flex-col items-center justify-center gap-3 text-muted-foreground"
		>
			<div class="text-4xl">🌺</div>
			<p class="text-lg font-medium">Araliya</p>
			<p class="text-sm">Send a message to begin</p>
		</div>
	{:else}
		{#each messages as message (message.id)}
			<ChatMessage {message} />
		{/each}

		{#if getIsLoading()}
			<div class="message-enter flex justify-start gap-3">
				<div
					class="flex size-8 shrink-0 items-center justify-center rounded-full bg-primary/20 text-xs font-bold text-primary"
				>
					A
				</div>
				<div class="rounded-2xl rounded-bl-md bg-muted px-4 py-3">
					<div class="flex items-center gap-1.5">
						<span class="dot-bounce size-2 rounded-full bg-primary/60"></span>
						<span class="dot-bounce size-2 rounded-full bg-primary/60"></span>
						<span class="dot-bounce size-2 rounded-full bg-primary/60"></span>
						<span class="ml-1 text-[11px] text-muted-foreground/60">Thinking…</span>
					</div>
				</div>
			</div>
		{/if}
	{/if}
</div>
