import type { HealthResponse, MainProcessStatus, SubsystemStatus } from '$lib/types';

export function mainProcess(info: HealthResponse): MainProcessStatus {
	return (
		info.main_process ?? {
			id: 'supervisor',
			name: 'Supervisor',
			status: info.status ?? 'unknown',
			uptime_ms: info.uptime_ms ?? 0,
			details: {
				bot_id: info.bot_id,
				llm_provider: info.llm_provider,
				llm_model: info.llm_model,
				llm_timeout_seconds: info.llm_timeout_seconds
			}
		}
	);
}

export function subsystems(info: HealthResponse): SubsystemStatus[] {
	return info.subsystems ?? [];
}

export function formatUptime(ms: number | undefined): string {
	if (!ms || ms < 0) return '—';
	const totalSeconds = Math.floor(ms / 1000);
	const hours = Math.floor(totalSeconds / 3600);
	const minutes = Math.floor((totalSeconds % 3600) / 60);
	const seconds = totalSeconds % 60;
	return `${hours}h ${minutes}m ${seconds}s`;
}

export function statusDotClass(status: string): string {
	const normalized = status.toLowerCase();
	if (normalized === 'ok' || normalized === 'running') return 'bg-emerald-500';
	if (normalized === 'degraded' || normalized === 'warning') return 'bg-yellow-500';
	if (normalized === 'error' || normalized === 'failed') return 'bg-destructive';
	return 'bg-muted-foreground';
}

export function statusPillClass(status: string): string {
	const normalized = status.toLowerCase();
	if (normalized === 'ok' || normalized === 'running') {
		return 'border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300';
	}
	if (normalized === 'degraded' || normalized === 'warning') {
		return 'border-yellow-500/30 bg-yellow-500/10 text-yellow-700 dark:text-yellow-300';
	}
	if (normalized === 'error' || normalized === 'failed') {
		return 'border-destructive/30 bg-destructive/10 text-destructive';
	}
	return 'border-muted-foreground/25 bg-muted/40 text-muted-foreground';
}

export function subsystemKind(id: string): string {
	const normalized = id.toLowerCase();
	if (normalized.includes('agent')) return 'agents';
	if (normalized.includes('comm')) return 'comms';
	if (normalized.includes('memory')) return 'memory';
	if (normalized.includes('llm')) return 'llm';
	if (normalized.includes('manage')) return 'management';
	if (normalized.includes('ui')) return 'ui';
	return 'default';
}

export function subsystemCardClass(id: string): string {
	switch (subsystemKind(id)) {
		case 'agents':
			return 'border-violet-500/35 bg-violet-500/[0.04]';
		case 'comms':
			return 'border-sky-500/35 bg-sky-500/[0.04]';
		case 'memory':
			return 'border-amber-500/35 bg-amber-500/[0.04]';
		case 'llm':
			return 'border-emerald-500/35 bg-emerald-500/[0.04]';
		case 'management':
			return 'border-primary/35 bg-primary/[0.04]';
		case 'ui':
			return 'border-fuchsia-500/35 bg-fuchsia-500/[0.04]';
		default:
			return 'border-border/70 bg-card/80';
	}
}

export function subsystemHeaderClass(id: string): string {
	switch (subsystemKind(id)) {
		case 'agents':
			return 'bg-violet-500/[0.08] hover:bg-violet-500/[0.14]';
		case 'comms':
			return 'bg-sky-500/[0.08] hover:bg-sky-500/[0.14]';
		case 'memory':
			return 'bg-amber-500/[0.08] hover:bg-amber-500/[0.14]';
		case 'llm':
			return 'bg-emerald-500/[0.08] hover:bg-emerald-500/[0.14]';
		case 'management':
			return 'bg-primary/[0.08] hover:bg-primary/[0.14]';
		case 'ui':
			return 'bg-fuchsia-500/[0.08] hover:bg-fuchsia-500/[0.14]';
		default:
			return 'bg-muted/20 hover:bg-muted/35';
	}
}

export function formatDetailLabel(key: string): string {
	return key
		.replace(/_/g, ' ')
		.replace(/([a-z0-9])([A-Z])/g, '$1 $2')
		.replace(/\s+/g, ' ')
		.trim();
}

export function formatDetailValue(value: unknown): string {
	if (value === null || value === undefined) return '—';
	if (typeof value === 'string') return value;
	if (typeof value === 'number' || typeof value === 'boolean') return String(value);
	try {
		return JSON.stringify(value);
	} catch {
		return '—';
	}
}

export function detailEntries(details?: Record<string, unknown>): [string, unknown][] {
	if (!details) return [];
	return Object.entries(details);
}

export function pickDetail(details: Record<string, unknown> | undefined, keys: string[]): unknown {
	if (!details) return undefined;
	for (const key of keys) {
		if (Object.hasOwn(details, key)) return details[key];
	}
	return undefined;
}

export function detailList(details: Record<string, unknown> | undefined, keys: string[]): string[] {
	const value = pickDetail(details, keys);
	if (Array.isArray(value)) {
		return value.map((entry) => formatDetailValue(entry)).filter((entry) => entry !== '—');
	}
	if (typeof value === 'string') {
		return value
			.split(',')
			.map((part) => part.trim())
			.filter(Boolean);
	}
	return [];
}

export function detailFlag(details: Record<string, unknown> | undefined, keys: string[]): string {
	const value = pickDetail(details, keys);
	if (typeof value === 'boolean') return value ? 'Enabled' : 'Disabled';
	if (typeof value === 'number') return value > 0 ? 'Enabled' : 'Disabled';
	if (typeof value === 'string') {
		const normalized = value.toLowerCase();
		if (['true', 'enabled', 'on', 'yes'].includes(normalized)) return 'Enabled';
		if (['false', 'disabled', 'off', 'no'].includes(normalized)) return 'Disabled';
	}
	return '—';
}

export function detailCount(details: Record<string, unknown> | undefined, keys: string[]): string {
	const value = pickDetail(details, keys);
	if (typeof value === 'number') return String(value);
	if (Array.isArray(value)) return String(value.length);
	if (typeof value === 'string' && value.trim()) return value;
	return '—';
}
