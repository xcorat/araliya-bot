<script lang="ts">
	import { Server } from '@lucide/svelte';
	import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card';
	import type { HealthResponse } from '$lib/types';
	import { mainProcess, formatUptime, statusDotClass } from '$lib/utils/status';

	let {
		serviceInfo,
		error = ''
	}: {
		serviceInfo: HealthResponse | null;
		error?: string;
	} = $props();
</script>

<Card class="border-primary/20">
	<CardHeader class="pb-2 pt-3 px-3">
		<CardTitle class="flex items-center gap-2 text-xs font-medium">
			<Server class="size-3.5 text-primary" />
			Main Process
		</CardTitle>
	</CardHeader>
	<CardContent class="px-3 pb-3">
		{#if error}
			<p class="text-xs text-muted-foreground">{error}</p>
		{:else if serviceInfo}
			{@const process = mainProcess(serviceInfo)}
			<div class="space-y-2 text-xs">
				<div class="flex items-center justify-between">
					<span class="text-muted-foreground">Status</span>
					<div class="flex items-center gap-1.5">
						<span class={`size-1.5 rounded-full ${statusDotClass(process.status)}`}></span>
						<span class="font-medium capitalize">{process.status}</span>
					</div>
				</div>
				<div class="flex items-center justify-between">
					<span class="text-muted-foreground">Process</span>
					<span class="font-medium">{process.name}</span>
				</div>
				<div class="flex items-center justify-between">
					<span class="text-muted-foreground">Uptime</span>
					<span class="font-medium">{formatUptime(process.uptime_ms)}</span>
				</div>
				{#if serviceInfo.llm_provider || serviceInfo.llm_model}
					<div class="flex items-center justify-between gap-2">
						<span class="shrink-0 text-muted-foreground">LLM</span>
						<span class="truncate text-right font-mono text-[10px] text-foreground/80">
							{[serviceInfo.llm_provider, serviceInfo.llm_model].filter(Boolean).join(' · ')}
						</span>
					</div>
				{/if}
			</div>
		{:else}
			<p class="text-xs text-muted-foreground">Connecting…</p>
		{/if}
	</CardContent>
</Card>
