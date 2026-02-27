<script lang="ts">
	import { getContext } from 'svelte';
	import { page } from '$app/state';
	import ComponentDetailPanel from '$lib/components/ComponentDetailPanel.svelte';
	import { STATUS_ROUTE_CONTEXT, type StatusRouteContext } from '$lib/status-route-context';

	const context = getContext<StatusRouteContext>(STATUS_ROUTE_CONTEXT);
	const serviceInfo = context.serviceInfo;
	const nodeId = $derived(page.params.nodeId ?? '');
	const selectedNode = $derived(context.resolveNodeById(nodeId));
</script>

<svelte:head>
	<title>Araliya · Status · {nodeId || 'Node'}</title>
</svelte:head>

{#if selectedNode}
	<ComponentDetailPanel node={selectedNode} serviceInfo={$serviceInfo} />
{:else}
	<div class="flex flex-1 items-center justify-center p-6">
		<div class="max-w-sm rounded-lg border border-border/60 bg-card p-4 text-center">
			<p class="text-sm font-medium">Component not found</p>
			<p class="mt-1 text-xs text-muted-foreground">
				No status component matches id <span class="font-mono">{nodeId}</span>.
			</p>
		</div>
	</div>
{/if}
