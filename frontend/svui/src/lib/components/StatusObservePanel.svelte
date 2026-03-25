<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { base } from '$app/paths';
	import type { ObsEvent, ObsLevel } from '$lib/types';
	import * as api from '$lib/api';
	import { getBaseUrl } from '$lib/state.svelte';
	import { Badge, type BadgeVariant } from '$lib/components/ui/badge';
	import { Input } from '$lib/components/ui/input';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu';
	import * as Tabs from '$lib/components/ui/tabs';

	const MAX_EVENTS = 500;
	const ALL_LEVELS: ObsLevel[] = ['TRACE', 'DEBUG', 'INFO', 'WARN', 'ERROR'];

	let { filter = '' }: { filter?: string } = $props();

	let events = $state<ObsEvent[]>([]);
	let autoScroll = $state(true);
	let connected = $state(false);
	let error = $state('');
	let es: EventSource | null = null;
	let scrollContainer: HTMLDivElement | undefined = $state();
	let activeTab = $state<'all' | 'logs' | 'events'>('all');
	let searchQuery = $state('');
	let activeLevels = $state<ObsLevel[]>([...ALL_LEVELS]);

	const uiBase = $derived(base || '');

	// Split events by source type
	const stdoutEvents = $derived(events.filter((ev) => ev.span_id !== undefined));
	const domainEvents = $derived(events.filter((ev) => ev.session_id !== undefined));

	// Filter by active tab
	const tabFilteredEvents = $derived(
		activeTab === 'logs' ? stdoutEvents : activeTab === 'events' ? domainEvents : events
	);

	// Apply agent/session filter (URL-based)
	const agentFilteredEvents = $derived(
		filter
			? tabFilteredEvents.filter(
					(ev) =>
						(ev.fields as Record<string, unknown> | null)?.agent_id === filter ||
						ev.session_id?.startsWith(filter)
				)
			: tabFilteredEvents
	);

	// Apply level filter
	const levelFilteredEvents = $derived(
		activeLevels.length === ALL_LEVELS.length
			? agentFilteredEvents
			: agentFilteredEvents.filter((ev) => activeLevels.includes(ev.level))
	);

	// Apply text search
	const visibleEvents = $derived(
		searchQuery.trim()
			? levelFilteredEvents.filter((ev) => {
					const q = searchQuery.toLowerCase();
					return ev.message.toLowerCase().includes(q) || ev.target.toLowerCase().includes(q);
				})
			: levelFilteredEvents
	);

	const LEVEL_BADGE: Record<ObsLevel, { variant: BadgeVariant; cls: string }> = {
		TRACE: { variant: 'outline', cls: 'text-muted-foreground/50 border-muted-foreground/20' },
		DEBUG: { variant: 'outline', cls: 'text-blue-400 border-blue-400/30' },
		INFO: { variant: 'outline', cls: 'text-green-400 border-green-400/30' },
		WARN: { variant: 'outline', cls: 'text-yellow-400 border-yellow-400/30' },
		ERROR: { variant: 'destructive', cls: '' }
	};

	$effect(() => {
		if (autoScroll && scrollContainer && visibleEvents.length > 0) {
			scrollContainer.scrollTop = scrollContainer.scrollHeight;
		}
	});

	onMount(async () => {
		const baseUrl = getBaseUrl();
		if (!baseUrl) {
			error = 'No base URL configured';
			return;
		}

		// 1. Load snapshot (recent history)
		try {
			const snap = await api.fetchObserveSnapshot(baseUrl);
			events = snap.slice(-MAX_EVENTS);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load snapshot';
		}

		// 2. Subscribe to live SSE stream
		try {
			es = new EventSource(`${baseUrl}/api/observe/events`);
			es.onopen = () => {
				connected = true;
				error = '';
			};
			es.onmessage = (e) => {
				try {
					const evt: ObsEvent = JSON.parse(e.data);
					events = [...events.slice(-(MAX_EVENTS - 1)), evt];
				} catch {
					// Ignore malformed events
				}
			};
			es.addEventListener('lagged', async () => {
				// Re-sync from snapshot when the server signals dropped events
				try {
					const snap = await api.fetchObserveSnapshot(baseUrl);
					events = snap.slice(-MAX_EVENTS);
				} catch {
					// Silently ignore re-sync failures
				}
			});
			es.onerror = () => {
				connected = false;
			};
		} catch {
			error = 'Failed to connect to event stream';
		}
	});

	onDestroy(() => {
		es?.close();
		es = null;
	});

	async function handleClear() {
		const baseUrl = getBaseUrl();
		if (!baseUrl) return;
		try {
			await api.clearObserveEvents(baseUrl);
			events = [];
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to clear events';
		}
	}

	function formatTime(ts: number): string {
		return new Date(ts).toLocaleTimeString('en-US', { hour12: false, fractionalSecondDigits: 3 });
	}

	function handleScroll() {
		if (!scrollContainer) return;
		autoScroll = scrollContainer.scrollHeight - scrollContainer.scrollTop - scrollContainer.clientHeight < 40;
	}
</script>

<div class="flex h-full flex-col gap-0 p-4">
	<!-- Row 1: Header with tabs -->
	<div class="flex items-center justify-between gap-2 pb-2">
		<div class="flex items-center gap-2 min-w-0">
			<h2 class="text-sm font-semibold shrink-0">Event Log</h2>
			<span
				class="size-2 rounded-full shrink-0 {connected ? 'bg-green-500' : 'bg-red-500/70'}"
				title={connected ? 'Connected' : 'Disconnected'}
			></span>
			{#if filter}
				<span
					class="flex items-center gap-1 rounded-full border border-border/50 bg-muted/40 px-2 py-0.5 text-[10px] text-muted-foreground shrink-0"
				>
					agent: <span class="font-mono font-medium text-foreground/80">{filter}</span>
					<a
						href="{uiBase}/status/observe"
						class="ml-0.5 text-muted-foreground/50 hover:text-foreground"
						title="Clear filter"
					>×</a>
				</span>
			{/if}
			<span class="text-[10px] text-muted-foreground shrink-0">{visibleEvents.length} / {events.length}</span>
		</div>
		<Tabs.Tabs bind:value={activeTab} class="flex items-center gap-2">
			<Tabs.TabsList class="h-7 gap-0 bg-transparent p-0">
				<Tabs.TabsTrigger value="all" class="h-6 rounded-sm px-2.5 text-[11px] data-[state=active]:bg-muted data-[state=active]:shadow-none">
					All ({events.length})
				</Tabs.TabsTrigger>
				<Tabs.TabsTrigger value="logs" class="h-6 rounded-sm px-2.5 text-[11px] data-[state=active]:bg-muted data-[state=active]:shadow-none">
					Logs ({stdoutEvents.length})
				</Tabs.TabsTrigger>
				<Tabs.TabsTrigger value="events" class="h-6 rounded-sm px-2.5 text-[11px] data-[state=active]:bg-muted data-[state=active]:shadow-none">
					Events ({domainEvents.length})
				</Tabs.TabsTrigger>
			</Tabs.TabsList>
		</Tabs.Tabs>
	</div>

	<!-- Row 2: Filter toolbar -->
	<div class="flex items-center gap-2 pb-2">
		<Input
			type="search"
			bind:value={searchQuery}
			placeholder="Filter message or target…"
			class="h-7 text-xs flex-1 min-w-0"
		/>
		<DropdownMenu.DropdownMenu>
			<DropdownMenu.DropdownMenuTrigger>
				{#snippet child({ props })}
					<button
						{...props}
						class="h-7 rounded border border-input bg-background px-2 py-1 text-xs text-foreground transition-colors hover:bg-muted/50 shrink-0"
					>
						Level
						{#if activeLevels.length < 5}
							<Badge variant="secondary" class="ml-1 text-[9px] px-1 py-0.5">
								{activeLevels.length}
							</Badge>
						{/if}
					</button>
				{/snippet}
			</DropdownMenu.DropdownMenuTrigger>
			<DropdownMenu.DropdownMenuContent class="w-36">
				<DropdownMenu.DropdownMenuLabel class="text-xs">Levels</DropdownMenu.DropdownMenuLabel>
				<DropdownMenu.DropdownMenuSeparator />
				<DropdownMenu.DropdownMenuCheckboxGroup bind:value={activeLevels}>
					{#each ALL_LEVELS as level}
						<DropdownMenu.DropdownMenuCheckboxItem value={level} class="text-xs">
							<span class={LEVEL_BADGE[level].cls}>{level}</span>
						</DropdownMenu.DropdownMenuCheckboxItem>
					{/each}
				</DropdownMenu.DropdownMenuCheckboxGroup>
			</DropdownMenu.DropdownMenuContent>
		</DropdownMenu.DropdownMenu>
		<label class="flex items-center gap-1 text-xs text-muted-foreground shrink-0">
			<input type="checkbox" bind:checked={autoScroll} class="size-3" />
			Auto-scroll
		</label>
		<button
			onclick={handleClear}
			class="h-7 rounded bg-muted/40 px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-muted/70 shrink-0"
		>
			Clear
		</button>
	</div>

	{#if error}
		<div
			class="rounded border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive"
		>
			{error}
		</div>
	{/if}

	<!-- Row 3: Table with sticky headers -->
	<div
		class="flex-1 overflow-hidden rounded border border-border/40 bg-background/50 flex flex-col"
		style="--obs-cols: 7rem 4rem minmax(6rem,10rem) 1fr 5rem 5rem minmax(0,10rem)"
	>
		<!-- Sticky column headers -->
		<div
			class="sticky top-0 z-10 grid gap-x-2 px-2 py-1 shrink-0 bg-background/95 backdrop-blur-sm border-b border-border/40 font-sans text-[10px] font-semibold uppercase tracking-wide text-muted-foreground"
			style="grid-template-columns: var(--obs-cols)"
		>
			<span>Time</span>
			<span>Level</span>
			<span>Source</span>
			<span>Message</span>
			<span>Session</span>
			<span>Span</span>
			<span>Fields</span>
		</div>

		<!-- Scrollable event rows -->
		<div
			bind:this={scrollContainer}
			class="flex-1 overflow-y-auto font-mono text-[11px]"
			onscroll={handleScroll}
		>
			{#if visibleEvents.length === 0}
				<div class="flex h-full items-center justify-center py-12 text-xs text-muted-foreground">
					{#if searchQuery || activeLevels.length < 5}
						No events match current filters…
					{:else if filter}
						No events for agent "{filter}" yet…
					{:else}
						No events yet — waiting for activity…
					{/if}
				</div>
			{:else}
				<div class="space-y-px p-1">
					{#each visibleEvents as ev, i (i.toString() + ev.ts_unix_ms + ev.target)}
						{@const lb = LEVEL_BADGE[ev.level]}
						<div
							class="grid gap-x-2 rounded px-1.5 py-0.5 transition-colors hover:bg-muted/20"
							style="grid-template-columns: var(--obs-cols)"
						>
							<span class="tabular-nums text-muted-foreground/50">{formatTime(ev.ts_unix_ms)}</span>
							<Badge
								variant={lb.variant}
								class="font-mono text-[9px] px-1 py-0 h-fit {lb.cls}"
							>
								{ev.level}
							</Badge>
							<span class="truncate text-muted-foreground" title={ev.target}>{ev.target}</span>
							<span class="min-w-0 break-all text-foreground/90">{ev.message}</span>
							<span
								class="truncate text-muted-foreground/40 font-mono"
								title={ev.session_id ?? ''}
							>
								{ev.session_id?.slice(0, 8) ?? ''}
							</span>
							<span class="truncate text-muted-foreground/40 font-mono" title={ev.span_id ?? ''}>
								{ev.span_id?.slice(0, 8) ?? ''}
							</span>
							<span
								class="truncate text-muted-foreground/40"
								title={ev.fields ? JSON.stringify(ev.fields) : ''}
							>
								{ev.fields ? JSON.stringify(ev.fields) : ''}
							</span>
						</div>
					{/each}
				</div>
			{/if}
		</div>
	</div>
</div>
