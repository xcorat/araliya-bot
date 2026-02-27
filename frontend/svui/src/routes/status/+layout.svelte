<script lang="ts">
	import { onMount, onDestroy, setContext } from 'svelte';
	import { goto } from '$app/navigation';
	import { base } from '$app/paths';
	import { page } from '$app/state';
	import { writable } from 'svelte/store';
	import * as Sidebar from '$lib/components/ui/sidebar';
	import StatusSidebar from '$lib/components/StatusSidebar.svelte';
	import { initBaseUrl, doCheckHealth, getBaseUrl } from '$lib/state.svelte';
	import * as api from '$lib/api';
	import type { HealthResponse, TreeNode } from '$lib/types';
	import {
		STATUS_ROUTE_CONTEXT,
		type StatusRouteContext,
		findTreeNodeById
	} from '$lib/status-route-context';

	const POLL_INTERVAL_MS = 4000;

	let { children } = $props();

	const serviceInfo = writable<HealthResponse | null>(null);
	const treeData = writable<TreeNode | null>(null);
	const error = writable('');
	const treeError = writable('');
	const loading = writable(false);
	const lastRefresh = writable('');
	const isPolling = writable(true);

	let currentTreeData: TreeNode | null = null;
	let currentLoading = false;
	let pollingEnabled = true;
	let pollTimer: ReturnType<typeof setInterval> | null = null;

	const basePath = $derived(base || '');
	const statusPath = $derived(basePath ? `${basePath}/status` : '/status');
	const selectedNodeId = $derived(page.params.nodeId ?? '');

	const contextValue: StatusRouteContext = {
		serviceInfo,
		treeData,
		error,
		treeError,
		loading,
		lastRefresh,
		isPolling,
		fetchAll,
		togglePolling,
		resolveNodeById: (id: string) => findTreeNodeById(currentTreeData, id)
	};

	setContext(STATUS_ROUTE_CONTEXT, contextValue);

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
		if (pollingEnabled) {
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
		pollingEnabled = !pollingEnabled;
		isPolling.set(pollingEnabled);
		if (pollingEnabled) {
			startPolling();
			void fetchAll();
		} else {
			stopPolling();
		}
	}

	async function fetchAll() {
		if (currentLoading) return;
		const baseUrl = getBaseUrl();
		if (!baseUrl) return;

		currentLoading = true;
		loading.set(true);
		error.set('');
		treeError.set('');
		try {
			const [healthRes, treeRes] = await Promise.allSettled([
				api.checkHealth(baseUrl),
				api.fetchComponentTree(baseUrl)
			]);

			if (healthRes.status === 'fulfilled') {
				serviceInfo.set(healthRes.value);
			} else {
				error.set(
					healthRes.reason instanceof Error
						? healthRes.reason.message
						: 'Failed to fetch status data'
				);
			}

			if (treeRes.status === 'fulfilled') {
				currentTreeData = treeRes.value;
				treeData.set(currentTreeData);
			} else {
				treeError.set(
					treeRes.reason instanceof Error
						? treeRes.reason.message
						: 'Failed to fetch component tree'
				);
			}

			lastRefresh.set(new Date().toLocaleTimeString());
		} finally {
			currentLoading = false;
			loading.set(false);
		}
	}

	function onSelectNode(node: TreeNode) {
		void goto(`${statusPath}/${encodeURIComponent(node.id)}`);
	}
</script>

<Sidebar.SidebarProvider>
	<StatusSidebar
		serviceInfo={$serviceInfo}
		error={$error}
		treeData={$treeData}
		treeError={$treeError}
		{selectedNodeId}
		{onSelectNode}
	/>

	<Sidebar.SidebarInset>
		<div class="flex h-full min-h-0 flex-1 flex-col overflow-hidden">
			{@render children()}
		</div>
	</Sidebar.SidebarInset>
</Sidebar.SidebarProvider>
