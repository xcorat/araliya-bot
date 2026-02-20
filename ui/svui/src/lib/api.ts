import type {
	HealthResponse,
	MessageResponse,
	SessionsResponse,
	SessionDetailResponse,
	SessionMemoryResponse,
	SessionFilesResponse,
	ApiError,
	SessionMode
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

export async function sendMessage(
	baseUrl: string,
	message: string,
	sessionId?: string,
	mode?: SessionMode
): Promise<MessageResponse> {
	const payload: Record<string, string> = { message };
	if (sessionId) payload.session_id = sessionId;
	if (mode) payload.mode = mode;

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
