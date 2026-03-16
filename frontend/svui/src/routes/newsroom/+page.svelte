<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { marked } from 'marked';
	import { RefreshCw, Newspaper } from '@lucide/svelte';
	import * as api from '$lib/api';
	import {
		initBaseUrl,
		doCheckHealth,
		getMessages,
		getIsLoading,
		getIsHistoryLoading,
		getBaseUrl,
		initSharedSession,
		refreshSharedMessages
	} from '$lib/state.svelte';

	const AGENT_ID = 'gdelt_news';
	const REFRESH_INTERVAL_MS = 30 * 60 * 1000; // 30 minutes

	let isFetching = $state(false);
	let pollTimer: ReturnType<typeof setInterval> | null = null;

	// Trigger a fresh GDELT fetch via the buffered endpoint (gdelt_news is not a streaming agent).
	async function triggerFetch() {
		if (isFetching) return;
		isFetching = true;
		try {
			await api.sendMessage(getBaseUrl(), 'read', undefined, undefined, AGENT_ID);
			await refreshSharedMessages();
		} catch {
			// Error is not fatal — show whatever is cached.
		} finally {
			isFetching = false;
		}
	}

	function startPolling() {
		stopPolling();
		pollTimer = setInterval(() => {
			if (document.visibilityState === 'visible' && !isFetching) {
				void triggerFetch();
			}
		}, REFRESH_INTERVAL_MS);
	}

	function stopPolling() {
		if (pollTimer !== null) {
			clearInterval(pollTimer);
			pollTimer = null;
		}
	}

	onMount(async () => {
		initBaseUrl();
		doCheckHealth();
		await initSharedSession(AGENT_ID);
		// If no cached summary yet, trigger the first fetch automatically.
		if (getMessages().length === 0) {
			void triggerFetch();
		}
		startPolling();
	});

	onDestroy(() => {
		stopPolling();
	});

	async function handleRefresh() {
		void triggerFetch();
	}

	const messages = $derived(getMessages());
	const isLoading = $derived(getIsLoading());
	const isHistoryLoading = $derived(getIsHistoryLoading());

	// Show only the latest assistant message — the news briefing.
	const latestNews = $derived(
		[...messages].reverse().find((m) => m.role === 'assistant') ?? null
	);

	const renderedHtml = $derived(
		latestNews ? (marked.parse(latestNews.content) as string) : ''
	);

	function formatTimestamp(ts: string): string {
		const d = new Date(ts);
		if (Number.isNaN(d.valueOf())) return ts;
		return d.toLocaleString([], {
			month: 'short',
			day: 'numeric',
			hour: '2-digit',
			minute: '2-digit'
		});
	}
</script>

<svelte:head>
	<title>Araliya — Newsroom</title>
</svelte:head>

<div class="flex h-full min-h-0 flex-1 flex-col overflow-hidden">
	<!-- Header -->
	<header class="flex items-center justify-between border-b px-4 py-2">
		<div class="flex items-center gap-2">
			<Newspaper class="size-5 shrink-0 text-primary" />
			<h1 class="text-lg font-semibold">Newsroom</h1>
			<span class="text-xs text-muted-foreground">GDELT BigQuery</span>
			{#if latestNews}
				<span class="text-xs text-muted-foreground">
					· updated {formatTimestamp(latestNews.timestamp)}
				</span>
			{/if}
		</div>
		<div class="flex items-center gap-2">
			{#if isFetching}
				<span class="animate-pulse text-xs text-muted-foreground">fetching…</span>
			{/if}
			<button
				onclick={handleRefresh}
				disabled={isFetching || isHistoryLoading}
				title="Refresh news"
				class="rounded p-1 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:opacity-40"
			>
				<RefreshCw class="h-4 w-4 {isFetching || isHistoryLoading ? 'animate-spin' : ''}" />
			</button>
		</div>
	</header>

	<!-- Content -->
	<div class="flex-1 overflow-y-auto px-6 py-4">
		{#if !latestNews && (isFetching || isHistoryLoading)}
			<div class="flex items-center gap-2 text-sm text-muted-foreground">
				<RefreshCw class="h-4 w-4 animate-spin" />
				<span>Fetching latest global events…</span>
			</div>
		{:else if latestNews}
			<article class="prose prose-sm max-w-3xl dark:prose-invert">
				<!-- eslint-disable-next-line svelte/no-at-html-tags -->
				{@html renderedHtml}
			</article>
		{:else}
			<p class="text-sm text-muted-foreground">No news available yet.</p>
		{/if}
	</div>
</div>
