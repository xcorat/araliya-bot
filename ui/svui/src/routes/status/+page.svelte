<script lang="ts">
	import { onMount } from 'svelte';
	import * as Sidebar from '$lib/components/ui/sidebar';
	import SessionSidebar from '$lib/components/SessionSidebar.svelte';
	import StatusView from '$lib/components/StatusView.svelte';
	import { ChatHeader } from '$lib/components/chat';
	import {
		initBaseUrl,
		doCheckHealth,
		getSessionId,
		getSessions,
		getAgents,
		getIsLoadingSessions,
		refreshSessions,
		refreshAgents,
		loadSessionHistory,
		resetSession
	} from '$lib/state.svelte';

	onMount(() => {
		initBaseUrl();
		doCheckHealth();
		void refreshSessions({ force: true });
		void refreshAgents();
	});

	const sessionId = $derived(getSessionId());
	const sessions = $derived(getSessions());
	const agents = $derived(getAgents());
	const loadingSessions = $derived(getIsLoadingSessions());

	function onSelectSession(targetSessionId: string) {
		void loadSessionHistory(targetSessionId);
	}

	function onNewSession() {
		resetSession();
	}
</script>

<svelte:head>
	<title>Araliya · Status</title>
</svelte:head>

<Sidebar.SidebarProvider>
	<SessionSidebar
		{sessions}
		{agents}
		activeSessionId={sessionId}
		isLoading={loadingSessions}
		{onSelectSession}
		{onNewSession}
	/>

	<Sidebar.SidebarInset>
		<div class="flex h-dvh flex-col">
			<ChatHeader />
			<StatusView />
		</div>
	</Sidebar.SidebarInset>
</Sidebar.SidebarProvider>
