<script lang="ts">
	import { cn, type WithElementRef } from "$lib/utils.js";
	import type { HTMLAttributes } from "svelte/elements";
	import { useSidebar } from "./context.svelte.js";

	let {
		ref = $bindable(null),
		side = "left",
		class: className,
		children,
		...restProps
	}: WithElementRef<HTMLAttributes<HTMLButtonElement>, HTMLButtonElement> & {
		side?: "left" | "right";
	} = $props();

	const sidebar = useSidebar();

	let dragging = false;
	let startX = 0;
	let startWidth = 0;
	let containerEl: HTMLElement | null = null;

	function onPointerDown(e: PointerEvent) {
		// Only handle primary button
		if (e.button !== 0) return;
		e.preventDefault();
		dragging = false;
		startX = e.clientX;
		startWidth = sidebar.widthPx;
		// Capture pointer so fast mouse moves still fire even outside the element
		(e.currentTarget as Element).setPointerCapture(e.pointerId);
		// Suppress the sidebar width transition for instant feedback during drag
		containerEl = ref?.closest<HTMLElement>('[data-slot="sidebar-container"]') ?? null;
		if (containerEl) containerEl.style.transitionDuration = "0s";
		window.addEventListener("pointermove", onPointerMove);
		window.addEventListener("pointerup", onPointerUp, { once: true });
	}

	function onPointerMove(e: PointerEvent) {
		const dx = e.clientX - startX;
		if (!dragging && Math.abs(dx) > 4) dragging = true;
		if (dragging) {
			// For a left sidebar the rail is on the right edge: drag right = wider
			// For a right sidebar the rail is on the left edge: drag left = wider
			const newWidth = side === "right" ? startWidth - dx : startWidth + dx;
			sidebar.setWidthPx(newWidth);
		}
	}

	function onPointerUp() {
		window.removeEventListener("pointermove", onPointerMove);
		// Restore the transition now that drag is done
		if (containerEl) {
			containerEl.style.transitionDuration = "";
			containerEl = null;
		}
		if (!dragging) {
			// Plain click — toggle open/collapsed
			sidebar.toggle();
		}
		dragging = false;
	}
</script>

<button
	bind:this={ref}
	data-sidebar="rail"
	data-slot="sidebar-rail"
	aria-label="Toggle Sidebar"
	tabIndex={-1}
	onpointerdown={onPointerDown}
	title="Toggle Sidebar"
	class={cn(
		"hover:after:bg-sidebar-border absolute inset-y-0 z-20 hidden w-4 -translate-x-1/2 transition-[opacity] ease-linear group-data-[side=left]:-end-4 group-data-[side=right]:start-0 after:absolute after:inset-y-0 after:start-[calc(1/2*100%-1px)] after:w-[2px] sm:flex",
		"in-data-[side=left]:cursor-col-resize in-data-[side=right]:cursor-col-resize",
		"[[data-side=left][data-state=collapsed]_&]:cursor-e-resize [[data-side=right][data-state=collapsed]_&]:cursor-w-resize",
		"hover:group-data-[collapsible=offcanvas]:bg-sidebar group-data-[collapsible=offcanvas]:translate-x-0 group-data-[collapsible=offcanvas]:after:start-full",
		"[[data-side=left][data-collapsible=offcanvas]_&]:-end-2",
		"[[data-side=right][data-collapsible=offcanvas]_&]:-start-2",
		className
	)}
	{...restProps}
>
	{@render children?.()}
</button>
