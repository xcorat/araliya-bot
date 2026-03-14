import type {
	ChatMessage,
	SessionInfo,
	AgentInfo,
	SessionTranscriptMessage,
	ToolStep,
	UsageInfo,
	LlmTiming
} from './types';
import * as api from './api';

const NO_SESSION_ID = '00000000-0000-0000-0000-000000000000';

function generateId(): string {
	// crypto.randomUUID() requires a secure context (HTTPS/localhost).
	// crypto.getRandomValues() works on HTTP too and is universally supported.
	if (typeof crypto !== 'undefined' && crypto.randomUUID) {
		return crypto.randomUUID();
	}
	const bytes = new Uint8Array(16);
	crypto.getRandomValues(bytes);
	bytes[6] = (bytes[6] & 0x0f) | 0x40; // version 4
	bytes[8] = (bytes[8] & 0x3f) | 0x80; // variant bits
	const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
	return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
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
/** True while a session transcript is being fetched (history load / poll refresh).
 *  Separate from `isLoading` so history fetches don't trigger the Thinking… animation. */
let isHistoryLoading = $state<boolean>(false);
/** When set, `refreshSharedMessages` re-fetches via the agent session endpoint
 *  instead of the global session-by-id endpoint. Set by `initSharedSession`. */
let sharedAgentId = $state<string | null>(null);
let lastUsage = $state<UsageInfo | null>(null);
let sessionUsageTotals = $state<UsageInfo | null>(null);
let lastTiming = $state<LlmTiming | null>(null);
/** Live elapsed ms while a streaming request is in-flight; null otherwise. */
let streamElapsedMs = $state<number | null>(null);
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

export function getIsHistoryLoading(): boolean {
	return isHistoryLoading;
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

export function getLastTiming(): LlmTiming | null {
	return lastTiming;
}

export function getStreamElapsedMs(): number | null {
	return streamElapsedMs;
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

export async function doSendMessage(text: string, agentId?: string) {
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
		const res = await api.sendMessage(baseUrl, text.trim(), outgoingSessionId, undefined, agentId);

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
			intermediateSteps: res.intermediate_steps,
			thinking: res.thinking ?? undefined,
			usage: res.usage ?? undefined,
			timing: res.timing ?? undefined
		};
		messages = [...messages, assistantMsg];

		lastUsage = res.usage ?? null;
		lastTiming = res.timing ?? null;
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

/// Stream a message via POST /api/message/stream (SSE).
///
/// Appends an assistant message immediately (empty content) then fills it
/// in token-by-token as `content` and `thinking` chunks arrive.
/// Falls back to the buffered path on any fetch error.
export async function doSendMessageStreaming(text: string, agentId?: string) {
	if (!text.trim() || !baseUrl || isLoading) return;

	const userMsg: ChatMessage = {
		id: generateId(),
		role: 'user',
		content: text.trim(),
		timestamp: now()
	};
	messages = [...messages, userMsg];
	isLoading = true;

	// Pre-create the assistant message with an empty placeholder so the UI
	// can show the streaming cursor immediately.
	const assistantId = generateId();
	const assistantMsg: ChatMessage = {
		id: assistantId,
		role: 'assistant',
		content: '',
		timestamp: now()
	};
	messages = [...messages, assistantMsg];

	const updateAssistant = (patch: Partial<ChatMessage>) => {
		messages = messages.map((m) =>
			m.id === assistantId ? { ...m, ...patch } : m
		);
	};

	// Start a live elapsed-time counter so the status bar shows ⏱ Xs while streaming.
	const sendTime = Date.now();
	streamElapsedMs = 0;
	let elapsedTimer: ReturnType<typeof setInterval> | null = setInterval(() => {
		streamElapsedMs = Date.now() - sendTime;
	}, 100);

	const stopTimer = () => {
		if (elapsedTimer !== null) {
			clearInterval(elapsedTimer);
			elapsedTimer = null;
			streamElapsedMs = null;
		}
	};

	try {
		const outgoingSessionId =
			sessionId && sessionId !== NO_SESSION_ID ? sessionId : undefined;
		const streamPayload: Record<string, string | undefined> = {
			message: text.trim(),
			session_id: outgoingSessionId
		};
		if (agentId) streamPayload.agent_id = agentId;

		const res = await fetch(`${baseUrl}/api/message/stream`, {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify(streamPayload)
		});

		if (!res.ok || !res.body) {
			throw new Error(`HTTP ${res.status}`);
		}

		const reader = res.body.getReader();
		const decoder = new TextDecoder();
		let buf = '';
		let thinkingBuf = '';
		let contentBuf = '';

		while (true) {
			const { done, value } = await reader.read();
			if (done) break;
			buf += decoder.decode(value, { stream: true });

			// Process complete SSE lines from the buffer.
			let newline: number;
			while ((newline = buf.indexOf('\n')) !== -1) {
				const rawLine = buf.slice(0, newline).trim();
				buf = buf.slice(newline + 1);

				if (!rawLine || rawLine === 'data: [DONE]') continue;

				let eventName = 'message';
				let dataStr: string | null = null;

				if (rawLine.startsWith('event: ')) {
					eventName = rawLine.slice(7).trim();
				} else if (rawLine.startsWith('data: ')) {
					dataStr = rawLine.slice(6);
				}

				if (!dataStr) {
					// Peek at next line for the data field.
					const nextNewline = buf.indexOf('\n');
					if (nextNewline !== -1) {
						const nextLine = buf.slice(0, nextNewline).trim();
						if (nextLine.startsWith('data: ')) {
							dataStr = nextLine.slice(6);
							buf = buf.slice(nextNewline + 1);
						}
					}
				}

				if (!dataStr) continue;

				try {
					const payload = JSON.parse(dataStr) as Record<string, unknown>;
					if (eventName === 'thinking' && typeof payload.delta === 'string') {
						thinkingBuf += payload.delta;
						updateAssistant({ thinking: thinkingBuf });
					} else if (eventName === 'content' && typeof payload.delta === 'string') {
						contentBuf += payload.delta;
						updateAssistant({ content: contentBuf || '…' });
					} else if (eventName === 'done') {
						stopTimer();
						if (!contentBuf) updateAssistant({ content: '(no reply)' });
						// Parse and attach usage + timing from the done event.
						const doneUsage = payload.usage as UsageInfo | undefined;
						const doneTiming = payload.timing as LlmTiming | undefined;
						if (doneUsage) {
							lastUsage = doneUsage;
						}
						if (doneTiming) {
							lastTiming = doneTiming;
						}
						updateAssistant({ usage: doneUsage, timing: doneTiming });
					}
				} catch {
					// Ignore malformed JSON chunks.
				}
			}
		}

		stopTimer();
		void refreshSessions({ force: true });
	} catch (e: unknown) {
		stopTimer();
		updateAssistant({
			role: 'error',
			content: e instanceof Error ? e.message : 'Stream failed'
		});
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

	isHistoryLoading = true;
	try {
		const res = await api.getSessionById(baseUrl, targetSessionId);

		sessionId = res.session_id;
		messages = mapTranscriptToChatMessages(res.session_id, res.transcript);

		lastUsage = null;
		lastTiming = null;
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
		isHistoryLoading = false;
	}
}

export function resetSession() {
	messages = [];
	sessionId = '';
	lastUsage = null;
	lastTiming = null;
	sessionUsageTotals = null;
	streamElapsedMs = null;
	workingMemoryUpdated = false;
	healthStatus = 'unknown';
	healthMessage = '';
}

/**
 * Bootstrap the shared session for a given agent.
 *
 * Fetches the sessions list, finds the most recent session whose
 * `last_agent` matches `agentId`, and loads its full transcript so
 * all visitors immediately see the shared conversation.
 *
 * If no matching session exists yet (first-ever run) the function
 * is a no-op — the chat starts empty and a session will be created
 * on the first message.
 */
export async function initSharedSession(agentId: string) {
	if (!baseUrl) return;
	sharedAgentId = agentId;
	try {
		const res = await api.getAgentSession(baseUrl, agentId);
		if (res.session_id) {
			sessionId = res.session_id;
			messages = mapTranscriptToChatMessages(res.session_id, res.transcript);
		}
	} catch {
		// No session yet — start with an empty chat, that's fine.
	}
}

/**
 * Re-fetch the current shared session transcript from the server.
 *
 * Used by the Refresh button and the background polling loop to pull
 * in messages posted by other visitors while the current tab is open.
 * Safe to call while `isLoading` is true — skips silently so it never
 * interrupts an in-flight LLM request.
 */
export async function refreshSharedMessages() {
	if (!baseUrl || isLoading || isHistoryLoading) return;
	if (sharedAgentId) {
		// Agent-scoped session: re-fetch directly from the agent endpoint.
		isHistoryLoading = true;
		try {
			const res = await api.getAgentSession(baseUrl, sharedAgentId);
			if (res.session_id) {
				sessionId = res.session_id;
				messages = mapTranscriptToChatMessages(res.session_id, res.transcript);
			}
		} catch {
			// Silently ignore — polling failures should not disrupt the UI.
		} finally {
			isHistoryLoading = false;
		}
		return;
	}
	// Global session fallback (non-agent pages).
	if (!sessionId || sessionId === NO_SESSION_ID) return;
	await loadSessionHistory(sessionId);
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
