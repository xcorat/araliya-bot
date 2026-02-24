<script lang="ts">
	import type { ChatMessage } from '$lib/types';
	import { cn } from '$lib/utils';
	import { parseMarkdown } from '$lib/utils/markdown';
	import Badge from '$lib/components/ui/badge/badge.svelte';
	import ToolSteps from './ToolSteps.svelte';

	let { message }: { message: ChatMessage } = $props();

	const isUser = $derived(message.role === 'user');
	const isError = $derived(message.role === 'error');
	const isSystem = $derived(message.role === 'system');
	const hasSteps = $derived(
		message.intermediateSteps && message.intermediateSteps.length > 0
	);

	const time = $derived(() => {
		try {
			return new Date(message.timestamp).toLocaleTimeString([], {
				hour: '2-digit',
				minute: '2-digit'
			});
		} catch {
			return '';
		}
	});
</script>

<div class={cn('flex w-full gap-3 message-enter', isUser ? 'justify-end' : 'justify-start')}>
	{#if !isUser}
		<div
			class={cn(
				'flex size-8 shrink-0 items-center justify-center rounded-full text-xs font-bold',
				isError
					? 'bg-destructive/20 text-destructive'
					: 'bg-primary/20 text-primary'
			)}
		>
			{#if isError}!{:else if isSystem}S{:else}A{/if}
		</div>
	{/if}

	<div
		class={cn(
			'max-w-[80%] rounded-2xl px-4 py-2.5 text-sm leading-relaxed',
			isUser
				? 'rounded-br-md bg-primary text-primary-foreground'
				: isError
					? 'rounded-bl-md border border-destructive/20 bg-destructive/10 text-destructive'
					: 'rounded-bl-md bg-muted text-foreground'
		)}
	>
		{#if hasSteps}
			<div class="mb-2">
				<ToolSteps steps={message.intermediateSteps!} />
			</div>
		{/if}
		{#if isUser}
			<p class="whitespace-pre-wrap break-words">{message.content}</p>
		{:else}
			<div class="prose prose-sm">
				{@html parseMarkdown(message.content)}
			</div>
		{/if}
		<div
			class={cn(
				'mt-1 flex items-center gap-2 text-[10px]',
				isUser ? 'justify-end text-primary-foreground/60' : 'text-muted-foreground'
			)}
		>
			<span>{time()}</span>
			{#if isError}
				<Badge variant="destructive" class="h-4 px-1 text-[9px]">error</Badge>
			{/if}
		</div>
	</div>

	{#if isUser}
		<div
			class="flex size-8 shrink-0 items-center justify-center rounded-full bg-primary text-xs font-bold text-primary-foreground"
		>
			U
		</div>
	{/if}
</div>
