<script lang="ts">
	import { MousePointerClick, Box, ChevronDown, ChevronUp } from '@lucide/svelte';
	import Badge from '$lib/components/ui/badge/badge.svelte';
	import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card';
	import type { HealthResponse, TreeNode, SubsystemStatus } from '$lib/types';
	import {
		statusDotClass,
		statusPillClass,
		subsystemKind,
		subsystemCardClass,
		subsystemHeaderClass,
		formatUptime,
		formatDetailLabel,
		formatDetailValue,
		detailEntries,
		pickDetail,
		detailList,
		detailFlag,
		detailCount
	} from '$lib/utils/status';

	let {
		node,
		serviceInfo
	}: {
		node: TreeNode | null;
		serviceInfo: HealthResponse | null;
	} = $props();

	// Find matching SubsystemStatus by id or name for richer details
	const subsystem = $derived<SubsystemStatus | undefined>(
		node && serviceInfo?.subsystems
			? serviceInfo.subsystems.find(
					(s) => s.id === node.id || s.name.toLowerCase() === node.name.toLowerCase()
				)
			: undefined
	);

	const kind = $derived(subsystem ? subsystemKind(subsystem.id) : 'default');

	let detailsExpanded = $state(true);
</script>

<div class="flex h-full w-80 shrink-0 flex-col border-l bg-background">
	{#if !node}
		<!-- Placeholder -->
		<div class="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center">
			<div class="rounded-full border border-border/50 bg-muted/30 p-4">
				<MousePointerClick class="size-6 text-muted-foreground/50" />
			</div>
			<div>
				<p class="text-sm font-medium text-foreground/70">No component selected</p>
				<p class="mt-1 text-xs text-muted-foreground">
					Click any node in the component tree to see its details here.
				</p>
			</div>
		</div>
	{:else}
		<!-- Header -->
		<div class="flex items-start gap-3 border-b p-4">
			<div class="mt-0.5 rounded-md border border-border/50 bg-muted/20 p-1.5">
				<Box class="size-4 text-muted-foreground" />
			</div>
			<div class="min-w-0 flex-1">
				<div class="flex items-center gap-2">
					<span class={`size-2 shrink-0 rounded-full ${statusDotClass(node.status)}`}></span>
					<h3 class="truncate text-sm font-semibold">{node.name}</h3>
				</div>
				<p class="mt-0.5 truncate font-mono text-[10px] text-muted-foreground">{node.id}</p>
			</div>
			<Badge
				variant="outline"
				class={`shrink-0 text-[10px] capitalize ${statusPillClass(node.status)}`}
			>
				{node.status}
			</Badge>
		</div>

		<!-- Scrollable content -->
		<div class="flex-1 space-y-3 overflow-y-auto p-4">
			<!-- Core fields -->
			<Card>
				<CardContent class="grid grid-cols-2 gap-x-4 gap-y-3 p-3 text-xs">
					<div>
						<div class="mb-0.5 text-muted-foreground">State</div>
						<div class="font-medium capitalize">{node.state}</div>
					</div>
					<div>
						<div class="mb-0.5 text-muted-foreground">Status</div>
						<div class="font-medium capitalize">{node.status}</div>
					</div>
					{#if node.uptime_ms !== undefined}
						<div class="col-span-2">
							<div class="mb-0.5 text-muted-foreground">Uptime</div>
							<div class="font-medium">{formatUptime(node.uptime_ms)}</div>
						</div>
					{/if}
					{#if node.children.length > 0}
						<div class="col-span-2">
							<div class="mb-0.5 text-muted-foreground">Children</div>
							<div class="font-medium">{node.children.length}</div>
						</div>
					{/if}
				</CardContent>
			</Card>

			{#if subsystem}
				<!-- Rich subsystem details -->
				{#if kind === 'agents'}
					<Card class={subsystemCardClass(subsystem.id)}>
						<CardHeader class="px-3 pb-1 pt-2.5">
							<CardTitle class="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">
								Agent Details
							</CardTitle>
						</CardHeader>
						<CardContent class="grid grid-cols-2 gap-x-4 gap-y-2 px-3 pb-3 text-xs">
							<div>
								<div class="mb-0.5 text-muted-foreground">Session Count</div>
								<div class="font-medium">{detailCount(subsystem.details, ['session_count', 'sessions', 'active_sessions'])}</div>
							</div>
							<div>
								<div class="mb-0.5 text-muted-foreground">Default Agent</div>
								<div class="font-medium">{formatDetailValue(pickDetail(subsystem.details, ['default_agent', 'agent_default']))}</div>
							</div>
							{#if detailList(subsystem.details, ['enabled_agents', 'agents_enabled', 'agents']).length > 0}
								<div class="col-span-2">
									<div class="mb-1 text-muted-foreground">Enabled Agents</div>
									<div class="flex flex-wrap gap-1">
										{#each detailList(subsystem.details, ['enabled_agents', 'agents_enabled', 'agents']) as agent}
											<Badge variant="outline" class="text-[10px]">{agent}</Badge>
										{/each}
									</div>
								</div>
							{/if}
						</CardContent>
					</Card>
				{:else if kind === 'comms'}
					<Card class={subsystemCardClass(subsystem.id)}>
						<CardHeader class="px-3 pb-1 pt-2.5">
							<CardTitle class="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">
								Comms Details
							</CardTitle>
						</CardHeader>
						<CardContent class="grid grid-cols-2 gap-x-4 gap-y-2 px-3 pb-3 text-xs">
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
							{#if detailList(subsystem.details, ['enabled_channels', 'channels_enabled', 'enabled']).length > 0}
								<div class="col-span-2">
									<div class="mb-1 text-muted-foreground">Enabled Channels</div>
									<div class="flex flex-wrap gap-1">
										{#each detailList(subsystem.details, ['enabled_channels', 'channels_enabled', 'enabled']) as channel}
											<Badge variant="outline" class="text-[10px]">{channel}</Badge>
										{/each}
									</div>
								</div>
							{/if}
						</CardContent>
					</Card>
				{:else if kind === 'memory'}
					<Card class={subsystemCardClass(subsystem.id)}>
						<CardHeader class="px-3 pb-1 pt-2.5">
							<CardTitle class="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">
								Memory Details
							</CardTitle>
						</CardHeader>
						<CardContent class="grid grid-cols-2 gap-x-4 gap-y-2 px-3 pb-3 text-xs">
							<div>
								<div class="mb-0.5 text-muted-foreground">Sessions</div>
								<div class="font-medium">{detailCount(subsystem.details, ['session_count', 'sessions'])}</div>
							</div>
							<div>
								<div class="mb-0.5 text-muted-foreground">Index Entries</div>
								<div class="font-medium">{detailCount(subsystem.details, ['index_size', 'index_entries'])}</div>
							</div>
						</CardContent>
					</Card>
				{:else if kind === 'llm'}
					<Card class={subsystemCardClass(subsystem.id)}>
						<CardHeader class="px-3 pb-1 pt-2.5">
							<CardTitle class="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">
								LLM Details
							</CardTitle>
						</CardHeader>
						<CardContent class="grid grid-cols-2 gap-x-4 gap-y-2 px-3 pb-3 text-xs">
							<div>
								<div class="mb-0.5 text-muted-foreground">Provider</div>
								<div class="font-medium">{formatDetailValue(pickDetail(subsystem.details, ['provider', 'llm_provider']))}</div>
							</div>
							<div>
								<div class="mb-0.5 text-muted-foreground">Model</div>
								<div class="font-mono text-[10px]">{formatDetailValue(pickDetail(subsystem.details, ['model', 'llm_model']))}</div>
							</div>
						</CardContent>
					</Card>
				{/if}

				<!-- All raw details, collapsible -->
				{#if detailEntries(subsystem.details).length > 0}
					<Card>
						<button
							type="button"
							onclick={() => (detailsExpanded = !detailsExpanded)}
							class="flex w-full items-center justify-between px-3 py-2 text-left transition-colors hover:bg-muted/20"
						>
							<span class="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">
								All Properties
							</span>
							{#if detailsExpanded}
								<ChevronUp class="size-3.5 text-muted-foreground" />
							{:else}
								<ChevronDown class="size-3.5 text-muted-foreground" />
							{/if}
						</button>
						{#if detailsExpanded}
							<CardContent class="space-y-1.5 px-3 pb-3 pt-0">
								{#each detailEntries(subsystem.details) as [key, value]}
									<div class="rounded-md border border-border/50 bg-muted/5 px-2 py-1.5">
										<div class="mb-0.5 text-[10px] text-muted-foreground">{formatDetailLabel(key)}</div>
										<div class="break-all font-mono text-[10px] text-foreground/90">
											{formatDetailValue(value)}
										</div>
									</div>
								{/each}
							</CardContent>
						{/if}
					</Card>
				{/if}
			{:else}
				<!-- No matching subsystem — placeholder detail card -->
				<Card class="border-dashed">
					<CardContent class="p-4 text-center">
						<p class="text-xs text-muted-foreground">
							No additional detail data available for this component.
						</p>
					</CardContent>
				</Card>
			{/if}
		</div>
	{/if}
</div>
