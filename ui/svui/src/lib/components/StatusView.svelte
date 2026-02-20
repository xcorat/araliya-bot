<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import Button from '$lib/components/ui/button/button.svelte';
	import Badge from '$lib/components/ui/badge/badge.svelte';
	import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card';
	import { ScrollArea } from '$lib/components/ui/scroll-area';
	import { RefreshCw, Server, Brain, FileText } from '@lucide/svelte';
	import { getBaseUrl, getSessionId } from '$lib/state.svelte';
	import * as api from '$lib/api';
	import type { SessionFileInfo, HealthResponse } from '$lib/types';

	let memoryContent = $state('');
	let files = $state<SessionFileInfo[]>([]);
	let serviceInfo = $state<HealthResponse | null>(null);
	let isPolling = $state(true);
	let loading = $state(false);
	let error = $state('');
	let lastRefresh = $state('');
	let pollTimer: ReturnType<typeof setInterval> | null = null;

	const POLL_INTERVAL_MS = 4000;

	const sessionId = $derived(getSessionId());

	onMount(() => {
		void fetchAll();
		startPolling();
	});

	onDestroy(() => {
		stopPolling();
	});

	function startPolling() {
		stopPolling();
		if (isPolling) {
			pollTimer = setInterval(() => {
				void fetchAll();
			}, POLL_INTERVAL_MS);
		}
	}

	function stopPolling() {
		if (pollTimer) {
			clearInterval(pollTimer);
			pollTimer = null;
		}
	}

	function togglePolling() {
		isPolling = !isPolling;
		if (isPolling) {
			startPolling();
			void fetchAll();
		} else {
			stopPolling();
		}
	}

	async function fetchAll() {
		if (loading) return;
		const baseUrl = getBaseUrl();
		if (!baseUrl) return;

		loading = true;
		error = '';
		try {
			if (sessionId) {
				const [memRes, filesRes, healthRes] = await Promise.all([
					api.getSessionMemory(baseUrl, sessionId),
					api.getSessionFiles(baseUrl, sessionId),
					api.checkHealth(baseUrl)
				]);
				memoryContent = memRes.content;
				files = filesRes.files;
				serviceInfo = healthRes;
			} else {
				const healthRes = await api.checkHealth(baseUrl);
				serviceInfo = healthRes;
			}
			lastRefresh = new Date().toLocaleTimeString();
		} catch (e: unknown) {
			error = e instanceof Error ? e.message : 'Failed to fetch status data';
		} finally {
			loading = false;
		}
	}

	function formatBytes(bytes: number): string {
		if (bytes === 0) return '0 B';
		const units = ['B', 'KB', 'MB', 'GB'];
		const i = Math.floor(Math.log(bytes) / Math.log(1024));
		const value = bytes / Math.pow(1024, i);
		return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[i]}`;
	}

	function formatTime(iso: string): string {
		if (!iso) return '—';
		try {
			return new Date(iso).toLocaleString();
		} catch {
			return iso;
		}
	}
</script>

<div class="flex-1 space-y-4 overflow-auto p-4">
	<!-- Toolbar -->
	<div class="flex items-center justify-between">
		<h2 class="text-sm font-semibold">System Status</h2>
		<div class="flex items-center gap-2">
			{#if lastRefresh}
				<span class="hidden text-[10px] text-muted-foreground sm:inline">
					{lastRefresh}
				</span>
			{/if}
			<Button
				variant={isPolling ? 'secondary' : 'ghost'}
				size="sm"
				onclick={togglePolling}
				title={isPolling ? 'Auto-refresh ON' : 'Auto-refresh OFF'}
				class="h-7 gap-1.5 px-2 text-xs"
			>
				<RefreshCw
					class="size-3 {isPolling ? 'animate-spin' : ''}"
					style={isPolling ? 'animation-duration: 4s' : ''}
				/>
				{isPolling ? 'Live' : 'Paused'}
			</Button>
			<Button
				variant="ghost"
				size="sm"
				onclick={() => fetchAll()}
				title="Refresh now"
				disabled={loading}
				class="size-7 p-0"
			>
				<RefreshCw class="size-3.5" />
			</Button>
		</div>
	</div>

	{#if error}
		<div
			class="rounded-lg border border-destructive/50 bg-destructive/10 p-4 text-sm text-destructive"
		>
			{error}
		</div>
	{/if}

	<!-- Service Status -->
	{#if serviceInfo}
		<Card>
			<CardHeader class="pb-3">
				<CardTitle class="flex items-center gap-2 text-sm font-medium">
					<Server class="size-4 text-primary" />
					Service Status
				</CardTitle>
			</CardHeader>
			<CardContent>
				<div class="grid grid-cols-1 gap-4 text-sm sm:grid-cols-2 lg:grid-cols-3">
					<div>
						<div class="mb-1 text-xs text-muted-foreground">Status</div>
						<div class="flex items-center gap-2">
							<span class="size-2 rounded-full bg-emerald-500"></span>
							<span class="font-medium capitalize">{serviceInfo.status}</span>
						</div>
					</div>
					<div>
						<div class="mb-1 text-xs text-muted-foreground">Bot ID</div>
						<div class="truncate font-mono text-xs">{serviceInfo.bot_id ? serviceInfo.bot_id.slice(0, 16) + '...' : '—'}</div>
					</div>
					<div>
						<div class="mb-1 text-xs text-muted-foreground">Sessions</div>
						<div class="font-medium">{serviceInfo.session_count ?? 0}</div>
					</div>
					<div>
						<div class="mb-1 text-xs text-muted-foreground">LLM Provider</div>
						<div class="font-medium capitalize">{serviceInfo.llm_provider ?? '—'}</div>
					</div>
					<div>
						<div class="mb-1 text-xs text-muted-foreground">Model</div>
						<div class="font-mono text-xs">{serviceInfo.llm_model ?? '—'}</div>
					</div>
					<div>
						<div class="mb-1 text-xs text-muted-foreground">Timeout</div>
						<div class="font-medium">{serviceInfo.llm_timeout_seconds ?? '—'}s</div>
					</div>
					<div class="sm:col-span-2 lg:col-span-3">
						<div class="mb-1 text-xs text-muted-foreground">
							Enabled Tools ({serviceInfo.enabled_tools?.length ?? 0})
						</div>
						<div class="flex flex-wrap gap-1.5">
							{#each serviceInfo.enabled_tools ?? [] as tool}
								<Badge variant="outline" class="font-mono text-[10px]">{tool}</Badge>
							{/each}
							<Badge variant="secondary" class="text-[10px]"
								>max {serviceInfo.max_tool_rounds ?? 0} rounds</Badge
							>
						</div>
					</div>
				</div>
			</CardContent>
		</Card>
	{/if}

	<!-- Session-specific cards -->
	{#if !sessionId}
		<div class="flex items-center justify-center py-8 text-muted-foreground">
			<p>No session selected. Send a message first to see session details.</p>
		</div>
	{:else}
		<!-- Working Memory -->
		<Card>
			<CardHeader class="pb-3">
				<CardTitle class="flex items-center gap-2 text-sm font-medium">
					<Brain class="size-4 text-primary" />
					Working Memory
				</CardTitle>
			</CardHeader>
			<CardContent>
				{#if memoryContent}
					<ScrollArea class="max-h-[50vh]">
						<pre
							class="whitespace-pre-wrap font-mono text-xs leading-relaxed text-foreground/90">{memoryContent}</pre>
					</ScrollArea>
				{:else}
					<p class="text-sm italic text-muted-foreground">
						No working memory yet for this session.
					</p>
				{/if}
			</CardContent>
		</Card>

		<!-- Session Files -->
		<Card>
			<CardHeader class="pb-3">
				<CardTitle class="flex items-center gap-2 text-sm font-medium">
					<FileText class="size-4 text-primary" />
					Session Files
				</CardTitle>
			</CardHeader>
			<CardContent>
				{#if files.length > 0}
					<div class="space-y-1">
						{#each files as file}
							<div
								class="flex items-center justify-between rounded-md px-3 py-2 text-xs transition-colors hover:bg-muted/50"
							>
								<span class="mr-4 flex-1 truncate font-mono">{file.name}</span>
								<div class="flex shrink-0 items-center gap-4 text-muted-foreground">
									<span>{formatBytes(file.size_bytes)}</span>
									<span class="hidden sm:inline">{formatTime(file.modified)}</span>
								</div>
							</div>
						{/each}
					</div>
				{:else}
					<p class="text-sm italic text-muted-foreground">
						No files found for this session.
					</p>
				{/if}
			</CardContent>
		</Card>
	{/if}
</div>
