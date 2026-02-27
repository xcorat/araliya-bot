<script lang="ts">
	import { getContext } from 'svelte';
	import StatusView from '$lib/components/StatusView.svelte';
	import { STATUS_ROUTE_CONTEXT, type StatusRouteContext } from '$lib/status-route-context';

	const context = getContext<StatusRouteContext>(STATUS_ROUTE_CONTEXT);
	const serviceInfo = context.serviceInfo;
	const error = context.error;
	const loading = context.loading;
	const lastRefresh = context.lastRefresh;
	const isPolling = context.isPolling;
</script>

<svelte:head>
	<title>Araliya · Status</title>
</svelte:head>

<StatusView
	serviceInfo={$serviceInfo}
	error={$error}
	loading={$loading}
	lastRefresh={$lastRefresh}
	isPolling={$isPolling}
	onTogglePolling={context.togglePolling}
	onRefresh={() => void context.fetchAll()}
/>
