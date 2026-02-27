<script lang="ts">
	import { goto } from '$app/navigation';
	import { base } from '$app/paths';
	import { page } from '$app/state';
	import Button from '$lib/components/ui/button/button.svelte';
	import Badge from '$lib/components/ui/badge/badge.svelte';
	import ThemeToggle from '$lib/components/ThemeToggle.svelte';
	import {
		getHealthStatus,
		getSessionId,
		getLastUsage,
		getSessionUsageTotals,
		getWorkingMemoryUpdated,
		getIsLoading,
		doCheckHealth,
		resetSession
	} from '$lib/state.svelte';
	import { RotateCcw, Flower2, Activity, MessageSquare, BookOpen } from '@lucide/svelte';

	const health = $derived(getHealthStatus());
	const sid = $derived(getSessionId());
	const totals = $derived(getSessionUsageTotals());
	const wmUpdated = $derived(getWorkingMemoryUpdated());
	const loading = $derived(getIsLoading());

	const basePath = $derived(base || '');
	const chatPath = $derived(basePath ? `${basePath}/` : '/');
	const statusPath = $derived(basePath ? `${basePath}/status` : '/status');
	const docsPath = $derived(basePath ? `${basePath}/docs` : '/docs');
	const isChatRoute = $derived(page.url.pathname === chatPath);
	const isStatusRoute = $derived(
		page.url.pathname === statusPath || page.url.pathname.startsWith(`${statusPath}/`)
	);
	const isDocsRoute = $derived(page.url.pathname === docsPath || page.url.pathname.startsWith(`${docsPath}/`));

	const healthColor = $derived(
		health === 'ok'
			? 'bg-emerald-500'
			: health === 'error'
				? 'bg-destructive'
				: health === 'checking'
					? 'bg-yellow-500 animate-pulse'
					: 'bg-muted-foreground/40'
	);
	const healthPing = $derived(health === 'ok');

	function shortSession(id: string) {
		return id ? id.slice(0, 8) + '...' : 'none';
	}

	function formatCost(usd: number) {
		return usd < 0.01 ? '<$0.01' : `$${usd.toFixed(2)}`;
	}

	function openChat() {
		if (page.url.pathname !== chatPath) {
			void goto(chatPath);
		}
	}

	function openStatus() {
		if (!isStatusRoute) {
			void goto(statusPath);
		}
	}

	function openDocs() {
		if (!isDocsRoute) {
			void goto(docsPath);
		}
	}
</script>

<header
	class="relative flex items-center justify-between gap-3 border-b bg-background/80 px-4 py-2.5 backdrop-blur-sm"
>
	<div class="flex items-center gap-2.5">
		<Flower2 class="size-5 text-primary" />
		<span class="text-sm font-semibold">Araliya</span>
		<button
			onclick={() => doCheckHealth()}
			class="flex items-center gap-1.5 rounded-full px-2 py-0.5 text-xs text-muted-foreground transition-colors hover:bg-muted"
			title="Check health"
		>
			<span class="relative flex size-2">
				{#if healthPing}
					<span class="absolute inline-flex size-full animate-ping rounded-full bg-emerald-400 opacity-60"></span>
				{/if}
				<span class={`relative inline-flex size-full rounded-full ${healthColor}`}></span>
			</span>
			{health}
		</button>
	</div>

	<div class="flex items-center gap-1.5">
		<!-- View toggle -->
		<div class="flex items-center rounded-lg border bg-muted/50 p-0.5">
			<button
				onclick={openChat}
				class="flex items-center gap-1 rounded-md px-2 py-1 text-xs transition-colors {isChatRoute
					? 'bg-background text-foreground shadow-sm'
					: 'text-muted-foreground hover:text-foreground'}"
			>
				<MessageSquare class="size-3" />
				Chat
			</button>
			<button
				onclick={openStatus}
				class="flex items-center gap-1 rounded-md px-2 py-1 text-xs transition-colors {isStatusRoute
					? 'bg-background text-foreground shadow-sm'
					: 'text-muted-foreground hover:text-foreground'}"
			>
				<Activity class="size-3" />
				Status
			</button>
			<button
				onclick={openDocs}
				class="flex items-center gap-1 rounded-md px-2 py-1 text-xs transition-colors {isDocsRoute
					? 'bg-background text-foreground shadow-sm'
					: 'text-muted-foreground hover:text-foreground'}"
			>
				<BookOpen class="size-3" />
				Docs
			</button>
		</div>

		{#if sid}
			<Badge variant="secondary" class="font-mono text-[10px]">
				{shortSession(sid)}
			</Badge>
		{/if}
		{#if wmUpdated}
			<Badge variant="outline" class="text-[10px]">WM updated</Badge>
		{/if}
		{#if totals}
			<Badge variant="outline" class="hidden text-[10px] lg:inline-flex">
				{totals.total_tokens} tok &middot; {formatCost(totals.estimated_cost_usd)}
			</Badge>
		{/if}
		<Button variant="ghost" size="icon" onclick={resetSession} title="New session" class="size-8">
			<RotateCcw class="size-3.5" />
		</Button>
		<ThemeToggle />
	</div>
	{#if loading}
		<div class="loading-bar" aria-hidden="true"></div>
	{/if}
</header>
