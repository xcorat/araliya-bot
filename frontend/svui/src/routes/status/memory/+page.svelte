<script lang="ts">
	import { onMount } from 'svelte';
	import { base } from '$app/paths';
	import { Loader2, Database, Network } from '@lucide/svelte';
	import Badge from '$lib/components/ui/badge/badge.svelte';
	import { Card, CardContent } from '$lib/components/ui/card';
	import type { AgentInfo, SessionInfo } from '$lib/types';
	import { getBaseUrl } from '$lib/state.svelte';
	import * as api from '$lib/api';

	const uiBase = $derived(base || '');

	let agents = $state<AgentInfo[]>([]);
	let sessions = $state<SessionInfo[]>([]);
	let loading = $state(false);
	let error = $state('');

	// Per-agent session count from the sessions list (more accurate than AgentInfo.session_count)
	const sessionsByAgent = $derived(() => {
		const m: Record<string, number> = {};
		for (const s of sessions) {
			if (s.last_agent) m[s.last_agent] = (m[s.last_agent] ?? 0) + 1;
		}
		return m;
	});

	onMount(async () => {
		const baseUrl = getBaseUrl();
		if (!baseUrl) { error = 'No base URL configured'; return; }
		loading = true;
		try {
			const [agentsRes, sessionsRes] = await Promise.allSettled([
				api.listAgents(baseUrl),
				api.listSessions(baseUrl)
			]);
			if (agentsRes.status === 'fulfilled') agents = agentsRes.value.agents;
			if (sessionsRes.status === 'fulfilled') sessions = sessionsRes.value.sessions;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load';
		} finally {
			loading = false;
		}
	});
</script>

<svelte:head>
	<title>Araliya · Status · Memory</title>
</svelte:head>

<div class="flex h-full flex-col gap-4 p-6">
	<div>
		<h2 class="text-base font-semibold">Memory Inspector</h2>
		<p class="mt-0.5 text-xs text-muted-foreground">Browse session memory and knowledge graphs by agent.</p>
	</div>

	{#if error}
		<div class="rounded border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">
			{error}
		</div>
	{/if}

	{#if loading}
		<div class="flex items-center gap-2 text-xs text-muted-foreground">
			<Loader2 class="size-4 animate-spin" />
			Loading agents…
		</div>
	{:else if agents.length === 0}
		<div class="flex flex-col items-center gap-2 py-12 text-center">
			<Database class="size-8 text-muted-foreground/30" />
			<p class="text-xs text-muted-foreground">No agents found.</p>
		</div>
	{:else}
		<div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
			{#each agents as agent (agent.agent_id)}
				{@const hasKG = agent.store_types?.includes('kgdocstore') ?? false}
				{@const sessionCount = sessionsByAgent()[agent.agent_id] ?? agent.session_count}
				<Card class="transition-colors hover:bg-muted/10">
					<CardContent class="p-4">
						<div class="mb-3 flex items-start justify-between gap-2">
							<div class="min-w-0">
								<p class="truncate text-sm font-medium">{agent.name}</p>
								<p class="mt-0.5 truncate font-mono text-[10px] text-muted-foreground">{agent.agent_id}</p>
							</div>
							<Badge variant="outline" class="shrink-0 text-[10px]">
								{sessionCount} session{sessionCount === 1 ? '' : 's'}
							</Badge>
						</div>

						{#if agent.store_types && agent.store_types.length > 0}
							<div class="mb-3 flex flex-wrap gap-1">
								{#each agent.store_types as st}
									<Badge variant="secondary" class="text-[9px] px-1.5 py-0">{st}</Badge>
								{/each}
							</div>
						{/if}

						<div class="flex flex-wrap gap-x-3 gap-y-1.5">
							<a
								href="{uiBase}/status/{encodeURIComponent(agent.agent_id)}/memory"
								class="flex items-center gap-1 text-[11px] text-primary hover:underline"
							>
								<Database class="size-3" />
								Memory
							</a>
							{#if hasKG}
								<a
									href="{uiBase}/status/{encodeURIComponent(agent.agent_id)}/kg"
									class="flex items-center gap-1 text-[11px] text-primary hover:underline"
								>
									<Network class="size-3" />
									KG
								</a>
							{/if}
						</div>
					</CardContent>
				</Card>
			{/each}
		</div>
	{/if}
</div>
