<script lang="ts">
	import { page } from '$app/state';
	import StatusObservePanel from '$lib/components/StatusObservePanel.svelte';

	const filter = $derived(page.params.filter ?? '');
</script>

<svelte:head>
	<title>Araliya · Status · Observe · {filter}</title>
</svelte:head>

<StatusObservePanel {filter} />
