// Bridge between the global ChatHeader and per-route SidebarProviders.
// Each page registers its sidebar toggle on mount and clears it on destroy.

let _toggle: (() => void) | null = null;

export function registerSidebarToggle(fn: () => void) {
	_toggle = fn;
}

export function unregisterSidebarToggle() {
	_toggle = null;
}

export function fireSidebarToggle() {
	_toggle?.();
}
