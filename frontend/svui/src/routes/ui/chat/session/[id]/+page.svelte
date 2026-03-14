<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import * as Sidebar from '$lib/components/ui/sidebar';
	import SessionSidebar from '$lib/components/SessionSidebar.svelte';
	import { ChatMessages, ChatInput } from '$lib/components/chat';
	import {
		initBaseUrl,
		doCheckHealth,
		getMessages,
		getSessionId,
		getSessions,
		getAgents,
		getIsLoadingSessions,
		refreshSessions,
		refreshAgents,
		loadSessionHistory,
		resetSession,
		setSessionId
	} from '$lib/state.svelte';

	// Extract session ID from the URL path parameter.
	const routeSessionId = $derived(page.params.id);

	onMount(() => {
		initBaseUrl();
		doCheckHealth();
		void refreshSessions({ force: true });
		void refreshAgents();

		// If a session ID is in the URL, load that session's transcript.
		if (routeSessionId) {
			void loadSessionHistory(routeSessionId);
		}
	});

	// Re-load when the URL session ID changes (e.g. sidebar navigation).
	$effect(() => {
		if (routeSessionId) {
			void loadSessionHistory(routeSessionId);
		}
	});

	const messages = $derived(getMessages());
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
	<title>Araliya — Session {routeSessionId?.slice(0, 8) ?? ''}</title>
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
		<div class="flex h-full min-h-0 flex-1 flex-col overflow-hidden">
			<ChatMessages {messages} />
			<ChatInput />
		</div>
	</Sidebar.SidebarInset>
</Sidebar.SidebarProvider>
