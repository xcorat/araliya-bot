<script lang="ts">
	import type { SessionInfo } from '$lib/types';
	import * as Sidebar from '$lib/components/ui/sidebar';
	import Button from '$lib/components/ui/button/button.svelte';
	import { Flower2, Plus } from '@lucide/svelte';

	let {
		sessions,
		activeSessionId,
		isLoading,
		onSelectSession,
		onNewSession
	}: {
		sessions: SessionInfo[];
		activeSessionId: string;
		isLoading: boolean;
		onSelectSession: (sessionId: string) => void;
		onNewSession: () => void;
	} = $props();

	function shortId(sessionId: string) {
		return `${sessionId.slice(0, 8)}...`;
	}

	function formatTime(value: string) {
		const date = new Date(value);
		if (Number.isNaN(date.valueOf())) return value;
		const now = new Date();
		const diff = now.getTime() - date.getTime();
		// Less than 24h ago — show time only
		if (diff < 86400000) {
			return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
		}
		// Otherwise show date
		return date.toLocaleDateString([], { month: 'short', day: 'numeric' });
	}
</script>

<Sidebar.Sidebar collapsible="icon" class="border-r">
	<Sidebar.SidebarHeader class="p-3">
		<div class="flex items-center gap-2 group-data-[collapsible=icon]:justify-center">
			<Flower2 class="size-5 shrink-0 text-primary" />
			<span class="text-sm font-semibold group-data-[collapsible=icon]:hidden">Sessions</span>
		</div>
	</Sidebar.SidebarHeader>

	<Sidebar.SidebarContent>
		<Sidebar.SidebarGroup>
			<Sidebar.SidebarGroupContent>
				<Sidebar.SidebarMenu>
					{#if isLoading}
						<Sidebar.SidebarMenuItem>
							<div class="px-2 py-1.5 text-xs text-muted-foreground">Loading...</div>
						</Sidebar.SidebarMenuItem>
					{:else if sessions.length === 0}
						<Sidebar.SidebarMenuItem>
							<div class="px-2 py-1.5 text-xs text-muted-foreground">No sessions yet</div>
						</Sidebar.SidebarMenuItem>
					{:else}
						{#each sessions as session (session.session_id)}
							<Sidebar.SidebarMenuItem>
								<Sidebar.SidebarMenuButton
									isActive={session.session_id === activeSessionId}
									onclick={() => onSelectSession(session.session_id)}
								tooltipContent={session.session_id}
								>
									<div class="flex flex-col gap-0.5 overflow-hidden">
										<span class="truncate font-mono text-xs">{shortId(session.session_id)}</span>
										<span class="truncate text-[10px] text-muted-foreground">
											{formatTime(session.updated_at)}
										</span>
									</div>
								</Sidebar.SidebarMenuButton>
							</Sidebar.SidebarMenuItem>
						{/each}
					{/if}
				</Sidebar.SidebarMenu>
			</Sidebar.SidebarGroupContent>
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
