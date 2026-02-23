<script lang="ts">
	import type { SessionInfo, AgentInfo } from '$lib/types';
	import * as Sidebar from '$lib/components/ui/sidebar';
	import Button from '$lib/components/ui/button/button.svelte';
	import { Flower2, Plus, ChevronDown, ChevronRight, Bot } from '@lucide/svelte';

	let {
		sessions,
		agents,
		activeSessionId,
		isLoading,
		onSelectSession,
		onNewSession
	}: {
		sessions: SessionInfo[];
		agents: AgentInfo[];
		activeSessionId: string;
		isLoading: boolean;
		onSelectSession: (sessionId: string) => void;
		onNewSession: () => void;
	} = $props();

	let sessionsOpen = $state(true);
	let agentsOpen = $state(true);

	function shortId(sessionId: string) {
		return `${sessionId.slice(0, 8)}…`;
	}

	function formatTime(value: string | null | undefined) {
		if (!value) return '—';
		const date = new Date(value);
		if (Number.isNaN(date.valueOf())) return value;
		const now = new Date();
		const diff = now.getTime() - date.getTime();
		if (diff < 86400000) {
			return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
		}
		return date.toLocaleDateString([], { month: 'short', day: 'numeric' });
	}

	const sortedSessions = $derived(
		[...sessions].sort((a, b) => {
			const ta = new Date(a.updated_at ?? a.created_at).getTime();
			const tb = new Date(b.updated_at ?? b.created_at).getTime();
			return tb - ta;
		})
	);

	const sortedAgents = $derived(
		[...agents].sort((a, b) => {
			if (!a.last_fetched && !b.last_fetched) return 0;
			if (!a.last_fetched) return 1;
			if (!b.last_fetched) return -1;
			return new Date(b.last_fetched).getTime() - new Date(a.last_fetched).getTime();
		})
	);
</script>

<Sidebar.Sidebar collapsible="icon" class="border-r">
	<Sidebar.SidebarHeader class="p-3">
		<div class="flex items-center gap-2 group-data-[collapsible=icon]:justify-center">
			<Flower2 class="size-5 shrink-0 text-primary" />
			<span class="text-sm font-semibold group-data-[collapsible=icon]:hidden">Araliya</span>
		</div>
	</Sidebar.SidebarHeader>

	<Sidebar.SidebarContent>
		<!-- Sessions group -->
		<Sidebar.SidebarGroup>
			<button
				class="flex w-full items-center justify-between px-2 py-1.5 text-xs font-medium text-muted-foreground hover:text-foreground group-data-[collapsible=icon]:hidden"
				onclick={() => (sessionsOpen = !sessionsOpen)}
			>
				<span>Sessions</span>
				{#if sessionsOpen}
					<ChevronDown class="size-3" />
				{:else}
					<ChevronRight class="size-3" />
				{/if}
			</button>
			{#if sessionsOpen}
				<Sidebar.SidebarGroupContent>
					<Sidebar.SidebarMenu>
						{#if isLoading}
							<Sidebar.SidebarMenuItem>
								<div class="px-2 py-1.5 text-xs text-muted-foreground">Loading…</div>
							</Sidebar.SidebarMenuItem>
						{:else if sortedSessions.length === 0}
							<Sidebar.SidebarMenuItem>
								<div class="px-2 py-1.5 text-xs text-muted-foreground">No sessions yet</div>
							</Sidebar.SidebarMenuItem>
						{:else}
							{#each sortedSessions as session (session.session_id)}
								<Sidebar.SidebarMenuItem>
									<Sidebar.SidebarMenuButton
										isActive={session.session_id === activeSessionId}
										onclick={() => onSelectSession(session.session_id)}
										tooltipContent={session.session_id}
									>
										<div class="flex flex-col gap-0.5 overflow-hidden">
											<span class="truncate font-mono text-xs">{shortId(session.session_id)}</span>
											<span class="truncate text-[10px] text-muted-foreground">
												{formatTime(session.updated_at ?? session.created_at)}
											</span>
										</div>
									</Sidebar.SidebarMenuButton>
								</Sidebar.SidebarMenuItem>
							{/each}
						{/if}
					</Sidebar.SidebarMenu>
				</Sidebar.SidebarGroupContent>
			{/if}
		</Sidebar.SidebarGroup>

		<!-- Agents group -->
		<Sidebar.SidebarGroup>
			<button
				class="flex w-full items-center justify-between px-2 py-1.5 text-xs font-medium text-muted-foreground hover:text-foreground group-data-[collapsible=icon]:hidden"
				onclick={() => (agentsOpen = !agentsOpen)}
			>
				<span>Agents</span>
				{#if agentsOpen}
					<ChevronDown class="size-3" />
				{:else}
					<ChevronRight class="size-3" />
				{/if}
			</button>
			{#if agentsOpen}
				<Sidebar.SidebarGroupContent>
					<Sidebar.SidebarMenu>
						{#if sortedAgents.length === 0}
							<Sidebar.SidebarMenuItem>
								<div class="px-2 py-1.5 text-xs text-muted-foreground">No agents registered</div>
							</Sidebar.SidebarMenuItem>
						{:else}
							{#each sortedAgents as agent (agent.agent_id)}
								<Sidebar.SidebarMenuItem>
									<Sidebar.SidebarMenuButton
										isActive={false}
										tooltipContent={`${agent.name} · ${agent.session_count} session${agent.session_count === 1 ? '' : 's'}`}
									>
										<div class="flex items-center gap-1.5 overflow-hidden">
											<Bot class="size-3.5 shrink-0 text-muted-foreground" />
											<div class="flex flex-col gap-0.5 overflow-hidden group-data-[collapsible=icon]:hidden">
												<span class="truncate text-xs capitalize">{agent.name}</span>
												<span class="truncate text-[10px] text-muted-foreground">
													{formatTime(agent.last_fetched)}
												</span>
											</div>
										</div>
									</Sidebar.SidebarMenuButton>
								</Sidebar.SidebarMenuItem>
							{/each}
						{/if}
					</Sidebar.SidebarMenu>
				</Sidebar.SidebarGroupContent>
			{/if}
		</Sidebar.SidebarGroup>
	</Sidebar.SidebarContent>

	<Sidebar.SidebarFooter class="p-2">
		<Button
			variant="outline"
			size="sm"
			onclick={onNewSession}
			class="w-full gap-1.5 group-data-[collapsible=icon]:size-8 group-data-[collapsible=icon]:p-0"
		>
			<Plus class="size-4 shrink-0" />
			<span class="group-data-[collapsible=icon]:hidden">New Session</span>
		</Button>
	</Sidebar.SidebarFooter>

	<Sidebar.SidebarRail />
</Sidebar.Sidebar>
