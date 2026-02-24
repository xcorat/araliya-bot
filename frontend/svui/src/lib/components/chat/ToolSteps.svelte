<script lang="ts">
	import type { ToolStep } from '$lib/types';
	import { cn } from '$lib/utils';
	import { ChevronRight, Wrench, Check, X, ChevronDown } from '@lucide/svelte';

	let { steps }: { steps: ToolStep[] } = $props();

	let expanded = $state(false);
	let expandedResults = $state<Set<number>>(new Set());

	const hasErrors = $derived(steps.some((s) => s.result.startsWith('Error:')));
	const summary = $derived(() => {
		const count = steps.length;
		const names = [...new Set(steps.map((s) => s.tool_name))];
		if (names.length === 1) {
			return `${count} ${names[0]} call${count > 1 ? 's' : ''}`;
		}
		return `${count} tool call${count > 1 ? 's' : ''}`;
	});

	function formatArgs(args: Record<string, unknown>): string {
		const entries = Object.entries(args);
		if (entries.length === 0) return '';
		return entries
			.map(([k, v]) => {
				const val = typeof v === 'string' ? v : JSON.stringify(v);
				const short = val.length > 60 ? val.slice(0, 57) + '...' : val;
				return `${k}: ${short}`;
			})
			.join(', ');
	}

	function analyzeResult(result: string): {
		truncated: string;
		full: string;
		isTruncated: boolean;
		extraLines: number;
	} {
		const lines = result.split('\n');
		const isTruncated = lines.length > 6;

		if (!isTruncated) {
			return { truncated: result, full: result, isTruncated: false, extraLines: 0 };
		}

		return {
			truncated: lines.slice(0, 5).join('\n'),
			full: result,
			isTruncated: true,
			extraLines: lines.length - 5
		};
	}

	function toggleResult(index: number) {
		const newSet = new Set(expandedResults);
		if (newSet.has(index)) {
			newSet.delete(index);
		} else {
			newSet.add(index);
		}
		expandedResults = newSet;
	}
</script>

<button
	onclick={() => (expanded = !expanded)}
	class={cn(
		'group flex w-full items-center gap-1.5 rounded-lg px-2 py-1 text-xs transition-colors hover:bg-muted/80',
		hasErrors ? 'text-destructive/80' : 'text-muted-foreground'
	)}
>
	<ChevronRight
		class={cn('size-3 shrink-0 transition-transform duration-150', expanded && 'rotate-90')}
	/>
	<Wrench class="size-3 shrink-0" />
	<span class="truncate">{summary()}</span>
</button>

{#if expanded}
	<div class="ml-3 mt-1 space-y-2 border-l-2 border-muted pl-3">
		{#each steps as step, i}
			{@const resultData = analyzeResult(step.result)}
			{@const isExpanded = expandedResults.has(i)}
			<div class="space-y-0.5">
				<div class="flex items-center gap-1.5 text-xs font-medium text-foreground/80">
					{#if step.result.startsWith('Error:')}
						<X class="size-3 text-destructive" />
					{:else}
						<Check class="size-3 text-emerald-500" />
					{/if}
					<span class="font-mono">{step.tool_name}</span>
					{#if formatArgs(step.arguments)}
						<span class="truncate font-normal text-muted-foreground"
							>({formatArgs(step.arguments)})</span
						>
					{/if}
				</div>
				<div class="relative">
					<pre
						class={cn(
							'mt-0.5 whitespace-pre-wrap break-words rounded-md px-2 py-1 font-mono text-[11px] leading-relaxed',
							step.result.startsWith('Error:')
								? 'bg-destructive/5 text-destructive/80'
								: 'bg-muted/50 text-muted-foreground'
						)}>{isExpanded ? resultData.full : resultData.truncated}</pre>
					{#if resultData.isTruncated}
						<button
							onclick={() => toggleResult(i)}
							class="mt-1 flex items-center gap-1 text-[10px] text-muted-foreground transition-colors hover:text-foreground"
						>
							<ChevronDown
								class={cn(
									'size-3 transition-transform duration-150',
									isExpanded && 'rotate-180'
								)}
							/>
							{isExpanded
								? 'Show less'
								: `Show ${resultData.extraLines} more line${resultData.extraLines === 1 ? '' : 's'}`}
						</button>
					{/if}
				</div>
			</div>
		{/each}
	</div>
{/if}
