<script lang="ts">
	import { Collapsible } from 'bits-ui';
	import {
		ChevronRight,
		Cpu,
		Settings,
		Bot,
		Brain,
		Wrench,
		Clock,
		Radio,
		Database,
		MessageSquare,
		Globe,
		Terminal
	} from '@lucide/svelte';
	import type { TreeNode } from '$lib/types';
	import type { ComponentType } from 'svelte';
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
	let open = $state(depth < 2);

	const hasChildren = $derived(node.children && node.children.length > 0);
	const isRoot = $derived(depth === 0);
	const isSelected = $derived(selectedId === node.id);

	function treeNodeIcon(node: TreeNode): ComponentType {
		const id = (node.id ?? '').toLowerCase();
		const name = (node.name ?? '').toLowerCase();
		if (isRoot) return Cpu;
		if (id === 'manage' || name.includes('management')) return Settings;
		if (id === 'agents' || name.includes('agent')) return Bot;
		if (id === 'llm') return Brain;
		if (id === 'tools') return Wrench;
		if (id === 'cron') return Clock;
		if (id.includes('comm') || name.includes('comms')) return Radio;
		if (id.includes('memory') || name.includes('memory')) return Database;
		if (id.startsWith('http') || name.includes('http')) return Globe;
		if (id.startsWith('pty') || name.includes('pty')) return Terminal;
		// Child agents (docs, chat, etc.)
		if (depth >= 1 && (id.length < 12 || name.includes('agent') || name.includes('chat') || name.includes('docs')))
			return MessageSquare;
		return Bot;
	}

	const NodeIcon = $derived(treeNodeIcon(node));

	function stateDotClass(state: string | undefined | null, status: string | undefined | null): string {
		const s = (state ?? '').toLowerCase();
		const st = (status ?? '').toLowerCase();
		if (s === 'on' && (st === 'running' || st === 'ok')) return 'bg-emerald-500';
		if (s === 'off') return 'border border-muted-foreground/40 bg-transparent';
		if (s === 'err' || st === 'error' || st === 'failed') return 'bg-destructive';
		if (st === 'degraded' || st === 'warning') return 'bg-yellow-500';
		return 'bg-emerald-500';
	}

	function statePillClass(state: string | undefined | null): string {
		const s = (state ?? '').toLowerCase();
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

	// Maps a subsystem node id/name to a colour kind (mirrors subsystemKind in status.ts).
	function nodeKind(node: TreeNode): string {
		const id = (node.id ?? '').toLowerCase();
		const name = (node.name ?? '').toLowerCase();
		if (id.includes('agent') || name.includes('agent')) return 'agents';
		if (id.includes('comm') || name.includes('comm')) return 'comms';
		if (id.includes('memory') || name.includes('memory')) return 'memory';
		if (id.includes('llm')) return 'llm';
		if (id.includes('manage') || name.includes('management')) return 'management';
		if (id.includes('tool') || name.includes('tool')) return 'tools';
		if (id.includes('cron') || name.includes('cron')) return 'cron';
		return 'default';
	}

	// Very light row tint applied only to depth-1 subsystem rows.
	function subsystemRowBg(node: TreeNode): string {
		if (depth !== 1) return '';
		switch (nodeKind(node)) {
			case 'agents':     return 'bg-violet-500/[0.06]';
			case 'comms':      return 'bg-sky-500/[0.06]';
			case 'memory':     return 'bg-amber-500/[0.06]';
			case 'llm':        return 'bg-emerald-500/[0.06]';
			case 'management': return 'bg-primary/[0.06]';
			case 'tools':      return 'bg-orange-500/[0.06]';
			case 'cron':       return 'bg-blue-500/[0.06]';
			default:           return 'bg-muted/30';
		}
	}

	const uptime = $derived(formatUptime(node.uptime_ms));
	const indent = $derived(depth * 12);
	const rowBg = $derived(subsystemRowBg(node));
	// depth-1 subsystem rows get a little extra vertical breathing room
	const rowPy = $derived(isRoot ? 'py-1' : depth === 1 ? 'py-1' : 'py-0.5');
</script>

<!-- Row wrapper: full-width, with light bottom border for gentle row separation -->
<div style="padding-left: {indent}px;" class="border-b border-border/20 last:border-b-0 {rowBg}">
	{#if hasChildren}
		<Collapsible.Root bind:open>
			<!-- Full-width click area, no highlight here; uses group for inner tab hover -->
			<Collapsible.Trigger
				onclick={() => onSelectNode?.(node)}
				class="group flex w-full items-center gap-1.5 text-left transition-colors {rowPy}"
			>
				<!-- ── Inner "tab": highlighted on select/hover, wraps only identity ── -->
				<span
					class="inline-flex min-w-0 shrink items-center gap-1 rounded-md px-1.5 py-0.5 transition-colors
						{isRoot ? 'font-medium' : ''}
						{isSelected
							? 'bg-primary/10 ring-1 ring-inset ring-primary/30'
							: 'group-hover:bg-muted/50'}"
				>
					<!-- expand/collapse chevron -->
					<ChevronRight
						class="size-3 shrink-0 text-muted-foreground/70 transition-transform duration-200
							{open ? 'rotate-90' : ''}"
					/>
					<!-- node type icon -->
					<svelte:component this={NodeIcon} class="size-3.5 shrink-0 text-muted-foreground/80" />
					<!-- status dot -->
					<span class={`size-1.5 shrink-0 rounded-full ${stateDotClass(node.state, node.status)}`}></span>
					<!-- name + id -->
					<span class="max-w-[160px] truncate {isRoot ? 'text-sm' : 'text-xs'}">
						{node.name}
						{#if !isRoot}
							<span class="text-muted-foreground/70">{node.id}</span>
						{/if}
					</span>
				</span>

				<!-- ── Right-hand metadata: no highlight, flush right ── -->
				<span class="ml-auto flex shrink-0 items-center gap-1.5 pr-1">
					<!-- child count badge -->
					<span class="rounded bg-muted/80 px-1 py-0.5 font-mono text-[10px] text-muted-foreground">
						{node.children.length}
					</span>
					<!-- uptime (root only) -->
					{#if uptime}
						<span class="hidden font-mono text-[10px] text-muted-foreground sm:inline">
							{uptime}
						</span>
					{/if}
					<!-- state pill -->
					<span
						class="rounded border px-1 py-0.5 text-[10px] font-medium uppercase tracking-wide {statePillClass(node.state)}"
					>
						{node.state}
					</span>
				</span>
			</Collapsible.Trigger>

			<Collapsible.Content
				class="overflow-hidden transition-all data-[state=open]:animate-none data-[state=closed]:animate-none"
			>
				<!-- Hierarchy connector: slightly more visible vertical line -->
				<div class="ml-2.5 border-l border-border/50">
					{#each node.children as child (child.id)}
						<Self node={child} depth={depth + 1} {selectedId} {onSelectNode} />
					{/each}
				</div>
			</Collapsible.Content>
		</Collapsible.Root>
	{:else}
		<!-- Leaf node: same pattern, no chevron -->
		<button
			type="button"
			onclick={() => onSelectNode?.(node)}
			class="group flex w-full items-center gap-1.5 text-left transition-colors {rowPy}"
		>
			<!-- ── Inner "tab" ── -->
			<span
				class="inline-flex min-w-0 shrink items-center gap-1 rounded-md px-1.5 py-0.5 transition-colors
					{isRoot ? 'font-medium' : ''}
					{isSelected
						? 'bg-primary/10 ring-1 ring-inset ring-primary/30'
						: 'group-hover:bg-muted/50'}"
			>
				<!-- spacer to align with chevron width -->
				<span class="size-3 shrink-0"></span>
				<!-- node type icon -->
				<svelte:component this={NodeIcon} class="size-3.5 shrink-0 text-muted-foreground/80" />
				<!-- status dot -->
				<span class={`size-1.5 shrink-0 rounded-full ${stateDotClass(node.state, node.status)}`}></span>
				<!-- name + id -->
				<span class="max-w-[160px] truncate {isRoot ? 'text-sm' : 'text-xs'}">
					{node.name}
					{#if !isRoot}
						<span class="text-muted-foreground/70">{node.id}</span>
					{/if}
				</span>
			</span>

			<!-- ── Right-hand metadata ── -->
			<span class="ml-auto flex shrink-0 items-center gap-1.5 pr-1">
				<!-- uptime (root only) -->
				{#if uptime}
					<span class="hidden font-mono text-[10px] text-muted-foreground sm:inline">
						{uptime}
					</span>
				{/if}
				<!-- state pill -->
				<span
					class="rounded border px-1 py-0.5 text-[10px] font-medium uppercase tracking-wide {statePillClass(node.state)}"
				>
					{node.state}
				</span>
			</span>
		</button>
	{/if}
</div>
