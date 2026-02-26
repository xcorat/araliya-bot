<script lang="ts">
	import { MousePointerClick, Box, ChevronDown, ChevronUp, Database, Loader2 } from '@lucide/svelte';
	import Badge from '$lib/components/ui/badge/badge.svelte';
	import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card';
	import * as Tabs from '$lib/components/ui/tabs';
	import type { HealthResponse, TreeNode, SubsystemStatus, SessionInfo, AgentInfo } from '$lib/types';
	import {
		statusDotClass,
		statusPillClass,
		subsystemKind,
		subsystemCardClass,
		formatUptime,
		formatDetailLabel,
		formatDetailValue,
		detailEntries,
		pickDetail,
		detailList,
		detailFlag,
		detailCount
	} from '$lib/utils/status';
	import { getBaseUrl } from '$lib/state.svelte';
	import * as api from '$lib/api';

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
					(s) =>
						s.id === node.id ||
						(s.name != null && node.name != null && s.name.toLowerCase() === node.name.toLowerCase())
				)
			: undefined
	);

	const kind = $derived(subsystem ? subsystemKind(subsystem.id) : 'default');

	// Find the agents subsystem entry regardless of which node is selected,
	// so we can detect whether the current node is a child agent.
	const agentsSubsystem = $derived<SubsystemStatus | undefined>(
		serviceInfo?.subsystems?.find((s) => subsystemKind(s.id) === 'agents')
	);

	// True when the selected node is a direct child agent (e.g. "docs", "chat"),
	// detected by checking its id against the agents subsystem's own agents list.
	const isAgentChild = $derived(
		!!node &&
			kind !== 'agents' &&
			detailList(agentsSubsystem?.details, ['agents', 'enabled_agents', 'agents_enabled']).includes(
				node.id
			)
	);

	const selectedAgentSubsystem = $derived<SubsystemStatus | undefined>(
		kind === 'agents' ? subsystem : isAgentChild ? agentsSubsystem : undefined
	);

	let detailsExpanded = $state(true);

	// ── Memory tab state ────────────────────────────────────────

	let sessions = $state<SessionInfo[]>([]);
	let agents = $state<AgentInfo[]>([]);

	// Sessions to display in the Memory tab: all for the agents subsystem node,
	// filtered by last_agent for individual agent child nodes.
	const visibleSessions = $derived(
		isAgentChild && node
			? sessions.filter((s) => s.last_agent === node.id)
			: sessions
	);

	const displayedAgentSessions = $derived(isAgentChild ? visibleSessions : sessions);
	let sessionsLoading = $state(false);
	let sessionsError = $state('');
	let agentsLoading = $state(false);
	let agentsError = $state('');
	// Map from session_id -> { loading, error, content, loaded }
	let memoryState = $state<Record<string, { loading: boolean; error: string; content: string; loaded: boolean }>>({});
	let memoryTabActivated = $state(false);
	let sessionsHydrated = $state(false);
	let agentsHydrated = $state(false);

	const selectedAgentMeta = $derived<AgentInfo | undefined>(
		isAgentChild && node ? agents.find((a) => a.agent_id === node.id) : undefined
	);

	const agentMemoryStoreTypes = $derived(
		Array.from(
			new Set(
				[
					...displayedAgentSessions.flatMap((session) =>
					Array.isArray(session.store_types) ? session.store_types : []
					),
					...(selectedAgentMeta?.store_types ?? [])
				]
			)
		).sort()
	);

	const trackedSessionCount = $derived.by(() => {
		if (displayedAgentSessions.length > 0) return displayedAgentSessions.length;
		if (isAgentChild && selectedAgentMeta) return selectedAgentMeta.session_count;
		return displayedAgentSessions.length;
	});

	const agentMemoryLastUpdated = $derived.by(() => {
		let latest: string | null = null;
		let latestMs = -1;
		for (const session of displayedAgentSessions) {
			const value = session.updated_at ?? session.created_at;
			if (!value) continue;
			const ts = Date.parse(value);
			if (Number.isNaN(ts)) continue;
			if (ts > latestMs) {
				latestMs = ts;
				latest = value;
			}
		}
		return latest;
	});

	async function loadSessions() {
		const baseUrl = getBaseUrl();
		if (!baseUrl) return;
		sessionsLoading = true;
		sessionsError = '';
		try {
			const res = await api.listSessions(baseUrl);
			sessions = res.sessions;
		} catch (e) {
			sessionsError = e instanceof Error ? e.message : 'Failed to load sessions';
		} finally {
			sessionsLoading = false;
			sessionsHydrated = true;
		}
	}

	async function loadAgents() {
		const baseUrl = getBaseUrl();
		if (!baseUrl) return;
		agentsLoading = true;
		agentsError = '';
		try {
			const res = await api.listAgents(baseUrl);
			agents = res.agents;
		} catch (e) {
			agentsError = e instanceof Error ? e.message : 'Failed to load agents';
		} finally {
			agentsLoading = false;
			agentsHydrated = true;
		}
	}

	$effect(() => {
		if ((kind === 'agents' || isAgentChild) && !sessionsHydrated && !sessionsLoading) {
			void loadSessions();
		}
		if ((kind === 'agents' || isAgentChild) && !agentsHydrated && !agentsLoading) {
			void loadAgents();
		}
	});

	function onTabChange(value: string) {
		if (value === 'memory' && !memoryTabActivated) {
			memoryTabActivated = true;
			void loadSessions();
		}
	}

	async function toggleMemory(sessionId: string) {
		const current = memoryState[sessionId];
		if (current?.loaded) {
			// Toggle visibility: mark as collapsed by removing the entry
			memoryState = { ...memoryState, [sessionId]: { ...current, loaded: false, content: '' } };
			return;
		}
		if (current?.loading) return;

		memoryState = { ...memoryState, [sessionId]: { loading: true, error: '', content: '', loaded: false } };
		const baseUrl = getBaseUrl();
		try {
			const res = await api.getSessionMemory(baseUrl, sessionId);
			memoryState = { ...memoryState, [sessionId]: { loading: false, error: '', content: res.content, loaded: true } };
		} catch (e) {
			memoryState = {
				...memoryState,
				[sessionId]: {
					loading: false,
					error: e instanceof Error ? e.message : 'Failed to load memory',
					content: '',
					loaded: false
				}
			};
		}
	}

	function shortId(id: string): string {
		return id.length > 12 ? id.slice(0, 8) + '…' : id;
	}

	function formatTime(iso: string | null): string {
		if (!iso) return '—';
		try {
			return new Date(iso).toLocaleString();
		} catch {
			return iso;
		}
	}
</script>

<div class="flex h-full flex-1 flex-col overflow-hidden">
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

		{#if kind === 'agents'}
			<!-- Agents subsystem: tabbed layout -->
			<Tabs.Tabs value="details" onValueChange={onTabChange} class="flex min-h-0 flex-1 flex-col">
				<div class="border-b px-4 pt-2">
					<Tabs.TabsList class="h-8 gap-1 bg-transparent p-0">
						<Tabs.TabsTrigger
							value="details"
							class="h-7 rounded-sm px-3 text-xs data-[state=active]:bg-muted data-[state=active]:shadow-none"
						>
							Details
						</Tabs.TabsTrigger>
						<Tabs.TabsTrigger
							value="memory"
							class="h-7 rounded-sm px-3 text-xs data-[state=active]:bg-muted data-[state=active]:shadow-none"
						>
							Memory
						</Tabs.TabsTrigger>
					</Tabs.TabsList>
				</div>

				<!-- Details tab -->
				<Tabs.TabsContent value="details" class="mt-0 flex-1 overflow-y-auto p-4">
					<div class="space-y-3">
						{@render coreCard()}
						{@render agentsSubsystemCard()}
						{@render allPropertiesCard()}
					</div>
				</Tabs.TabsContent>

				<!-- Memory tab -->
				<Tabs.TabsContent value="memory" class="mt-0 flex-1 overflow-y-auto p-4">
					{#if sessionsLoading}
						<div class="flex items-center gap-2 py-6 text-xs text-muted-foreground">
							<Loader2 class="size-4 animate-spin" />
							<span>Loading sessions…</span>
						</div>
					{:else if sessionsError}
						<p class="py-6 text-xs text-destructive">{sessionsError}</p>
					{:else if sessions.length === 0}
						<div class="flex flex-col items-center gap-2 py-10 text-center">
							<Database class="size-6 text-muted-foreground/40" />
							<p class="text-xs text-muted-foreground">No sessions found.</p>
						</div>
					{:else}
						<div class="space-y-2">
							{#each sessions as session (session.session_id)}
								{@const mem = memoryState[session.session_id]}
								<Card class="overflow-hidden">
									<!-- Session row / toggle -->
									<button
										type="button"
										onclick={() => toggleMemory(session.session_id)}
										class="flex w-full items-center gap-2 px-3 py-2.5 text-left transition-colors hover:bg-muted/30"
									>
										<span class="min-w-0 flex-1 space-y-0.5">
											<span class="flex items-center gap-2">
												<span class="font-mono text-[11px] font-medium text-foreground/90">{shortId(session.session_id)}</span>
												{#if session.last_agent}
													<Badge variant="outline" class="text-[9px] px-1 py-0">{session.last_agent}</Badge>
												{/if}
											</span>
											<span class="block text-[10px] text-muted-foreground">
												{formatTime(session.updated_at ?? session.created_at)}
											</span>
										</span>
										{#if mem?.loading}
											<Loader2 class="size-3.5 shrink-0 animate-spin text-muted-foreground" />
										{:else if mem?.loaded}
											<ChevronUp class="size-3.5 shrink-0 text-muted-foreground" />
										{:else}
											<ChevronDown class="size-3.5 shrink-0 text-muted-foreground" />
										{/if}
									</button>

									<!-- Memory content (expanded) -->
									{#if mem?.loaded}
										<div class="border-t bg-muted/5 px-3 py-2.5">
											{#if mem.content.trim()}
												<pre class="whitespace-pre-wrap break-words font-mono text-[10px] leading-relaxed text-foreground/80">{mem.content}</pre>
											{:else}
												<p class="text-[10px] text-muted-foreground italic">No working memory content.</p>
											{/if}
										</div>
									{:else if mem?.error}
										<div class="border-t bg-destructive/5 px-3 py-2">
											<p class="text-[10px] text-destructive">{mem.error}</p>
										</div>
									{/if}
								</Card>
							{/each}
						</div>
					{/if}
				</Tabs.TabsContent>
			</Tabs.Tabs>
		{:else if isAgentChild}
			<!-- Child agent node: focused detail view with memory summary -->
			<div class="flex-1 space-y-3 overflow-y-auto p-4">
				{@render coreCard()}
				{@render agentsSubsystemCard()}
				{#if selectedAgentSubsystem}
					{@render allPropertiesCard()}
				{/if}
			</div>
		{:else}
			<!-- Non-agents subsystems: single-column layout (no tabs) -->
			<div class="flex-1 space-y-3 overflow-y-auto p-4">
				{@render coreCard()}
				{#if subsystem}
					{#if kind === 'comms'}
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
					{@render allPropertiesCard()}
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
	{/if}
</div>

{#snippet coreCard()}
	<Card class="max-w-lg">
		<CardContent class="grid grid-cols-2 gap-x-4 gap-y-3 p-3 text-xs sm:grid-cols-4">
			<div>
				<div class="mb-0.5 text-muted-foreground">State</div>
				<div class="font-medium capitalize">{node?.state}</div>
			</div>
			<div>
				<div class="mb-0.5 text-muted-foreground">Status</div>
				<div class="font-medium capitalize">{node?.status}</div>
			</div>
			{#if node?.uptime_ms !== undefined}
				<div class="col-span-2">
					<div class="mb-0.5 text-muted-foreground">Uptime</div>
					<div class="font-medium">{formatUptime(node.uptime_ms)}</div>
				</div>
			{/if}
			{#if node && node.children.length > 0}
				<div class="col-span-2">
					<div class="mb-0.5 text-muted-foreground">Children</div>
					<div class="font-medium">{node.children.length}</div>
				</div>
			{/if}
		</CardContent>
	</Card>
{/snippet}

{#snippet agentsSubsystemCard()}
	{#if selectedAgentSubsystem}
		<Card class={subsystemCardClass(selectedAgentSubsystem.id)}>
			<CardHeader class="px-3 pb-1 pt-2.5">
				<CardTitle class="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">
					Agent Details
				</CardTitle>
			</CardHeader>
			<CardContent class="grid grid-cols-2 gap-x-4 gap-y-2 px-3 pb-3 text-xs">
				{#if isAgentChild}
					<div>
						<div class="mb-0.5 text-muted-foreground">Selected Agent</div>
						<div class="font-medium">{node?.id ?? '—'}</div>
					</div>
				{/if}
				<div>
					<div class="mb-0.5 text-muted-foreground">Session Count</div>
					<div class="font-medium">
						{#if isAgentChild && selectedAgentMeta}
							{selectedAgentMeta.session_count}
						{:else}
							{detailCount(selectedAgentSubsystem.details, ['session_count', 'sessions', 'active_sessions'])}
						{/if}
					</div>
				</div>
				{#if !isAgentChild}
					<div>
						<div class="mb-0.5 text-muted-foreground">Default Agent</div>
						<div class="font-medium">{formatDetailValue(pickDetail(selectedAgentSubsystem.details, ['default_agent', 'agent_default']))}</div>
					</div>
				{/if}
				{#if detailList(selectedAgentSubsystem.details, ['enabled_agents', 'agents_enabled', 'agents']).length > 0}
					<div class="col-span-2">
						<div class="mb-1 text-muted-foreground">Enabled Agents</div>
						<div class="flex flex-wrap gap-1">
							{#each detailList(selectedAgentSubsystem.details, ['enabled_agents', 'agents_enabled', 'agents']) as agent}
								<Badge variant="outline" class="text-[10px]">{agent}</Badge>
							{/each}
						</div>
					</div>
				{/if}
				<div class="col-span-2 rounded-md border border-border/50 bg-background/60 p-2">
					<div class="mb-1 text-[10px] uppercase tracking-wide text-muted-foreground">Memory Info</div>
					<div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
						<div>
							<div class="mb-0.5 text-muted-foreground">Tracked Sessions</div>
							{#if sessionsLoading && agentsLoading && displayedAgentSessions.length === 0 && trackedSessionCount === 0}
								<div class="flex items-center gap-1.5 text-muted-foreground">
									<Loader2 class="size-3 animate-spin" />
									<span>Loading…</span>
								</div>
							{:else}
								<div class="font-medium">{trackedSessionCount}</div>
							{/if}
						</div>
						<div>
							<div class="mb-0.5 text-muted-foreground">Last Updated</div>
							<div class="font-medium">{formatTime(agentMemoryLastUpdated)}</div>
						</div>
					</div>
					<div class="mt-2">
						<div class="mb-1 text-muted-foreground">Store Types</div>
						{#if sessionsLoading && agentsLoading && displayedAgentSessions.length === 0 && agentMemoryStoreTypes.length === 0}
							<div class="text-muted-foreground">Loading…</div>
						{:else if agentMemoryStoreTypes.length > 0}
							<div class="flex flex-wrap gap-1">
								{#each agentMemoryStoreTypes as storeType}
									<Badge variant="outline" class="text-[10px]">{storeType}</Badge>
								{/each}
							</div>
						{:else}
							<div class="text-muted-foreground">—</div>
						{/if}
					</div>
					{#if sessionsError}
						<div class="mt-2 text-destructive">{sessionsError}</div>
					{/if}
					{#if agentsError}
						<div class="mt-2 text-destructive">{agentsError}</div>
					{/if}
				</div>
			</CardContent>
		</Card>
	{/if}
{/snippet}

{#snippet allPropertiesCard()}
	{#if subsystem && detailEntries(subsystem.details).length > 0}
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
{/snippet}
