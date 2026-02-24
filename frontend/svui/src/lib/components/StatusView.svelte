<script lang="ts">
	import Button from '$lib/components/ui/button/button.svelte';
	import Badge from '$lib/components/ui/badge/badge.svelte';
	import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card';
	import { RefreshCw, Cpu, ChevronDown, ChevronUp } from '@lucide/svelte';
	import type { HealthResponse } from '$lib/types';
	import {
		subsystems,
		statusDotClass,
		statusPillClass,
		subsystemKind,
		subsystemCardClass,
		subsystemHeaderClass,
		formatDetailLabel,
		formatDetailValue,
		detailEntries,
		pickDetail,
		detailList,
		detailFlag,
		detailCount
	} from '$lib/utils/status';

	let {
		serviceInfo,
		error = '',
		loading = false,
		lastRefresh = '',
		isPolling = true,
		onTogglePolling,
		onRefresh
	}: {
		serviceInfo: HealthResponse | null;
		error?: string;
		loading?: boolean;
		lastRefresh?: string;
		isPolling?: boolean;
		onTogglePolling: () => void;
		onRefresh: () => void;
	} = $props();

	let expandedSubsystems = $state<Record<string, boolean>>({});

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
				onclick={onTogglePolling}
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
				onclick={onRefresh}
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

	{#if serviceInfo}
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
					<div class="grid grid-cols-1 gap-2 sm:grid-cols-2 lg:grid-cols-3">
						{#each subsystemItems as subsystem (subsystem.id)}
							<div
								class={`overflow-hidden rounded-lg border shadow-sm ${subsystemCardClass(subsystem.id)}`}
							>
								<button
									type="button"
									onclick={() => toggleSubsystem(subsystem.id)}
									class={`flex w-full items-center justify-between px-2.5 py-2 text-left transition-colors ${subsystemHeaderClass(subsystem.id)}`}
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
									{@const kind = subsystemKind(subsystem.id)}
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
										{#if kind === 'agents'}
											<div class="rounded-md border border-border/50 bg-background/60 p-2">
												<div class="mb-1 text-[10px] uppercase tracking-wide text-muted-foreground">Agent Details</div>
												<div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
													<div>
														<div class="mb-0.5 text-muted-foreground">Session Count</div>
														<div class="font-medium">{detailCount(subsystem.details, ['session_count', 'sessions', 'active_sessions'])}</div>
													</div>
													<div>
														<div class="mb-0.5 text-muted-foreground">Default Agent</div>
														<div class="font-medium">{formatDetailValue(pickDetail(subsystem.details, ['default_agent', 'agent_default']))}</div>
													</div>
												</div>
												{#if detailList(subsystem.details, ['enabled_agents', 'agents_enabled', 'agents']).length > 0}
													<div class="mt-2">
														<div class="mb-1 text-muted-foreground">Enabled Agents</div>
														<div class="flex flex-wrap gap-1">
															{#each detailList(subsystem.details, ['enabled_agents', 'agents_enabled', 'agents']) as agent}
																<Badge variant="outline" class="text-[10px]">{agent}</Badge>
															{/each}
														</div>
													</div>
												{/if}
											</div>
										{:else if kind === 'comms'}
											<div class="rounded-md border border-border/50 bg-background/60 p-2">
												<div class="mb-1 text-[10px] uppercase tracking-wide text-muted-foreground">Comms Details</div>
												<div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
													<div>
														<div class="mb-0.5 text-muted-foreground">HTTP</div>
														<div class="font-medium">{detailFlag(subsystem.details, ['http_enabled', 'http'])}</div>
													</div>
													<div>
														<div class="mb-0.5 text-muted-foreground">PTY</div>
														<div class="font-medium">{detailFlag(subsystem.details, ['pty_enabled', 'pty'])}</div>
													</div>
													<div>
														<div class="mb-0.5 text-muted-foreground">Telegram</div>
														<div class="font-medium">{detailFlag(subsystem.details, ['telegram_enabled', 'telegram'])}</div>
													</div>
													<div>
														<div class="mb-0.5 text-muted-foreground">Routed</div>
														<div class="font-medium">{detailCount(subsystem.details, ['routed_count', 'routes', 'routed_channels'])}</div>
													</div>
												</div>
												{#if detailList(subsystem.details, ['enabled_channels', 'channels_enabled', 'enabled']).length > 0}
													<div class="mt-2">
														<div class="mb-1 text-muted-foreground">Enabled Channels</div>
														<div class="flex flex-wrap gap-1">
															{#each detailList(subsystem.details, ['enabled_channels', 'channels_enabled', 'enabled']) as channel}
																<Badge variant="outline" class="text-[10px]">{channel}</Badge>
															{/each}
														</div>
													</div>
												{/if}
											</div>
										{:else if kind === 'memory'}
											<div class="rounded-md border border-border/50 bg-background/60 p-2">
												<div class="mb-1 text-[10px] uppercase tracking-wide text-muted-foreground">Memory Details</div>
												<div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
													<div>
														<div class="mb-0.5 text-muted-foreground">Sessions</div>
														<div class="font-medium">{detailCount(subsystem.details, ['session_count', 'sessions'])}</div>
													</div>
													<div>
														<div class="mb-0.5 text-muted-foreground">Index Entries</div>
														<div class="font-medium">{detailCount(subsystem.details, ['index_size', 'index_entries'])}</div>
													</div>
												</div>
											</div>
										{:else if kind === 'llm'}
											<div class="rounded-md border border-border/50 bg-background/60 p-2">
												<div class="mb-1 text-[10px] uppercase tracking-wide text-muted-foreground">LLM Details</div>
												<div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
													<div>
														<div class="mb-0.5 text-muted-foreground">Provider</div>
														<div class="font-medium">{formatDetailValue(pickDetail(subsystem.details, ['provider', 'llm_provider']))}</div>
													</div>
													<div>
														<div class="mb-0.5 text-muted-foreground">Model</div>
														<div class="font-mono text-[10px]">{formatDetailValue(pickDetail(subsystem.details, ['model', 'llm_model']))}</div>
													</div>
												</div>
											</div>
										{/if}
										{#if detailEntries(subsystem.details).length > 0}
											<div class="space-y-1.5">
												<div class="text-[10px] uppercase tracking-wide text-muted-foreground">Details</div>
												<div class="grid grid-cols-1 gap-1.5 sm:grid-cols-2">
													{#each detailEntries(subsystem.details) as [key, value]}
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
