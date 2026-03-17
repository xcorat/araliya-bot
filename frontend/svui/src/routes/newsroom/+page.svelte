<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { marked } from 'marked';
	import { RefreshCw, Newspaper, Zap, ExternalLink, ChevronDown, ChevronUp } from '@lucide/svelte';
	import * as api from '$lib/api';
	import {
		initBaseUrl,
		doCheckHealth,
		getMessages,
		getIsHistoryLoading,
		getBaseUrl,
		initSharedSession,
		refreshSharedMessages
	} from '$lib/state.svelte';

	const AGENT_ID = 'newsroom';

	// ── State ────────────────────────────────────────────────────────────────

	let isFetching = $state(false);
	let isUpdating = $state(false);
	let expandedIndex = $state<number | null>(null);
	let pollTimer: ReturnType<typeof setInterval> | null = null;

	interface NewsEvent {
		date: string;
		actor1: string;
		actor2: string;
		event_code: string;
		goldstein: number;
		num_articles: number;
		avg_tone: number;
		source_url: string;
		domain: string;
	}

	let events = $state<NewsEvent[]>([]);

	// ── API calls ────────────────────────────────────────────────────────────

	async function loadEvents() {
		try {
			const resp = await api.sendMessage(getBaseUrl(), 'events', undefined, undefined, AGENT_ID);
			const parsed = JSON.parse(resp.reply);
			if (Array.isArray(parsed)) events = parsed;
		} catch {
			// non-fatal
		}
	}

	async function loadLatest() {
		if (isFetching) return;
		isFetching = true;
		try {
			await api.sendMessage(getBaseUrl(), 'latest', undefined, undefined, AGENT_ID);
			await refreshSharedMessages();
			await loadEvents();
		} catch {
			// non-fatal
		} finally {
			isFetching = false;
		}
	}

	async function triggerUpdate() {
		if (isUpdating) return;
		isUpdating = true;
		expandedIndex = null;
		try {
			await api.sendMessage(getBaseUrl(), 'read', undefined, undefined, AGENT_ID);
			await refreshSharedMessages();
			await loadEvents();
		} catch {
			// non-fatal
		} finally {
			isUpdating = false;
		}
	}

	function startPolling() {
		stopPolling();
		pollTimer = setInterval(() => {
			if (document.visibilityState === 'visible' && !isFetching && !isUpdating) {
				void loadLatest();
			}
		}, 30 * 60 * 1000);
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
		void loadLatest();
		startPolling();
	});

	onDestroy(() => stopPolling());

	// ── Derived ──────────────────────────────────────────────────────────────

	const messages = $derived(getMessages());
	const isHistoryLoading = $derived(getIsHistoryLoading());

	const latestSummary = $derived(
		[...messages].reverse().find((m) => m.role === 'assistant') ?? null
	);

	/** Parse LLM markdown into individual bullet strings */
	function parseBullets(text: string): string[] {
		return text
			.split('\n')
			.filter((line) => /^[\-\*•]\s/.test(line.trim()))
			.map((line) => line.replace(/^[\-\*•]\s+/, '').trim())
			.filter(Boolean);
	}

	const bullets = $derived(latestSummary ? parseBullets(latestSummary.content) : []);

	/** Find events whose actors or event_code appear in a bullet's text */
	function matchedSources(bullet: string): NewsEvent[] {
		const lower = bullet.toLowerCase();
		return events
			.filter((ev) => {
				if (ev.actor1 && lower.includes(ev.actor1.toLowerCase())) return true;
				if (ev.actor2 && lower.includes(ev.actor2.toLowerCase())) return true;
				if (ev.event_code && lower.includes(ev.event_code)) return true;
				return false;
			})
			.slice(0, 6);
	}

	function faviconUrl(domain: string): string {
		return `https://www.google.com/s2/favicons?domain=${encodeURIComponent(domain)}&sz=32`;
	}

	function formatTimestamp(ts: string): string {
		const d = new Date(ts);
		if (Number.isNaN(d.valueOf())) return ts;
		return d.toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
	}
</script>

<svelte:head>
	<title>Araliya — Newsroom</title>
</svelte:head>

<div class="flex h-full min-h-0 flex-1 flex-col overflow-hidden">

	<!-- ── Header ── -->
	<header class="flex items-center justify-between border-b px-4 py-2">
		<div class="flex items-center gap-2">
			<Newspaper class="size-5 shrink-0 text-primary" />
			<h1 class="text-lg font-semibold">Newsroom</h1>
			<span class="text-xs text-muted-foreground">GDELT BigQuery</span>
			{#if latestSummary}
				<span class="text-xs text-muted-foreground">· {formatTimestamp(latestSummary.timestamp)}</span>
			{/if}
		</div>
		<div class="flex items-center gap-2">
			{#if isUpdating}
				<span class="animate-pulse text-xs text-muted-foreground">fetching…</span>
			{/if}
			<button
				onclick={triggerUpdate}
				disabled={isUpdating || isFetching}
				title="Fetch new events from BigQuery"
				class="flex items-center gap-1.5 rounded-md bg-primary px-3 py-1.5 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-40"
			>
				<Zap class="size-3.5 {isUpdating ? 'animate-pulse' : ''}" />
				Update
			</button>
			<button
				onclick={loadLatest}
				disabled={isFetching || isUpdating || isHistoryLoading}
				title="Reload latest stored summary"
				class="rounded p-1 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:opacity-40"
			>
				<RefreshCw class="h-4 w-4 {isFetching || isHistoryLoading ? 'animate-spin' : ''}" />
			</button>
		</div>
	</header>

	<!-- ── Content ── -->
	<div class="flex-1 overflow-y-auto">

		{#if bullets.length > 0}
			<ul class="divide-y">
				{#each bullets as bullet, i}
					{@const sources = matchedSources(bullet)}
					{@const open = expandedIndex === i}
					<li class="group">
						<!-- Card header — always visible -->
						<button
							onclick={() => { expandedIndex = open ? null : i; }}
							class="flex w-full items-start gap-3 px-4 py-3 text-left transition-colors hover:bg-muted/40 {open ? 'bg-muted/30' : ''}"
						>
							<span class="mt-0.5 shrink-0 text-muted-foreground">
								{#if open}
									<ChevronUp class="size-4" />
								{:else}
									<ChevronDown class="size-4" />
								{/if}
							</span>
							<!-- Render the bullet as inline HTML (keeps emoji, bold, etc.) -->
							<span class="min-w-0 flex-1 text-sm leading-snug">
								<!-- eslint-disable-next-line svelte/no-at-html-tags -->
								{@html marked.parseInline(bullet)}
							</span>
						</button>

						<!-- Expanded: matched source links -->
						{#if open}
							<div class="border-t bg-muted/10 px-4 pb-3 pt-2">
								{#if sources.length > 0}
									<ul class="flex flex-col gap-1.5">
										{#each sources as ev (ev.source_url)}
											<li class="flex items-center gap-2">
												<img
													src={faviconUrl(ev.domain)}
													alt={ev.domain}
													width="16"
													height="16"
													class="shrink-0 rounded-sm"
													onerror={(e) => { (e.currentTarget as HTMLImageElement).style.display = 'none'; }}
												/>
												<a
													href={ev.source_url}
													target="_blank"
													rel="noopener noreferrer"
													class="min-w-0 flex-1 truncate text-xs text-primary hover:underline"
												>
													{ev.source_url}
												</a>
												<ExternalLink class="size-3 shrink-0 text-muted-foreground" />
											</li>
										{/each}
									</ul>
								{:else}
									<p class="text-xs text-muted-foreground">No matching sources in current dataset.</p>
								{/if}
							</div>
						{/if}
					</li>
				{/each}
			</ul>

		{:else if isFetching || isUpdating || isHistoryLoading}
			<div class="flex items-center gap-2 px-6 py-8 text-sm text-muted-foreground">
				<RefreshCw class="h-4 w-4 animate-spin" />
				<span>Loading…</span>
			</div>

		{:else}
			<p class="px-6 py-8 text-sm text-muted-foreground">
				No summaries yet — press <strong>Update</strong> to fetch from GDELT.
			</p>
		{/if}

	</div>
</div>
