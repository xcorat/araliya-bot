<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import * as Sidebar from '$lib/components/ui/sidebar';
	import StatusSidebar from '$lib/components/StatusSidebar.svelte';
	import StatusView from '$lib/components/StatusView.svelte';
	import ComponentDetailPanel from '$lib/components/ComponentDetailPanel.svelte';
	import { ChatHeader } from '$lib/components/chat';
	import { initBaseUrl, doCheckHealth, getBaseUrl } from '$lib/state.svelte';
	import * as api from '$lib/api';
	import type { HealthResponse, TreeNode } from '$lib/types';

	const POLL_INTERVAL_MS = 4000;

	let serviceInfo = $state<HealthResponse | null>(null);
	let treeData = $state<TreeNode | null>(null);
	let treeError = $state('');
	let selectedNode = $state<TreeNode | null>(null);
	let isPolling = $state(true);
	let loading = $state(false);
	let error = $state('');
	let lastRefresh = $state('');
	let pollTimer: ReturnType<typeof setInterval> | null = null;

	onMount(() => {
		initBaseUrl();
		doCheckHealth();
		void fetchAll();
		startPolling();
	});

	onDestroy(() => {
		stopPolling();
	});

	function startPolling() {
		stopPolling();
		if (isPolling) {
			pollTimer = setInterval(() => {
				void fetchAll();
			}, POLL_INTERVAL_MS);
		}
	}

	function stopPolling() {
		if (pollTimer) {
			clearInterval(pollTimer);
			pollTimer = null;
		}
	}

	function togglePolling() {
		isPolling = !isPolling;
		if (isPolling) {
			startPolling();
			void fetchAll();
		} else {
			stopPolling();
		}
	}

	async function fetchAll() {
		if (loading) return;
		const baseUrl = getBaseUrl();
		if (!baseUrl) return;

		loading = true;
		error = '';
		treeError = '';
		try {
			const [healthRes, treeRes] = await Promise.allSettled([
				api.checkHealth(baseUrl),
				api.fetchComponentTree(baseUrl)
			]);

			if (healthRes.status === 'fulfilled') {
				serviceInfo = healthRes.value;
			} else {
				error =
					healthRes.reason instanceof Error
						? healthRes.reason.message
						: 'Failed to fetch status data';
			}

			if (treeRes.status === 'fulfilled') {
				treeData = treeRes.value;
			} else {
				treeError =
					treeRes.reason instanceof Error
						? treeRes.reason.message
						: 'Failed to fetch component tree';
			}

			lastRefresh = new Date().toLocaleTimeString();
		} finally {
			loading = false;
		}
	}
</script>

<svelte:head>
	<title>Araliya · Status</title>
</svelte:head>

<Sidebar.SidebarProvider>
	<StatusSidebar
		{serviceInfo}
		{error}
		{treeData}
		{treeError}
		selectedNodeId={selectedNode?.id ?? ''}
		onSelectNode={(node) => (selectedNode = node)}
	/>

	<Sidebar.SidebarInset>
		<div class="flex h-dvh flex-col">
			<ChatHeader />
			<div class="flex min-h-0 flex-1">
				<StatusView
					{serviceInfo}
					{error}
					{loading}
					{lastRefresh}
					{isPolling}
					onTogglePolling={togglePolling}
					onRefresh={() => void fetchAll()}
				/>
				<ComponentDetailPanel node={selectedNode} {serviceInfo} />
			</div>
		</div>
	</Sidebar.SidebarInset>
</Sidebar.SidebarProvider>
