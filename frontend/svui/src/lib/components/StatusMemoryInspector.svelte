<script lang="ts">
	import { base } from '$app/paths';
	import Badge from '$lib/components/ui/badge/badge.svelte';
	import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card';
	import { Loader2, Database, FileText } from '@lucide/svelte';
	import type { TreeNode, SessionInfo, AgentInfo, SessionFileInfo } from '$lib/types';
	import { getBaseUrl } from '$lib/state.svelte';
	import * as api from '$lib/api';

	let { node }: { node: TreeNode } = $props();

	let sessions = $state<SessionInfo[]>([]);
	let agents = $state<AgentInfo[]>([]);
	let sessionsLoading = $state(false);
	let agentsLoading = $state(false);
	let sessionsError = $state('');
	let agentsError = $state('');

	let filesBySession = $state<
		Record<string, { loading: boolean; loaded: boolean; error: string; files: SessionFileInfo[] }>
	>({});
	let memoryBySession = $state<
		Record<string, { loading: boolean; loaded: boolean; error: string; content: string }>
	>({});

	let activeStore = $state<string | null>(null);
	let activeFile = $state<{ sessionId: string; file: SessionFileInfo } | null>(null);

	let sessionsHydrated = $state(false);
	let agentsHydrated = $state(false);

	const uiBase = $derived(base || '');
	const selectedAgent = $derived(agents.find((agent) => agent.agent_id === node.id));
	const selectedAgentId = $derived(selectedAgent?.agent_id ?? node.id);
	const visibleSessions = $derived(sessions.filter((session) => session.last_agent === selectedAgentId));
	const storeTypes = $derived(
		Array.from(
			new Set([
				...(selectedAgent?.store_types ?? []),
				...visibleSessions.flatMap((session) =>
					Array.isArray(session.store_types) ? session.store_types : []
				)
			])
		).sort()
	);
	const sortedSessions = $derived.by(() =>
		[...visibleSessions].sort((left, right) => {
			const leftTs = Date.parse(left.updated_at ?? left.created_at ?? '');
			const rightTs = Date.parse(right.updated_at ?? right.created_at ?? '');
			return rightTs - leftTs;
		})
	);
	const primarySessionId = $derived(sortedSessions[0]?.session_id ?? null);
	const hasAnySessionFiles = $derived.by(() =>
		sortedSessions.some((session) => {
			const state = filesBySession[session.session_id];
			return !!state?.loaded && state.files.length > 0;
		})
	);

	$effect(() => {
		if (!sessionsHydrated && !sessionsLoading) {
			void loadSessions();
		}
		if (!agentsHydrated && !agentsLoading) {
			void loadAgents();
		}
	});

	$effect(() => {
		for (const session of sortedSessions) {
			if (!filesBySession[session.session_id]?.loaded && !filesBySession[session.session_id]?.loading) {
				void loadSessionFiles(session.session_id);
			}
		}
	});

	async function loadSessions() {
		const baseUrl = getBaseUrl();
		if (!baseUrl) return;
		sessionsLoading = true;
		sessionsError = '';
		try {
			const response = await api.listSessions(baseUrl);
			sessions = response.sessions;
		} catch (error) {
			sessionsError = error instanceof Error ? error.message : 'Failed to load sessions';
		} finally {
			sessionsLoading = false;
			sessionsHydrated = true;
		}
	}

	async function loadAgents() {
		const baseUrl = getBaseUrl();
		if (!baseUrl) return;
		agentsLoading = true;
		agentsError = '';
		try {
			const response = await api.listAgents(baseUrl);
			agents = response.agents;
		} catch (error) {
			agentsError = error instanceof Error ? error.message : 'Failed to load agents';
		} finally {
			agentsLoading = false;
			agentsHydrated = true;
		}
	}

	async function loadSessionFiles(sessionId: string) {
		const current = filesBySession[sessionId];
		if (current?.loading || current?.loaded) {
			return;
		}

		filesBySession = {
			...filesBySession,
			[sessionId]: { loading: true, loaded: false, error: '', files: [] }
		};

		const baseUrl = getBaseUrl();
		if (!baseUrl) {
			filesBySession = {
				...filesBySession,
				[sessionId]: { loading: false, loaded: false, error: 'Missing base URL', files: [] }
			};
			return;
		}

		try {
			const response = await api.getSessionFiles(baseUrl, sessionId);
			filesBySession = {
				...filesBySession,
				[sessionId]: { loading: false, loaded: true, error: '', files: response.files }
			};
		} catch (error) {
			filesBySession = {
				...filesBySession,
				[sessionId]: {
					loading: false,
					loaded: false,
					error: error instanceof Error ? error.message : 'Failed to load files',
					files: []
				}
			};
		}
	}

	async function ensureMemory(sessionId: string) {
		const current = memoryBySession[sessionId];
		if (current?.loaded || current?.loading) {
			return;
		}

		memoryBySession = {
			...memoryBySession,
			[sessionId]: { loading: true, loaded: false, error: '', content: '' }
		};

		const baseUrl = getBaseUrl();
		if (!baseUrl) {
			memoryBySession = {
				...memoryBySession,
				[sessionId]: { loading: false, loaded: false, error: 'Missing base URL', content: '' }
			};
			return;
		}

		try {
			const response = await api.getSessionMemory(baseUrl, sessionId);
			memoryBySession = {
				...memoryBySession,
				[sessionId]: { loading: false, loaded: true, error: '', content: response.content }
			};
		} catch (error) {
			memoryBySession = {
				...memoryBySession,
				[sessionId]: {
					loading: false,
					loaded: false,
					error: error instanceof Error ? error.message : 'Failed to load memory',
					content: ''
				}
			};
		}
	}

	function onStoreClick(storeType: string) {
		activeStore = storeType;
		activeFile = null;

		if ((storeType.toLowerCase().includes('memory') || storeType.toLowerCase() === 'wm') && primarySessionId) {
			void ensureMemory(primarySessionId);
		}
	}

	function onFileClick(sessionId: string, file: SessionFileInfo) {
		activeFile = { sessionId, file };
		activeStore = null;
	}

	function formatTime(iso: string): string {
		const parsed = Date.parse(iso);
		if (Number.isNaN(parsed)) {
			return iso;
		}
		return new Date(parsed).toLocaleString();
	}

	function formatBytes(bytes: number): string {
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
		return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
	}

	function shortSession(id: string): string {
		return id.length > 10 ? `${id.slice(0, 8)}…` : id;
	}
</script>

<div class="flex flex-1 flex-col overflow-y-auto p-4">
	<div class="space-y-3">
		<Card>
			<CardHeader class="pb-2">
				<CardTitle class="text-sm">Memory Inspector</CardTitle>
			</CardHeader>
			<CardContent class="space-y-2 text-xs">
				<div class="flex flex-wrap items-center gap-1.5">
					<Badge variant="outline">Agent {selectedAgentId}</Badge>
					<Badge variant="outline">Sessions {visibleSessions.length}</Badge>
					<Badge variant="outline">Stores {storeTypes.length}</Badge>
				</div>
				{#if sessionsError}
					<p class="text-destructive">{sessionsError}</p>
				{/if}
				{#if agentsError}
					<p class="text-destructive">{agentsError}</p>
				{/if}
			</CardContent>
		</Card>

		<Card>
			<CardHeader class="pb-2">
				<CardTitle class="text-xs uppercase tracking-wide text-muted-foreground">Store Types</CardTitle>
			</CardHeader>
			<CardContent>
				{#if sessionsLoading || agentsLoading}
					<div class="flex items-center gap-2 text-xs text-muted-foreground">
						<Loader2 class="size-3.5 animate-spin" />
						<span>Loading stores…</span>
					</div>
				{:else if storeTypes.length === 0}
					<p class="text-xs text-muted-foreground">No enabled stores found for this agent.</p>
				{:else}
					<div class="flex flex-wrap gap-1.5">
						{#each storeTypes as storeType}
							<button
								type="button"
								onclick={() => onStoreClick(storeType)}
								class="rounded border px-2 py-1 text-[11px] transition-colors hover:bg-muted/40 {activeStore === storeType ? 'bg-muted' : ''}"
							>
								{storeType}
							</button>
						{/each}
					</div>
				{/if}
			</CardContent>
		</Card>

		<Card>
			<CardHeader class="pb-2">
				<CardTitle class="text-xs uppercase tracking-wide text-muted-foreground">Session Files</CardTitle>
			</CardHeader>
			<CardContent>
				{#if sessionsLoading}
					<div class="flex items-center gap-2 text-xs text-muted-foreground">
						<Loader2 class="size-3.5 animate-spin" />
						<span>Loading sessions…</span>
					</div>
				{:else if visibleSessions.length === 0}
					<p class="text-xs text-muted-foreground">No sessions found for this agent.</p>
				{:else if !hasAnySessionFiles}
					<p class="text-xs text-muted-foreground">No files discovered yet.</p>
				{:else}
					<div class="space-y-2">
						{#each sortedSessions as session (session.session_id)}
							{@const sessionState = filesBySession[session.session_id]}
							<div class="rounded border border-border/60 p-2">
								<div class="mb-1.5 flex items-center justify-between gap-2">
									<div class="min-w-0">
										<p class="truncate font-mono text-[11px]">{session.session_id}</p>
										<p class="text-[10px] text-muted-foreground">
											{formatTime(session.updated_at ?? session.created_at)}
										</p>
									</div>
									<Badge variant="outline" class="text-[10px]">{shortSession(session.session_id)}</Badge>
								</div>

								{#if sessionState?.loading}
									<div class="flex items-center gap-2 text-[11px] text-muted-foreground">
										<Loader2 class="size-3 animate-spin" />
										<span>Loading files…</span>
									</div>
								{:else if sessionState?.error}
									<p class="text-[11px] text-destructive">{sessionState.error}</p>
								{:else if sessionState?.loaded && sessionState.files.length > 0}
									<div class="space-y-1">
										{#each sessionState.files as file (`${session.session_id}:${file.name}:${file.modified}`)}
											<button
												type="button"
												onclick={() => onFileClick(session.session_id, file)}
												class="flex w-full items-center gap-2 rounded border px-2 py-1.5 text-left text-[11px] transition-colors hover:bg-muted/40 {activeFile?.sessionId === session.session_id && activeFile?.file.name === file.name ? 'bg-muted' : ''}"
											>
												<FileText class="size-3.5 shrink-0 text-muted-foreground" />
												<span class="min-w-0 flex-1 truncate">{file.name}</span>
											</button>
										{/each}
									</div>
								{:else}
									<p class="text-[11px] text-muted-foreground">No files in this session.</p>
								{/if}
							</div>
						{/each}
					</div>
				{/if}
			</CardContent>
		</Card>

		<Card>
			<CardHeader class="pb-2">
				<CardTitle class="text-xs uppercase tracking-wide text-muted-foreground">Inspector</CardTitle>
			</CardHeader>
			<CardContent class="space-y-2 text-xs">
				{#if activeStore}
					<div class="rounded border border-border/60 bg-muted/15 p-2">
						<div class="mb-1 flex items-center gap-1.5 font-medium">
							<Database class="size-3.5" />
							<span>{activeStore}</span>
						</div>
						{#if primarySessionId && (activeStore.toLowerCase().includes('memory') || activeStore.toLowerCase() === 'wm')}
							{@const memoryState = memoryBySession[primarySessionId]}
							{#if memoryState?.loading}
								<div class="flex items-center gap-2 text-muted-foreground">
									<Loader2 class="size-3.5 animate-spin" />
									<span>Loading memory preview…</span>
								</div>
							{:else if memoryState?.error}
								<p class="text-destructive">{memoryState.error}</p>
							{:else if memoryState?.loaded}
								{#if memoryState.content.trim()}
									<pre class="max-h-56 overflow-auto whitespace-pre-wrap break-words rounded border bg-background p-2 font-mono text-[10px] leading-relaxed">{memoryState.content}</pre>
								{:else}
									<p class="text-muted-foreground">No working memory content in latest session.</p>
								{/if}
							{:else}
								<p class="text-muted-foreground">Memory preview will load from the latest session.</p>
							{/if}
						{:else}
							<p class="text-muted-foreground">This store has metadata-only inspection in this phase.</p>
						{/if}
					</div>
				{:else if activeFile}
					<div class="rounded border border-border/60 bg-muted/15 p-2">
						<div class="mb-1 font-medium">{activeFile.file.name}</div>
						<div class="grid grid-cols-1 gap-1 text-[11px] sm:grid-cols-2">
							<div>
								<span class="text-muted-foreground">Session:</span>
								<span class="ml-1 font-mono">{activeFile.sessionId}</span>
							</div>
							<div>
								<span class="text-muted-foreground">Size:</span>
								<span class="ml-1">{formatBytes(activeFile.file.size_bytes)}</span>
							</div>
							<div class="sm:col-span-2">
								<span class="text-muted-foreground">Modified:</span>
								<span class="ml-1">{formatTime(activeFile.file.modified)}</span>
							</div>
						</div>
						<p class="mt-2 text-[11px] text-muted-foreground">
							File content preview is not available in this phase.
						</p>
					</div>
				{:else}
					<p class="text-muted-foreground">Select a store type or file link to inspect details.</p>
				{/if}
			</CardContent>
		</Card>

		{#if selectedAgent == null && agents.length > 0}
			<Card>
				<CardHeader class="pb-2">
					<CardTitle class="text-xs uppercase tracking-wide text-muted-foreground">Available Agents</CardTitle>
				</CardHeader>
				<CardContent>
					<div class="flex flex-wrap gap-1.5">
						{#each agents as agent (agent.agent_id)}
							<a
								href={`${uiBase}/status/${encodeURIComponent(agent.agent_id)}/memory`}
								class="rounded border px-2 py-1 text-[11px] transition-colors hover:bg-muted/40"
							>
								{agent.agent_id}
							</a>
						{/each}
					</div>
				</CardContent>
			</Card>
		{/if}
	</div>
</div>
