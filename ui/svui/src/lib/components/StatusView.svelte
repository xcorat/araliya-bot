<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import Button from '$lib/components/ui/button/button.svelte';
	import Badge from '$lib/components/ui/badge/badge.svelte';
	import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card';
	import { RefreshCw, Server, Cpu, ChevronDown, ChevronUp } from '@lucide/svelte';
	import { getBaseUrl } from '$lib/state.svelte';
	import * as api from '$lib/api';
	import type { HealthResponse, MainProcessStatus, SubsystemStatus } from '$lib/types';

	let serviceInfo = $state<HealthResponse | null>(null);
	let expandedSubsystems = $state<Record<string, boolean>>({});
	let isPolling = $state(true);
	let loading = $state(false);
	let error = $state('');
	let lastRefresh = $state('');
	let pollTimer: ReturnType<typeof setInterval> | null = null;

	const POLL_INTERVAL_MS = 4000;

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
			const healthRes = await api.checkHealth(baseUrl);
			serviceInfo = healthRes;
			lastRefresh = new Date().toLocaleTimeString();
		} catch (e: unknown) {
			error = e instanceof Error ? e.message : 'Failed to fetch status data';
		} finally {
			loading = false;
		}
	}

	function formatUptime(ms: number | undefined): string {
		if (!ms || ms < 0) return '—';
		const totalSeconds = Math.floor(ms / 1000);
		const hours = Math.floor(totalSeconds / 3600);
		const minutes = Math.floor((totalSeconds % 3600) / 60);
		const seconds = totalSeconds % 60;
		return `${hours}h ${minutes}m ${seconds}s`;
	}

	function mainProcess(info: HealthResponse): MainProcessStatus {
		return (
			info.main_process ?? {
				id: 'supervisor',
				name: 'Supervisor',
				status: info.status ?? 'unknown',
				uptime_ms: info.uptime_ms ?? 0,
				details: {
					bot_id: info.bot_id,
					llm_provider: info.llm_provider,
					llm_model: info.llm_model,
					llm_timeout_seconds: info.llm_timeout_seconds
				}
			}
		);
	}

	function subsystems(info: HealthResponse): SubsystemStatus[] {
		return info.subsystems ?? [];
	}

	function statusDotClass(status: string): string {
		const normalized = status.toLowerCase();
		if (normalized === 'ok' || normalized === 'running') return 'bg-emerald-500';
		if (normalized === 'degraded' || normalized === 'warning') return 'bg-yellow-500';
		if (normalized === 'error' || normalized === 'failed') return 'bg-destructive';
		return 'bg-muted-foreground';
	}

	function statusPillClass(status: string): string {
		const normalized = status.toLowerCase();
		if (normalized === 'ok' || normalized === 'running') {
			return 'border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300';
		}
		if (normalized === 'degraded' || normalized === 'warning') {
			return 'border-yellow-500/30 bg-yellow-500/10 text-yellow-700 dark:text-yellow-300';
		}
		if (normalized === 'error' || normalized === 'failed') {
			return 'border-destructive/30 bg-destructive/10 text-destructive';
		}
		return 'border-muted-foreground/25 bg-muted/40 text-muted-foreground';
	}

	function formatDetailLabel(key: string): string {
		return key
			.replace(/_/g, ' ')
			.replace(/([a-z0-9])([A-Z])/g, '$1 $2')
			.replace(/\s+/g, ' ')
			.trim();
	}

	function formatDetailValue(value: unknown): string {
		if (value === null || value === undefined) return '—';
		if (typeof value === 'string') return value;
		if (typeof value === 'number' || typeof value === 'boolean') return String(value);
		try {
			return JSON.stringify(value);
		} catch {
			return '—';
		}
	}

	function detailEntries(details?: Record<string, unknown>): [string, unknown][] {
		if (!details) return [];
		return Object.entries(details);
	}

	function toggleSubsystem(id: string) {
		expandedSubsystems = {
			...expandedSubsystems,
			[id]: !expandedSubsystems[id]
		};
	}

	function isExpanded(id: string): boolean {
		return !!expandedSubsystems[id];
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
		<Card class="border-primary/20">
			<CardHeader class="pb-3">
				<CardTitle class="flex items-center gap-2 text-sm font-medium">
					<Server class="size-4 text-primary" />
					Main Process
				</CardTitle>
			</CardHeader>
			<CardContent>
				{@const process = mainProcess(serviceInfo)}
				<div class="grid grid-cols-1 gap-4 text-sm sm:grid-cols-2 lg:grid-cols-4">
					<div>
						<div class="mb-1 text-xs text-muted-foreground">Status</div>
						<div class="flex items-center gap-2">
							<span class={`size-2 rounded-full ${statusDotClass(process.status)}`}></span>
							<span class="font-medium capitalize">{process.status}</span>
						</div>
					</div>
					<div>
						<div class="mb-1 text-xs text-muted-foreground">Process</div>
						<div class="font-medium">{process.name}</div>
					</div>
					<div>
						<div class="mb-1 text-xs text-muted-foreground">Uptime</div>
						<div class="font-medium">{formatUptime(process.uptime_ms)}</div>
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
					<div>
						<div class="mb-1 text-xs text-muted-foreground">Bot ID</div>
						<div class="truncate font-mono text-xs">{serviceInfo.bot_id ? serviceInfo.bot_id.slice(0, 16) + '...' : '—'}</div>
					</div>
					<div>
						<div class="mb-1 text-xs text-muted-foreground">Subsystems</div>
						<div class="font-medium">{subsystems(serviceInfo).length}</div>
					</div>
					<div class="sm:col-span-2 lg:col-span-4">
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

		<Card>
			<CardHeader class="pb-3">
				<CardTitle class="flex items-center gap-2 text-sm font-medium">
					<Cpu class="size-4 text-primary" />
					Subsystems
				</CardTitle>
			</CardHeader>
			<CardContent>
				{@const subsystemItems = subsystems(serviceInfo)}
				{#if subsystemItems.length === 0}
					<p class="text-sm text-muted-foreground">No subsystem details reported.</p>
				{:else}
					<div class="space-y-2">
						{#each subsystemItems as subsystem (subsystem.id)}
							<div class="overflow-hidden rounded-lg border border-border/70 bg-card/80 shadow-sm">
								<button
									type="button"
									onclick={() => toggleSubsystem(subsystem.id)}
									class="flex w-full items-center justify-between bg-muted/20 px-3 py-2.5 text-left transition-colors hover:bg-muted/35"
								>
									<div class="flex items-center gap-2">
										<span class={`size-2 rounded-full ${statusDotClass(subsystem.status)}`}></span>
										<div>
											<div class="text-sm font-medium">{subsystem.name}</div>
											<div class="text-xs text-muted-foreground">{subsystem.id}</div>
										</div>
									</div>
									<div class="flex items-center gap-2">
										<Badge
											variant="outline"
											class={`text-[10px] capitalize ${statusPillClass(subsystem.status)}`}
										>
											{subsystem.status}
										</Badge>
										{#if isExpanded(subsystem.id)}
											<ChevronUp class="size-4 text-muted-foreground" />
										{:else}
											<ChevronDown class="size-4 text-muted-foreground" />
										{/if}
									</div>
								</button>
								{#if isExpanded(subsystem.id)}
									<div class="space-y-2 border-t bg-muted/10 px-3 py-2.5 text-xs">
										<div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
											<div>
												<div class="mb-0.5 text-muted-foreground">State</div>
												<div class="rounded-md border border-border/50 bg-background/60 px-2 py-1 capitalize">
													{subsystem.state ?? '—'}
												</div>
											</div>
											<div>
												<div class="mb-0.5 text-muted-foreground">Status</div>
												<div class="rounded-md border border-border/50 bg-background/60 px-2 py-1 capitalize">
													{subsystem.status}
												</div>
											</div>
										</div>
										{@const details = detailEntries(subsystem.details)}
										{#if details.length > 0}
											<div class="space-y-1.5">
												<div class="text-[10px] uppercase tracking-wide text-muted-foreground">Details</div>
												<div class="grid grid-cols-1 gap-1.5 sm:grid-cols-2">
													{#each details as [key, value]}
														<div class="rounded-md border border-border/50 bg-background/60 px-2 py-1.5">
															<div class="mb-0.5 text-[10px] text-muted-foreground">{formatDetailLabel(key)}</div>
															<div class="break-all font-mono text-[10px] text-foreground/90">
																{formatDetailValue(value)}
															</div>
														</div>
													{/each}
												</div>
											</div>
										{/if}
									</div>
								{/if}
							</div>
						{/each}
					</div>
				{/if}
			</CardContent>
		</Card>
	{/if}
</div>
