<script lang="ts">
	import './layout.css';
	import { ModeWatcher } from 'mode-watcher';
	import { ChatHeader } from '$lib/components/chat';

	let { children } = $props();
	const version = import.meta.env.VITE_APP_VERSION || 'unknown';
</script>

<ModeWatcher defaultMode="dark" />
<div class="flex min-h-dvh flex-col bg-background">
	<ChatHeader />
	<main class="min-h-0 flex-1 overflow-hidden">
		{@render children()}
	</main>
	<footer class="px-3 py-1 text-right text-xs text-gray-500">v{version}</footer>
</div>
