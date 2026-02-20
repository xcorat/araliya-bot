<script lang="ts">
	import { toggleMode, mode } from 'mode-watcher';
	import { Sun, Moon } from '@lucide/svelte';
	import Button from '$lib/components/ui/button/button.svelte';
</script>

<Button variant="ghost" size="icon" onclick={toggleMode} aria-label="Toggle theme">
	{#if mode.current === 'dark'}
		<Sun class="size-4" />
	{:else}
		<Moon class="size-4" />
	{/if}
</Button>
