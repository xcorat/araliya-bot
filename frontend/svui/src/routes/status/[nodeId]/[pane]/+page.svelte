<script lang="ts">
	import { getContext } from 'svelte';
	import { page } from '$app/state';
	import ComponentDetailPanel from '$lib/components/ComponentDetailPanel.svelte';
	import StatusMemoryInspector from '$lib/components/StatusMemoryInspector.svelte';
	import { STATUS_ROUTE_CONTEXT, type StatusRouteContext } from '$lib/status-route-context';

	const context = getContext<StatusRouteContext>(STATUS_ROUTE_CONTEXT);
	const serviceInfo = context.serviceInfo;
	const nodeId = $derived(page.params.nodeId ?? '');
	const pane = $derived((page.params.pane ?? '').toLowerCase());
	const selectedNode = $derived(context.resolveNodeById(nodeId));
	const isDetailsPane = $derived(pane === 'details');
	const isMemoryPane = $derived(pane === 'memory');
</script>

<svelte:head>
	<title>Araliya · Status · {nodeId || 'Node'} · {pane || 'Pane'}</title>
</svelte:head>

{#if !selectedNode}
	<div class="flex flex-1 items-center justify-center p-6">
		<div class="max-w-sm rounded-lg border border-border/60 bg-card p-4 text-center">
			<p class="text-sm font-medium">Component not found</p>
			<p class="mt-1 text-xs text-muted-foreground">
				No status component matches id <span class="font-mono">{nodeId}</span>.
			</p>
		</div>
	</div>
{:else if isDetailsPane}
	<ComponentDetailPanel node={selectedNode} serviceInfo={$serviceInfo} />
{:else if isMemoryPane}
	<StatusMemoryInspector node={selectedNode} />
{:else}
	<div class="flex flex-1 items-center justify-center p-6">
		<div class="max-w-sm rounded-lg border border-border/60 bg-card p-4 text-center">
			<p class="text-sm font-medium">Unknown status pane</p>
			<p class="mt-1 text-xs text-muted-foreground">
				Pane <span class="font-mono">{pane}</span> is not available yet.
			</p>
		</div>
	</div>
{/if}
