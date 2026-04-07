import type {
	HealthResponse,
	MessageResponse,
	SessionsResponse,
	AgentsResponse,
	SessionDetailResponse,
	SessionMemoryResponse,
	SessionFilesResponse,
	AgentKGResponse,
	AgentSpendResponse,
	SessionDebugResponse,
	TreeNode,
	ApiError,
	SessionMode,
	ObsEvent
} from './types';

// ── Helpers ─────────────────────────────────────────────────

async function readResponse<T>(response: Response): Promise<T> {
	const contentType = response.headers.get('content-type') || '';
	if (contentType.includes('application/json')) {
		return response.json();
	}
	const text = await response.text();
	throw new Error(text || `HTTP ${response.status}`);
}

async function handleError(response: Response): Promise<never> {
	const body = await readResponse<ApiError>(response).catch(() => ({
		error: 'unknown',
		message: `HTTP ${response.status}`
	}));
	throw new Error(body.message || `HTTP ${response.status}`);
}

// ── API functions ───────────────────────────────────────────

export async function checkHealth(baseUrl: string): Promise<HealthResponse> {
	const response = await fetch(`${baseUrl}/api/health`);
	if (!response.ok) return handleError(response);
	return readResponse<HealthResponse>(response);
}

export async function fetchComponentTree(baseUrl: string): Promise<TreeNode> {
	const response = await fetch(`${baseUrl}/api/tree`);
	if (!response.ok) return handleError(response);
	return readResponse<TreeNode>(response);
}

export async function sendMessage(
	baseUrl: string,
	message: string,
	sessionId?: string,
	mode?: SessionMode,
	agentId?: string
): Promise<MessageResponse> {
	const payload: Record<string, string> = { message };
	if (sessionId) payload.session_id = sessionId;
	if (mode) payload.mode = mode;
	if (agentId) payload.agent_id = agentId;

	const response = await fetch(`${baseUrl}/api/message`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(payload)
	});

	if (!response.ok) return handleError(response);
	return readResponse<MessageResponse>(response);
}

export async function listSessions(baseUrl: string): Promise<SessionsResponse> {
	const response = await fetch(`${baseUrl}/api/sessions`);
	if (!response.ok) return handleError(response);
	return readResponse<SessionsResponse>(response);
}

export async function listAgents(baseUrl: string): Promise<AgentsResponse> {
	const response = await fetch(`${baseUrl}/api/agents`);
	if (!response.ok) return handleError(response);
	return readResponse<AgentsResponse>(response);
}

export async function getAgentSession(
	baseUrl: string,
	agentId: string
): Promise<SessionDetailResponse> {
	const response = await fetch(
		`${baseUrl}/api/agents/${encodeURIComponent(agentId)}/session`
	);
	if (!response.ok) return handleError(response);
	return readResponse<SessionDetailResponse>(response);
}

export async function getSessionById(
	baseUrl: string,
	sessionId: string
): Promise<SessionDetailResponse> {
	const response = await fetch(`${baseUrl}/api/session/${encodeURIComponent(sessionId)}`);
	if (!response.ok) return handleError(response);
	return readResponse<SessionDetailResponse>(response);
}

export async function getSessionMemory(
	baseUrl: string,
	sessionId: string
): Promise<SessionMemoryResponse> {
	const response = await fetch(
		`${baseUrl}/api/sessions/${encodeURIComponent(sessionId)}/memory`
	);
	if (!response.ok) return handleError(response);
	return readResponse<SessionMemoryResponse>(response);
}

export async function getSessionFiles(
	baseUrl: string,
	sessionId: string
): Promise<SessionFilesResponse> {
	const response = await fetch(
		`${baseUrl}/api/sessions/${encodeURIComponent(sessionId)}/files`
	);
	if (!response.ok) return handleError(response);
	return readResponse<SessionFilesResponse>(response);
}

export async function getAgentKG(baseUrl: string, agentId: string): Promise<AgentKGResponse> {
	const response = await fetch(`${baseUrl}/api/agents/${encodeURIComponent(agentId)}/kg`);
	if (!response.ok) return handleError(response);
	return readResponse<AgentKGResponse>(response);
}

export async function getMemoryAgentKG(
	baseUrl: string,
	agentId: string
): Promise<AgentKGResponse> {
	const response = await fetch(
		`${baseUrl}/api/memory/agents/${encodeURIComponent(agentId)}/kg`
	);
	if (!response.ok) return handleError(response);
	return readResponse<AgentKGResponse>(response);
}

export async function getAgentSpend(baseUrl: string, agentId: string): Promise<AgentSpendResponse> {
	const response = await fetch(`${baseUrl}/api/agents/${encodeURIComponent(agentId)}/spend`);
	if (!response.ok) return handleError(response);
	return readResponse<AgentSpendResponse>(response);
}

export async function getSessionDebug(
	baseUrl: string,
	sessionId: string
): Promise<SessionDebugResponse> {
	const response = await fetch(
		`${baseUrl}/api/sessions/${encodeURIComponent(sessionId)}/debug`
	);
	if (!response.ok) return handleError(response);
	return readResponse<SessionDebugResponse>(response);
}

export async function fetchObserveSnapshot(baseUrl: string): Promise<ObsEvent[]> {
	const response = await fetch(`${baseUrl}/api/observe/snapshot`);
	if (!response.ok) return handleError(response);
	return readResponse<ObsEvent[]>(response);
}

export async function clearObserveEvents(baseUrl: string): Promise<{ cleared: number }> {
	const response = await fetch(`${baseUrl}/api/observe/clear`, { method: 'POST' });
	if (!response.ok) return handleError(response);
	return readResponse<{ cleared: number }>(response);
}

export async function listLlmProviders(baseUrl: string): Promise<import('./types').LlmProvidersResponse> {
	const response = await fetch(`${baseUrl}/api/llm/providers`);
	if (!response.ok) return handleError(response);
	return readResponse<import('./types').LlmProvidersResponse>(response);
}

export async function setLlmDefault(baseUrl: string, provider: string): Promise<import('./types').LlmSetDefaultResponse> {
	const response = await fetch(`${baseUrl}/api/llm/default`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ provider })
	});
	if (!response.ok) return handleError(response);
	return readResponse<import('./types').LlmSetDefaultResponse>(response);
}
