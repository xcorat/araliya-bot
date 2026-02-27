<script lang="ts">
	import { ChartNetwork, Loader2, ChevronDown, ChevronUp } from '@lucide/svelte';
	import Badge from '$lib/components/ui/badge/badge.svelte';
	import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card';
	import type { TreeNode, KgEntity, KgRelation, KgEntityKind } from '$lib/types';
	import { getBaseUrl } from '$lib/state.svelte';
	import * as api from '$lib/api';

	let { node }: { node: TreeNode } = $props();

	let loading = $state(false);
	let error = $state('');
	let entities = $state<KgEntity[]>([]);
	let relations = $state<KgRelation[]>([]);
	let entityMap = $state<Record<string, KgEntity>>({});
	let hydrated = $state(false);

	let entitiesExpanded = $state(true);
	let relationsExpanded = $state(true);

	async function loadKG() {
		const baseUrl = getBaseUrl();
		if (!baseUrl) return;
		loading = true;
		error = '';
		try {
			const res = await api.getAgentKG(baseUrl, node.id);
			entityMap = res.graph.entities ?? {};
			entities = Object.values(entityMap).sort((a, b) => b.mention_count - a.mention_count);
			relations = (res.graph.relations ?? []).sort((a, b) => b.weight - a.weight);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load knowledge graph';
		} finally {
			loading = false;
			hydrated = true;
		}
	}

	$effect(() => {
		if (!hydrated && !loading) void loadKG();
	});

	function kindClass(kind: KgEntityKind): string {
		switch (kind) {
			case 'concept': return 'bg-blue-500/10 text-blue-600 border-blue-500/30';
			case 'system':  return 'bg-purple-500/10 text-purple-600 border-purple-500/30';
			case 'person':  return 'bg-green-500/10 text-green-600 border-green-500/30';
			case 'term':    return 'bg-amber-500/10 text-amber-600 border-amber-500/30';
			case 'acronym': return 'bg-rose-500/10 text-rose-600 border-rose-500/30';
			default:        return 'bg-muted text-muted-foreground';
		}
	}

	function entityName(id: string): string {
		return entityMap[id]?.name ?? id.slice(0, 8);
	}

	function formatWeight(w: number): string {
		return (w * 100).toFixed(0) + '%';
	}
</script>

<div class="flex h-full flex-1 flex-col overflow-hidden">
	<!-- Header -->
	<div class="flex items-center gap-3 border-b p-4">
		<div class="rounded-md border border-border/50 bg-muted/20 p-1.5">
			<ChartNetwork class="size-4 text-muted-foreground" />
		</div>
		<div class="min-w-0 flex-1">
			<h3 class="text-sm font-semibold">Knowledge Graph</h3>
			<p class="mt-0.5 font-mono text-[10px] text-muted-foreground">{node.id}</p>
		</div>
		{#if hydrated && !loading}
			<div class="flex gap-2 text-[10px] text-muted-foreground">
				<span>{entities.length} entities</span>
				<span>·</span>
				<span>{relations.length} relations</span>
			</div>
		{/if}
	</div>

	{#if loading}
		<div class="flex flex-1 items-center justify-center gap-2 text-xs text-muted-foreground">
			<Loader2 class="size-4 animate-spin" />
			<span>Loading knowledge graph…</span>
		</div>
	{:else if error}
		<div class="p-4">
			<p class="text-xs text-destructive">{error}</p>
		</div>
	{:else if entities.length === 0 && relations.length === 0}
		<div class="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center">
			<div class="rounded-full border border-border/50 bg-muted/30 p-4">
				<ChartNetwork class="size-6 text-muted-foreground/40" />
			</div>
			<div>
				<p class="text-sm font-medium text-foreground/70">No knowledge graph yet</p>
				<p class="mt-1 text-xs text-muted-foreground">
					The graph is built when the agent indexes documents.<br />
					Trigger a rebuild via the docs agent to populate it.
				</p>
			</div>
		</div>
	{:else}
		<div class="flex-1 space-y-3 overflow-y-auto p-4">

			<!-- Entities -->
			<Card>
				<button
					type="button"
					onclick={() => (entitiesExpanded = !entitiesExpanded)}
					class="flex w-full items-center justify-between px-3 py-2 text-left transition-colors hover:bg-muted/20"
				>
					<span class="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">
						Entities <span class="ml-1 font-mono normal-case text-foreground/60">{entities.length}</span>
					</span>
					{#if entitiesExpanded}
						<ChevronUp class="size-3.5 text-muted-foreground" />
					{:else}
						<ChevronDown class="size-3.5 text-muted-foreground" />
					{/if}
				</button>
				{#if entitiesExpanded}
					<CardContent class="space-y-1 px-3 pb-3 pt-0">
						{#each entities as entity (entity.id)}
							<div class="flex items-center gap-2 rounded-md border border-border/40 bg-muted/5 px-2 py-1.5">
								<span class={`shrink-0 rounded border px-1.5 py-0.5 font-mono text-[9px] uppercase tracking-wide ${kindClass(entity.kind)}`}>
									{entity.kind}
								</span>
								<span class="min-w-0 flex-1 truncate text-[11px] font-medium">{entity.name}</span>
								<span class="shrink-0 font-mono text-[10px] text-muted-foreground" title="mention count">
									×{entity.mention_count}
								</span>
							</div>
						{/each}
					</CardContent>
				{/if}
			</Card>

			<!-- Relations -->
			{#if relations.length > 0}
				<Card>
					<button
						type="button"
						onclick={() => (relationsExpanded = !relationsExpanded)}
						class="flex w-full items-center justify-between px-3 py-2 text-left transition-colors hover:bg-muted/20"
					>
						<span class="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">
							Relations <span class="ml-1 font-mono normal-case text-foreground/60">{relations.length}</span>
						</span>
						{#if relationsExpanded}
							<ChevronUp class="size-3.5 text-muted-foreground" />
						{:else}
							<ChevronDown class="size-3.5 text-muted-foreground" />
						{/if}
					</button>
					{#if relationsExpanded}
						<CardContent class="space-y-1 px-3 pb-3 pt-0">
							{#each relations as rel, i (i)}
								<div class="flex items-center gap-1.5 rounded-md border border-border/40 bg-muted/5 px-2 py-1.5 text-[10px]">
									<span class="min-w-0 truncate font-medium text-foreground/80">{entityName(rel.from)}</span>
									<Badge variant="outline" class="shrink-0 px-1.5 py-0 text-[9px]">{rel.label}</Badge>
									<span class="min-w-0 truncate font-medium text-foreground/80">{entityName(rel.to)}</span>
									<span class="ml-auto shrink-0 font-mono text-muted-foreground">{formatWeight(rel.weight)}</span>
								</div>
							{/each}
						</CardContent>
					{/if}
				</Card>
			{/if}

		</div>
	{/if}
</div>
