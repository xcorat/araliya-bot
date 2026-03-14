<script lang="ts">
	import Button from '$lib/components/ui/button/button.svelte';
	import {
		getIsLoading,
		doSendMessageStreaming,
		getLastUsage,
		getLastTiming,
		getStreamElapsedMs,
		getAgentSpend
	} from '$lib/state.svelte';
	import { SendHorizontal, Loader2 } from '@lucide/svelte';

	let inputText = $state('');
	let textareaEl = $state<HTMLTextAreaElement | null>(null);

	const loading = $derived(getIsLoading());
	const canSend = $derived(inputText.trim().length > 0 && !loading);

	const lastUsage = $derived(getLastUsage());
	const lastTiming = $derived(getLastTiming());
	const streamElapsedMs = $derived(getStreamElapsedMs());
	const spend = $derived(getAgentSpend());

	const isStreaming = $derived(streamElapsedMs !== null);

	const liveElapsed = $derived(
		streamElapsedMs !== null ? `${(streamElapsedMs / 1000).toFixed(1)}s` : null
	);

	const lastTurnLabel = $derived((() => {
		if (!lastTiming || !lastUsage) return null;
		const secs = (lastTiming.total_ms / 1000).toFixed(1);
		const tok = lastUsage.total_tokens ?? ((lastUsage.prompt_tokens ?? 0) + (lastUsage.completion_tokens ?? 0));
		return `${secs}s · ${tok} tok`;
	})());

	function formatCost(usd: number | null | undefined) {
		if (usd == null || usd === 0) return '$0.00';
		return usd < 0.01 ? '<$0.01' : `$${usd.toFixed(2)}`;
	}

	function formatTokens(n: number) {
		if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
		if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
		return `${n}`;
	}

	function autoResize() {
		if (textareaEl) {
			textareaEl.style.height = 'auto';
			textareaEl.style.height = Math.min(textareaEl.scrollHeight, 200) + 'px';
		}
	}

	async function handleSend() {
		if (!canSend) return;
		const text = inputText;
		inputText = '';
		if (textareaEl) {
			textareaEl.style.height = 'auto';
		}
		await doSendMessageStreaming(text);
		textareaEl?.focus();
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter' && !e.shiftKey) {
			e.preventDefault();
			handleSend();
		}
	}
</script>

<div class="flex flex-col border-t bg-background/80 backdrop-blur-sm">
	<!-- Input row -->
	<div class="px-4 py-3">
		<div class="mx-auto flex max-w-3xl items-end gap-2">
			<textarea
				bind:this={textareaEl}
				bind:value={inputText}
				oninput={autoResize}
				onkeydown={handleKeydown}
				placeholder="Type a message... (Enter to send, Shift+Enter for newline)"
				disabled={loading}
				rows={1}
				class="flex-1 resize-none rounded-xl border bg-muted/50 px-4 py-2.5 text-sm outline-none transition-colors placeholder:text-muted-foreground/60 focus:border-ring focus:ring-1 focus:ring-ring disabled:opacity-50 {loading ? 'opacity-60' : ''}"
			></textarea>
			<Button size="icon" disabled={!canSend} onclick={handleSend} class="shrink-0 rounded-xl transition-opacity">
				{#if loading}
					<Loader2 class="size-4 spin" />
				{:else}
					<SendHorizontal class="size-4" />
				{/if}
			</Button>
		</div>
	</div>

	<!-- Status bar -->
	<div class="status-bar border-t px-4 py-1">
		<div class="mx-auto flex max-w-3xl items-center justify-between gap-2 text-[11px] tabular-nums text-muted-foreground/70">
			<span class="flex items-center gap-2">
				{#if isStreaming && liveElapsed}
					<span class="animate-pulse font-medium text-yellow-600 dark:text-yellow-400">⏱ {liveElapsed}</span>
				{:else if lastTurnLabel}
					<span title="Last turn: time · tokens">{lastTurnLabel}</span>
				{:else}
					<span class="opacity-40">ready</span>
				{/if}
			</span>
			{#if spend}
				<span class="opacity-60" title="Session totals (from spend.json)">{formatTokens(spend.total_input_tokens + spend.total_output_tokens + spend.total_cached_tokens)} tok · {formatCost(spend.total_cost_usd)}</span>
			{/if}
		</div>
	</div>
</div>

<style>
	.status-bar {
		background: color-mix(in srgb, var(--color-muted) 30%, transparent);
		border-color: color-mix(in srgb, var(--color-border) 50%, transparent);
	}
</style>
