import type {
	ChatMessage,
	SessionInfo,
	AgentInfo,
	SessionTranscriptMessage,
	ToolStep,
	UsageInfo
} from './types';
import * as api from './api';

const NO_SESSION_ID = '00000000-0000-0000-0000-000000000000';

function generateId(): string {
	return crypto.randomUUID();
}

function now(): string {
	return new Date().toISOString();
}

// ── Reactive state (Svelte 5 runes) ────────────────────────

let messages = $state<ChatMessage[]>([]);
let sessionId = $state<string>('');
let sessions = $state<SessionInfo[]>([]);
let agents = $state<AgentInfo[]>([]);
let baseUrl = $state<string>('');
let healthStatus = $state<'unknown' | 'checking' | 'ok' | 'error'>('unknown');
let healthMessage = $state<string>('');
let isLoading = $state<boolean>(false);
let isLoadingSessions = $state<boolean>(false);
let lastUsage = $state<UsageInfo | null>(null);
let sessionUsageTotals = $state<UsageInfo | null>(null);
let workingMemoryUpdated = $state<boolean>(false);
let debugExpanded = $state<boolean>(false);

let sessionsRequest: Promise<void> | null = null;
let lastSessionsRefreshAt = 0;
const SESSIONS_REFRESH_THROTTLE_MS = 3000;

// ── Init ────────────────────────────────────────────────────

export function initBaseUrl() {
	if (typeof window !== 'undefined') {
		const envUrl = import.meta.env.VITE_API_BASE_URL;
		baseUrl = envUrl || window.location.origin;
	}
}

// ── Getters ─────────────────────────────────────────────────

export function getMessages(): ChatMessage[] {
	return messages;
}

export function getSessionId(): string {
	return sessionId;
}

export function getSessions(): SessionInfo[] {
	return sessions;
}

export function getAgents(): AgentInfo[] {
	return agents;
}

export function getBaseUrl(): string {
	return baseUrl;
}

export function getHealthStatus(): 'unknown' | 'checking' | 'ok' | 'error' {
	return healthStatus;
}

export function getHealthMessage(): string {
	return healthMessage;
}

export function getIsLoading(): boolean {
	return isLoading;
}

export function getIsLoadingSessions(): boolean {
	return isLoadingSessions;
}

export function getLastUsage(): UsageInfo | null {
	return lastUsage;
}

export function getSessionUsageTotals(): UsageInfo | null {
	return sessionUsageTotals;
}

export function getWorkingMemoryUpdated(): boolean {
	return workingMemoryUpdated;
}

export function getDebugExpanded(): boolean {
	return debugExpanded;
}

// ── Setters ─────────────────────────────────────────────────

export function setBaseUrl(url: string) {
	baseUrl = url.replace(/\/+$/, '');
}

export function setSessionId(id: string) {
	sessionId = id;
}

export function setDebugExpanded(open: boolean) {
	debugExpanded = open;
}

// ── Actions ─────────────────────────────────────────────────

export async function doCheckHealth() {
	if (!baseUrl) return;
	healthStatus = 'checking';
	healthMessage = '';
	try {
		const res = await api.checkHealth(baseUrl);
		healthStatus = 'ok';
		healthMessage = res.status || 'ok';
	} catch (e: unknown) {
		healthStatus = 'error';
		healthMessage = e instanceof Error ? e.message : 'Connection failed';
	}
}

export async function doSendMessage(text: string) {
	if (!text.trim() || !baseUrl || isLoading) return;

	const userMsg: ChatMessage = {
		id: generateId(),
		role: 'user',
		content: text.trim(),
		timestamp: now()
	};
	messages = [...messages, userMsg];
	isLoading = true;

	try {
		const outgoingSessionId = sessionId && sessionId !== NO_SESSION_ID ? sessionId : undefined;
		const res = await api.sendMessage(baseUrl, text.trim(), outgoingSessionId);

		if (res.session_id && res.session_id !== NO_SESSION_ID) {
			sessionId = res.session_id;
		} else {
			sessionId = '';
		}

		const assistantMsg: ChatMessage = {
			id: generateId(),
			role: 'assistant',
			content: res.reply || '(no reply)',
			timestamp: now(),
			intermediateSteps: res.intermediate_steps
		};
		messages = [...messages, assistantMsg];

		lastUsage = res.usage ?? null;
		sessionUsageTotals = res.session_usage_totals ?? null;
		workingMemoryUpdated = res.working_memory_updated ?? false;
		void refreshSessions({ force: true });
	} catch (e: unknown) {
		const errorMsg: ChatMessage = {
			id: generateId(),
			role: 'error',
			content: e instanceof Error ? e.message : 'Failed to send message',
			timestamp: now()
		};
		messages = [...messages, errorMsg];
	} finally {
		isLoading = false;
	}
}

export async function refreshSessions(options: { force?: boolean } = {}) {
	if (!baseUrl) return;

	if (sessionsRequest) {
		return sessionsRequest;
	}

	const nowMs = Date.now();
	if (!options.force && nowMs - lastSessionsRefreshAt < SESSIONS_REFRESH_THROTTLE_MS) {
		return;
	}

	isLoadingSessions = true;
	sessionsRequest = (async () => {
		try {
			const res = await api.listSessions(baseUrl);
			sessions = res.sessions;
			lastSessionsRefreshAt = Date.now();
		} finally {
			isLoadingSessions = false;
			sessionsRequest = null;
		}
	})();

	return sessionsRequest;
}

export async function refreshAgents() {
	if (!baseUrl) return;
	try {
		const res = await api.listAgents(baseUrl);
		agents = res.agents;
	} catch {
		// silently ignore — agents panel degrades gracefully
	}
}

export async function loadSessionHistory(targetSessionId: string) {
	if (!baseUrl || !targetSessionId) return;

	isLoading = true;
	try {
		const res = await api.getSessionById(baseUrl, targetSessionId);

		sessionId = res.session_id;
		messages = mapTranscriptToChatMessages(res.session_id, res.transcript);

		lastUsage = null;
		sessionUsageTotals = null;
		workingMemoryUpdated = false;
	} catch (e: unknown) {
		const errorMsg: ChatMessage = {
			id: generateId(),
			role: 'error',
			content: e instanceof Error ? e.message : 'Failed to load session',
			timestamp: now()
		};
		messages = [...messages, errorMsg];
	} finally {
		isLoading = false;
	}
}

export function resetSession() {
	messages = [];
	sessionId = '';
	lastUsage = null;
	sessionUsageTotals = null;
	workingMemoryUpdated = false;
	healthStatus = 'unknown';
	healthMessage = '';
}

// ── Helpers ─────────────────────────────────────────────────

function mapTranscriptToChatMessages(
	currentSessionId: string,
	transcript: SessionTranscriptMessage[]
): ChatMessage[] {
	const restoredMessages: ChatMessage[] = [];
	let pendingSteps: ToolStep[] = [];

	for (const entry of transcript) {
		if (entry.role === 'assistant' && entry.tool_calls && entry.tool_calls.length > 0) {
			for (const call of entry.tool_calls) {
				pendingSteps.push({
					tool_call_id: call.id,
					tool_name: call.function.name,
					arguments: parseToolArguments(call.function.arguments),
					result: ''
				});
			}
			continue;
		}

		if (entry.role === 'tool' && entry.tool_call_id) {
			const step = pendingSteps.find((c) => c.tool_call_id === entry.tool_call_id);
			if (step) {
				step.result = entry.content;
			}
			continue;
		}

		if ((entry.role === 'user' || entry.role === 'assistant') && entry.content.trim()) {
			restoredMessages.push({
				id: `${currentSessionId}-${restoredMessages.length}-${entry.timestamp}`,
				role: entry.role as 'user' | 'assistant',
				content: entry.content,
				timestamp: entry.timestamp,
				intermediateSteps:
					entry.role === 'assistant' && pendingSteps.length > 0
						? pendingSteps
						: undefined
			});

			if (entry.role === 'assistant') {
				pendingSteps = [];
			}
		}
	}

	return restoredMessages;
}

function parseToolArguments(raw: string): Record<string, unknown> {
	try {
		const parsed = JSON.parse(raw);
		if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
			return parsed as Record<string, unknown>;
		}
	} catch {
		// fall through
	}
	return {};
}
