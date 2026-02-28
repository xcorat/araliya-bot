<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { Activity, Loader2, Copy, Check } from '@lucide/svelte';
	import type { TreeNode, SessionDebugTurn } from '$lib/types';
	import { getBaseUrl } from '$lib/state.svelte';
	import * as api from '$lib/api';

	let { node }: { node: TreeNode } = $props();

	const sessionId = $derived(page.url.searchParams.get('session') ?? '');

	let loading = $state(false);
	let error = $state('');
	let turns = $state<SessionDebugTurn[]>([]);
	let hydrated = $state(false);
	let diagramSource = $state('');
	let copied = $state(false);

	let chartContainer: HTMLDivElement | undefined = $state();

	async function loadDebug() {
		if (!sessionId) {
			error = 'No session selected.';
			hydrated = true;
			return;
		}
		const baseUrl = getBaseUrl();
		if (!baseUrl) return;
		loading = true;
		error = '';
		try {
			const res = await api.getSessionDebug(baseUrl, sessionId);
			turns = res.turns ?? [];
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load debug data';
		} finally {
			loading = false;
			hydrated = true;
		}
	}

	function buildDiagram(turns: SessionDebugTurn[]): string {
		const lines: string[] = [
			'sequenceDiagram',
			'  actor User',
			'  participant Agent',
			'  participant ILlm as Instruction LLM',
			'  participant Tool as Tool(s)',
			'  participant MLlm as Response LLM',
		];

		for (const turn of turns) {
			lines.push(`  Note over User,MLlm: Turn ${turn.n}`);

			const input = truncate(turn.user_input, 60);
			lines.push(`  User->>Agent: ${JSON.stringify(input)}`);

			const ipLen = turn.instruct_prompt.length;
			lines.push(`  Agent->>ILlm: instruct_prompt (${ipLen} chars)`);

			// Parse tool calls to show per-tool arrows
			let toolCalls: Array<{ tool: string; action: string }> = [];
			try {
				const raw = turn.tool_calls_json.trim();
				if (raw && raw !== '[]') {
					const parsed = JSON.parse(raw);
					if (Array.isArray(parsed)) {
						toolCalls = parsed.map((c: { tool?: string; action?: string }) => ({
							tool: String(c.tool ?? 'unknown'),
							action: String(c.action ?? '?'),
						}));
					}
				}
			} catch {
				// ignore parse errors
			}

			if (toolCalls.length > 0) {
				lines.push(`  ILlm-->>Agent: tool_calls (${toolCalls.length})`);
				for (const tc of toolCalls) {
					lines.push(`  Agent->>Tool: ${tc.tool}/${tc.action}`);
					lines.push(`  Tool-->>Agent: output`);
				}
			} else {
				lines.push(`  ILlm-->>Agent: (no tool calls)`);
			}

			const rpLen = turn.response_prompt.length;
			lines.push(`  Agent->>MLlm: response_prompt (${rpLen} chars)`);
			lines.push(`  MLlm-->>Agent: assistant reply`);
			lines.push(`  Agent-->>User: reply`);
		}

		return lines.join('\n');
	}

	function truncate(s: string, max: number): string {
		const line = s.split('\n')[0].trim();
		return line.length > max ? line.slice(0, max) + '…' : line;
	}

	async function renderDiagram() {
		if (!chartContainer || turns.length === 0) return;
		const src = buildDiagram(turns);
		diagramSource = src;
		try {
			const mermaid = (await import('mermaid')).default;
			mermaid.initialize({ startOnLoad: false, theme: 'neutral' });
			const id = `debug-chart-${Date.now()}`;
			const { svg } = await mermaid.render(id, src);
			chartContainer.innerHTML = svg;
		} catch (e) {
			chartContainer.innerHTML = `<pre class="text-xs text-destructive">${String(e)}</pre>`;
		}
	}

	async function copyDiagram() {
		if (!diagramSource) return;
		await navigator.clipboard.writeText(diagramSource);
		copied = true;
		setTimeout(() => (copied = false), 2000);
	}

	$effect(() => {
		if (!hydrated && !loading) void loadDebug();
	});

	$effect(() => {
		if (hydrated && turns.length > 0) void renderDiagram();
	});
</script>

<div class="flex h-full flex-1 flex-col overflow-hidden">
	<!-- Header -->
	<div class="flex items-center gap-3 border-b p-4">
		<div class="rounded-md border border-border/50 bg-muted/20 p-1.5">
			<Activity class="size-4 text-muted-foreground" />
		</div>
		<div class="min-w-0 flex-1">
			<h3 class="text-sm font-semibold">Agent Debug Flowchart</h3>
			<p class="mt-0.5 font-mono text-[10px] text-muted-foreground">
				{sessionId ? sessionId.slice(0, 16) + '…' : 'no session'}
			</p>
		</div>
		{#if turns.length > 0}
			<div class="flex items-center gap-2">
				<span class="text-[10px] text-muted-foreground">{turns.length} turn{turns.length !== 1 ? 's' : ''}</span>
				<button
					type="button"
					onclick={copyDiagram}
					class="flex items-center gap-1 rounded border border-border/50 bg-muted/20 px-2 py-1 text-[10px] text-muted-foreground transition-colors hover:bg-muted/40"
					title="Copy diagram source"
				>
					{#if copied}
						<Check class="size-3" />
						Copied
					{:else}
						<Copy class="size-3" />
						Copy source
					{/if}
				</button>
			</div>
		{/if}
	</div>

	<!-- Body -->
	{#if loading}
		<div class="flex flex-1 items-center justify-center gap-2 text-xs text-muted-foreground">
			<Loader2 class="size-4 animate-spin" />
			<span>Loading debug data…</span>
		</div>
	{:else if error}
		<div class="p-4">
			<p class="text-xs text-destructive">{error}</p>
		</div>
	{:else if turns.length === 0 && hydrated}
		<div class="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center">
			<div class="rounded-full border border-border/50 bg-muted/30 p-4">
				<Activity class="size-6 text-muted-foreground/40" />
			</div>
			<div>
				<p class="text-sm font-medium text-foreground/70">No debug data for this session</p>
				<p class="mt-1 text-xs text-muted-foreground">
					Set <code class="font-mono">debug_logging = true</code> in the
					<code class="font-mono">[agents]</code> config section, then send messages to this agent.
				</p>
			</div>
		</div>
	{:else}
		<div class="flex-1 overflow-auto p-4">
			<div bind:this={chartContainer} class="min-w-0"></div>
		</div>
	{/if}
</div>
