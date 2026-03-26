<script lang="ts">
	import { Activity, ChartPie, Database, GitBranch } from '@lucide/svelte';
	import { base } from '$app/paths';
	import * as Sidebar from '$lib/components/ui/sidebar';
	import MainProcessInfoCard from '$lib/components/MainProcessInfoCard.svelte';
	import ComponentTreeNode from '$lib/components/ComponentTreeNode.svelte';
	import type { HealthResponse, TreeNode } from '$lib/types';

	let {
		serviceInfo,
		error = '',
		treeData,
		treeError = '',
		selectedNodeId = '',
		onSelectNode
	}: {
		serviceInfo: HealthResponse | null;
		error?: string;
		treeData: TreeNode | null;
		treeError?: string;
		selectedNodeId?: string;
		onSelectNode?: (node: TreeNode) => void;
	} = $props();
</script>

<Sidebar.Sidebar collapsible="icon" class="border-r">
	<Sidebar.SidebarHeader class="p-3">
		<div class="flex items-center gap-2 group-data-[collapsible=icon]:justify-center">
			<ChartPie class="size-5 shrink-0 text-primary" />
			<span class="text-sm font-semibold group-data-[collapsible=icon]:hidden">Status</span>
		</div>
	</Sidebar.SidebarHeader>

	<Sidebar.SidebarContent>
		<!-- Main process info -->
		<Sidebar.SidebarGroup class="group-data-[collapsible=icon]:hidden">
			<Sidebar.SidebarGroupContent class="px-2 pt-1">
				<MainProcessInfoCard {serviceInfo} {error} />
			</Sidebar.SidebarGroupContent>
		</Sidebar.SidebarGroup>

		<Sidebar.SidebarSeparator class="group-data-[collapsible=icon]:hidden" />

		<!-- Component tree -->
		<Sidebar.SidebarGroup class="group-data-[collapsible=icon]:hidden min-h-0 flex-1">
			<Sidebar.SidebarGroupLabel class="flex items-center gap-1.5">
				<GitBranch class="size-3" />
				Component Tree
			</Sidebar.SidebarGroupLabel>
			<Sidebar.SidebarGroupContent class="overflow-y-auto px-2 pb-2">
				{#if treeError}
					<p class="px-1 text-xs text-muted-foreground">{treeError}</p>
				{:else if treeData}
					<div class="rounded-lg border border-border/50 bg-muted/5 p-1">
						<ComponentTreeNode node={treeData} selectedId={selectedNodeId} {onSelectNode} />
					</div>
				{:else}
					<p class="px-1 text-xs text-muted-foreground">Loading…</p>
				{/if}
			</Sidebar.SidebarGroupContent>
		</Sidebar.SidebarGroup>
	</Sidebar.SidebarContent>

	<Sidebar.SidebarSeparator class="group-data-[collapsible=icon]:hidden" />

	<!-- Quick links -->
	<Sidebar.SidebarGroup class="group-data-[collapsible=icon]:hidden shrink-0">
		<Sidebar.SidebarGroupContent class="px-2 pb-2">
			<Sidebar.SidebarMenu>
				<Sidebar.SidebarMenuItem>
					<Sidebar.SidebarMenuButton class="text-xs gap-1.5">
						{#snippet child({ props })}
							<a href="{base || ''}/status/memory" {...props}>
								<Database class="size-3.5" />
								Memory
							</a>
						{/snippet}
					</Sidebar.SidebarMenuButton>
				</Sidebar.SidebarMenuItem>
				<Sidebar.SidebarMenuItem>
					<Sidebar.SidebarMenuButton class="text-xs gap-1.5">
						{#snippet child({ props })}
							<a href="{base || ''}/status/observe" {...props}>
								<Activity class="size-3.5" />
								Event Log
							</a>
						{/snippet}
					</Sidebar.SidebarMenuButton>
				</Sidebar.SidebarMenuItem>
			</Sidebar.SidebarMenu>
		</Sidebar.SidebarGroupContent>
	</Sidebar.SidebarGroup>

	<Sidebar.SidebarRail />
</Sidebar.Sidebar>
