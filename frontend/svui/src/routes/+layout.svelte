<script lang="ts">
	import './layout.css';
	import { ModeWatcher } from 'mode-watcher';

	let { children } = $props();
	const version = import.meta.env.VITE_APP_VERSION || 'unknown';
</script>

<ModeWatcher defaultMode="dark" />
{@render children()}

<footer class="fixed bottom-0 right-0 p-2 text-xs text-gray-500">v{version}</footer>
