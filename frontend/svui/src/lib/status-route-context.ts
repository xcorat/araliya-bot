import type { Writable } from 'svelte/store';
import type { HealthResponse, TreeNode } from '$lib/types';

export const STATUS_ROUTE_CONTEXT = Symbol('status-route-context');

export interface StatusRouteContext {
	serviceInfo: Writable<HealthResponse | null>;
	treeData: Writable<TreeNode | null>;
	error: Writable<string>;
	treeError: Writable<string>;
	loading: Writable<boolean>;
	lastRefresh: Writable<string>;
	isPolling: Writable<boolean>;
	fetchAll: () => Promise<void>;
	togglePolling: () => void;
	resolveNodeById: (id: string) => TreeNode | null;
}

export function findTreeNodeById(root: TreeNode | null, targetId: string): TreeNode | null {
	if (!root || !targetId) {
		return null;
	}
	if (root.id === targetId) {
		return root;
	}
	for (const child of root.children) {
		const found = findTreeNodeById(child, targetId);
		if (found) {
			return found;
		}
	}
	return null;
}
