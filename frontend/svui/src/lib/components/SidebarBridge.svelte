<script lang="ts">
	import { onDestroy } from 'svelte';
	import { useSidebar } from '$lib/components/ui/sidebar/context.svelte.js';
	import { registerSidebarToggle, unregisterSidebarToggle } from '$lib/sidebar-bridge.svelte';

	const sidebar = useSidebar();
	registerSidebarToggle(() => sidebar.toggle());
	onDestroy(unregisterSidebarToggle);
</script>
