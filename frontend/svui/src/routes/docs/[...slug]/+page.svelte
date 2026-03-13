<script lang="ts">
	import { base } from '$app/paths';
	import * as Sidebar from '$lib/components/ui/sidebar';
	import SidebarBridge from '$lib/components/SidebarBridge.svelte';
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

	function jumpToHeading(e: MouseEvent, id: string) {
		e.preventDefault();
		const element = document.getElementById(id);
		if (!element) return;
		element.scrollIntoView({ behavior: 'smooth', block: 'start' });
		history.pushState(null, '', `#${id}`);
	}

	function isActive(routePath: string | null): boolean {
		if (routePath === null) return false;
		return routePath === activeRoutePath;
	}
</script>

<svelte:head>
	<title>Araliya · Docs</title>
</svelte:head>

<Sidebar.SidebarProvider class="h-full min-h-0">
	<SidebarBridge />
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
											class="text-xs"
											style={`padding-left: ${0.5 + item.depth * 0.75}rem`}
										>
											{#snippet child({ props })}
												<a href={toRouteHref(item.routePath)} {...props}>
													{item.title}
												</a>
											{/snippet}
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
							{#each rendered.headings as heading, idx (`${heading.level}:${heading.id}:${idx}`)}
								<Sidebar.SidebarMenuItem>
									<Sidebar.SidebarMenuButton
										class="text-xs"
										style={`padding-left: ${0.5 + (heading.level - 1) * 0.75}rem`}
									>
										{#snippet child({ props })}
											<a href={`#${heading.id}`} onclick={(e) => jumpToHeading(e, heading.id)} {...props}>
												{heading.text}
											</a>
										{/snippet}
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
		<div class="flex h-full min-h-0 flex-1 flex-col overflow-hidden">
			<main class="flex-1 overflow-y-auto">
				<div class="mx-auto w-full max-w-4xl px-6 py-8">
					<div class="mb-3">
						<a href={docsRouteBase} class="text-sm text-muted-foreground underline">← Docs home</a>
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
