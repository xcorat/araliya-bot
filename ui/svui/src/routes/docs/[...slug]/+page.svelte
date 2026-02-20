<script lang="ts">
	import { base } from '$app/paths';
	import { goto } from '$app/navigation';
	import { ChatHeader } from '$lib/components/chat';
	import * as Sidebar from '$lib/components/ui/sidebar';
	import { parseMarkdownDocument } from '$lib/utils/markdown';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	const docsRouteBase = `${base}/docs`;

	const rendered = $derived(
		parseMarkdownDocument(data.markdown, {
			docPath: data.docPath,
			docsRouteBase
		})
	);

	const activeRoutePath = $derived(data.resolvedRoutePath.replace(/^\/+|\/+$/g, ''));

	function toRouteHref(routePath: string | null): string {
		if (routePath === null) {
			return '#';
		}
		return routePath ? `${docsRouteBase}/${routePath}` : docsRouteBase;
	}

	function navigateToDoc(routePath: string | null) {
		if (routePath === null) {
			return;
		}
		void goto(toRouteHref(routePath));
	}

	function jumpToHeading(id: string) {
		const element = document.getElementById(id);
		if (!element) {
			return;
		}
		element.scrollIntoView({ behavior: 'smooth', block: 'start' });
		window.location.hash = id;
	}

	function isActive(routePath: string | null): boolean {
		if (routePath === null) {
			return false;
		}
		return routePath === activeRoutePath;
	}
</script>

<svelte:head>
	<title>Araliya · Docs</title>
</svelte:head>

<Sidebar.SidebarProvider>
	<Sidebar.Sidebar class="border-r">
		<Sidebar.SidebarHeader class="p-3">
			<div class="flex items-center gap-2">
				<div class="size-2 rounded-full bg-primary" aria-hidden="true"></div>
				<span class="text-sm font-semibold">Documentation</span>
			</div>
		</Sidebar.SidebarHeader>

		<Sidebar.SidebarContent>
			<Sidebar.SidebarGroup>
				<Sidebar.SidebarGroupLabel>Contents</Sidebar.SidebarGroupLabel>
				<Sidebar.SidebarGroupContent>
					<Sidebar.SidebarMenu>
						{#if data.manifest.items.length === 0}
							<Sidebar.SidebarMenuItem>
								<div class="px-2 py-1.5 text-xs text-muted-foreground">No docs index available</div>
							</Sidebar.SidebarMenuItem>
						{:else}
							{#each data.manifest.items as item, idx (`${item.kind}:${item.routePath ?? item.title}:${idx}`)}
								<Sidebar.SidebarMenuItem>
									{#if item.kind === 'directory'}
										<div
											class="px-2 py-1.5 text-xs font-medium text-muted-foreground"
											style={`padding-left: ${0.5 + item.depth * 0.75}rem`}
										>
											{item.title}
										</div>
									{:else}
										<Sidebar.SidebarMenuButton
											isActive={isActive(item.routePath)}
											onclick={() => navigateToDoc(item.routePath)}
											class="text-xs"
											style={`padding-left: ${0.5 + item.depth * 0.75}rem`}
										>
											{item.title}
										</Sidebar.SidebarMenuButton>
									{/if}
								</Sidebar.SidebarMenuItem>
							{/each}
						{/if}
					</Sidebar.SidebarMenu>
				</Sidebar.SidebarGroupContent>
			</Sidebar.SidebarGroup>

			<Sidebar.SidebarSeparator />

			<Sidebar.SidebarGroup>
				<Sidebar.SidebarGroupLabel>On this page</Sidebar.SidebarGroupLabel>
				<Sidebar.SidebarGroupContent>
					<Sidebar.SidebarMenu>
						{#if rendered.headings.length === 0}
							<Sidebar.SidebarMenuItem>
								<div class="px-2 py-1.5 text-xs text-muted-foreground">No headings</div>
							</Sidebar.SidebarMenuItem>
						{:else}
							{#each rendered.headings as heading (`${heading.level}:${heading.id}`)}
								<Sidebar.SidebarMenuItem>
									<Sidebar.SidebarMenuButton
										onclick={() => jumpToHeading(heading.id)}
										class="text-xs"
										style={`padding-left: ${0.5 + (heading.level - 1) * 0.75}rem`}
									>
										{heading.text}
									</Sidebar.SidebarMenuButton>
								</Sidebar.SidebarMenuItem>
							{/each}
						{/if}
					</Sidebar.SidebarMenu>
				</Sidebar.SidebarGroupContent>
			</Sidebar.SidebarGroup>
		</Sidebar.SidebarContent>

		<Sidebar.SidebarRail />
	</Sidebar.Sidebar>

	<Sidebar.SidebarInset>
		<div class="flex h-dvh flex-col">
			<ChatHeader />

			<main class="flex-1 overflow-y-auto">
				<div class="mx-auto w-full max-w-4xl px-6 py-8">
					<div class="mb-3">
						<a href={base || '/'} class="text-sm text-muted-foreground underline">Back to app</a>
					</div>
					{#if data.notFound}
						<p class="text-sm text-muted-foreground">Requested path: /docs/{data.requestPath}</p>
					{/if}
					<article class="prose prose-sm md:prose-base mt-3" aria-label="Documentation content">
						{@html rendered.html}
					</article>
				</div>
			</main>
		</div>
	</Sidebar.SidebarInset>
</Sidebar.SidebarProvider>
