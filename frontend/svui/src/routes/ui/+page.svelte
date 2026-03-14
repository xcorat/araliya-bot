<script lang="ts">
	import { onMount } from 'svelte';
	import { ChatMessages } from '$lib/components/chat';
	import UniwebInput from './UniwebInput.svelte';
	import {
		initBaseUrl,
		doCheckHealth,
		getMessages,
		getSessionId,
		getIsLoading,
		loadSessionHistory,
		resetSession
	} from '$lib/state.svelte';

	onMount(() => {
		initBaseUrl();
		doCheckHealth();
		// The uniweb agent always returns its global session ID.
		// On first load, attempt to resume the shared transcript.
		resetSession();
	});

	const messages = $derived(getMessages());
	const sessionId = $derived(getSessionId());
	const isLoading = $derived(getIsLoading());
</script>

<svelte:head>
	<title>Araliya — Uniweb</title>
</svelte:head>

<div class="flex h-full min-h-0 flex-1 flex-col overflow-hidden">
	<header class="flex items-center justify-between border-b px-4 py-2">
		<div class="flex items-center gap-2">
			<h1 class="text-lg font-semibold">Front Porch</h1>
			<span class="text-xs text-muted-foreground">shared session — everyone sees the same conversation</span>
		</div>
		{#if isLoading}
			<span class="text-xs text-muted-foreground animate-pulse">processing…</span>
		{/if}
	</header>

	<ChatMessages {messages} />
	<UniwebInput />
</div>
