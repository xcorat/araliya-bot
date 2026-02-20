<script lang="ts">
	import { onMount } from 'svelte';
	import * as Sidebar from '$lib/components/ui/sidebar';
	import SessionSidebar from '$lib/components/SessionSidebar.svelte';
	import StatusView from '$lib/components/StatusView.svelte';
	import { ChatHeader, ChatMessages, ChatInput } from '$lib/components/chat';
	import {
		initBaseUrl,
		doCheckHealth,
		getMessages,
		getSessionId,
		getSessions,
		getIsLoadingSessions,
		getActiveView,
		refreshSessions,
		loadSessionHistory,
		resetSession
	} from '$lib/state.svelte';

	onMount(() => {
		initBaseUrl();
		doCheckHealth();
		void refreshSessions({ force: true });
	});

	const messages = $derived(getMessages());
	const sessionId = $derived(getSessionId());
	const sessions = $derived(getSessions());
	const loadingSessions = $derived(getIsLoadingSessions());
	const activeView = $derived(getActiveView());

	function onSelectSession(targetSessionId: string) {
		void loadSessionHistory(targetSessionId);
	}

	function onNewSession() {
		resetSession();
	}
</script>

<svelte:head>
	<title>Araliya</title>
</svelte:head>

<Sidebar.SidebarProvider>
	<SessionSidebar
		{sessions}
		activeSessionId={sessionId}
		isLoading={loadingSessions}
		{onSelectSession}
		{onNewSession}
	/>

	<Sidebar.SidebarInset>
		<div class="flex h-dvh flex-col">
			<ChatHeader />

			{#if activeView === 'chat'}
				<ChatMessages {messages} />
				<ChatInput />
			{:else}
				<StatusView />
			{/if}
		</div>
	</Sidebar.SidebarInset>
</Sidebar.SidebarProvider>
