<script lang="ts">
	import { Collapsible } from 'bits-ui';
	import { ChevronRight } from '@lucide/svelte';
	import type { TreeNode } from '$lib/types';
	import Self from './ComponentTreeNode.svelte';

	let {
		node,
		depth = 0,
		selectedId = '',
		onSelectNode
	}: {
		node: TreeNode;
		depth?: number;
		selectedId?: string;
		onSelectNode?: (node: TreeNode) => void;
	} = $props();

	// Nodes at depth 0 and 1 start expanded; deeper levels start collapsed.
	// Capture depth via a closure to satisfy Svelte's reactive-access linting.
	let open = $state((() => depth < 2)());

	const hasChildren = $derived(node.children && node.children.length > 0);
	const isRoot = $derived(depth === 0);
	const isSelected = $derived(selectedId === node.id);

	function stateDotClass(state: string, status: string): string {
		const s = state.toLowerCase();
		const st = status.toLowerCase();
		if (s === 'on' && (st === 'running' || st === 'ok')) return 'bg-emerald-500';
		if (s === 'off') return 'border border-muted-foreground/40 bg-transparent';
		if (s === 'err' || st === 'error' || st === 'failed') return 'bg-destructive';
		if (st === 'degraded' || st === 'warning') return 'bg-yellow-500';
		return 'bg-emerald-500';
	}

	function statePillClass(state: string): string {
		const s = state.toLowerCase();
		if (s === 'on') return 'border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300';
		if (s === 'off') return 'border-muted-foreground/25 bg-muted/40 text-muted-foreground';
		if (s === 'err') return 'border-destructive/30 bg-destructive/10 text-destructive';
		return 'border-muted-foreground/25 bg-muted/40 text-muted-foreground';
	}

	function formatUptime(ms: number | undefined): string {
		if (!ms || ms < 0) return '';
		const totalSeconds = Math.floor(ms / 1000);
		const hours = Math.floor(totalSeconds / 3600);
		const minutes = Math.floor((totalSeconds % 3600) / 60);
		const seconds = totalSeconds % 60;
		if (hours > 0) return `${hours}h ${minutes}m`;
		if (minutes > 0) return `${minutes}m ${seconds}s`;
		return `${seconds}s`;
	}

	const uptime = $derived(formatUptime(node.uptime_ms));
	const indent = $derived(depth * 16);
</script>

<div style="padding-left: {indent}px;">
	{#if hasChildren}
		<Collapsible.Root bind:open>
			<Collapsible.Trigger
				onclick={() => onSelectNode?.(node)}
				class="flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left transition-colors hover:bg-muted/40
					{isRoot ? 'font-medium' : ''}
					{isSelected ? 'bg-primary/10 ring-1 ring-inset ring-primary/30' : isRoot ? 'bg-muted/20' : ''}"
			>
				<!-- expand/collapse chevron -->
				<ChevronRight
					class="size-3.5 shrink-0 text-muted-foreground/70 transition-transform duration-200
						{open ? 'rotate-90' : ''}"
				/>

				<!-- status dot -->
				<span class={`size-2 shrink-0 rounded-full ${stateDotClass(node.state, node.status)}`}></span>

				<!-- name + id -->
				<span class="min-w-0 flex-1 truncate {isRoot ? 'text-sm' : 'text-xs'}">
					{node.name}
					{#if !isRoot}
						<span class="ml-1 text-muted-foreground/70">{node.id}</span>
					{/if}
				</span>

				<!-- child count badge -->
				<span class="shrink-0 rounded bg-muted px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground">
					{node.children.length}
				</span>

				<!-- uptime (root only) -->
				{#if uptime}
					<span class="hidden shrink-0 font-mono text-[10px] text-muted-foreground sm:inline">
						{uptime}
					</span>
				{/if}

				<!-- state pill -->
				<span
					class="shrink-0 rounded border px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide {statePillClass(node.state)}"
				>
					{node.state}
				</span>
			</Collapsible.Trigger>

			<Collapsible.Content
				class="overflow-hidden transition-all data-[state=open]:animate-none data-[state=closed]:animate-none"
			>
				<div class="mt-0.5 border-l border-border/40 ml-3.5">
					{#each node.children as child (child.id)}
						<Self node={child} depth={depth + 1} {selectedId} {onSelectNode} />
					{/each}
				</div>
			</Collapsible.Content>
		</Collapsible.Root>
	{:else}
		<!-- Leaf node -->
		<button
			type="button"
			onclick={() => onSelectNode?.(node)}
			class="flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left transition-colors hover:bg-muted/30
				{isRoot ? 'font-medium' : ''}
				{isSelected ? 'bg-primary/10 ring-1 ring-inset ring-primary/30' : isRoot ? 'bg-muted/20' : ''}"
		>
			<!-- placeholder to align with chevron width -->
			<span class="size-3.5 shrink-0"></span>

			<!-- status dot -->
			<span class={`size-2 shrink-0 rounded-full ${stateDotClass(node.state, node.status)}`}></span>

			<!-- name + id -->
			<span class="min-w-0 flex-1 truncate {isRoot ? 'text-sm' : 'text-xs'}">
				{node.name}
				{#if !isRoot}
					<span class="ml-1 text-muted-foreground/70">{node.id}</span>
				{/if}
			</span>

			<!-- uptime (root only) -->
			{#if uptime}
				<span class="hidden shrink-0 font-mono text-[10px] text-muted-foreground sm:inline">
					{uptime}
				</span>
			{/if}

			<!-- state pill -->
			<span
				class="shrink-0 rounded border px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide {statePillClass(node.state)}"
			>
				{node.state}
			</span>
		</button>
	{/if}
</div>
