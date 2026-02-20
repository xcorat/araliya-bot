// ── Chat types ──────────────────────────────────────────────

export interface ChatMessage {
	id: string;
	role: 'user' | 'assistant' | 'system' | 'error';
	content: string;
	timestamp: string;
	intermediateSteps?: ToolStep[];
}

export interface ToolStep {
	tool_call_id: string;
	tool_name: string;
	arguments: Record<string, unknown>;
	result: string;
}

export type SessionMode = 'chat' | 'agent';

export interface UsageInfo {
	prompt_tokens: number;
	completion_tokens: number;
	total_tokens: number;
	estimated_cost_usd: number;
}

// ── API response types ──────────────────────────────────────

export interface MessageResponse {
	session_id: string;
	mode: SessionMode;
	run_id?: string;
	reply: string;
	working_memory_updated: boolean;
	intermediate_steps?: ToolStep[];
	usage?: UsageInfo;
	session_usage_totals?: UsageInfo;
}

export interface HealthResponse {
	status: string;
	bot_id: string;
	llm_provider: string;
	llm_model: string;
	llm_timeout_seconds: number;
	enabled_tools: string[];
	max_tool_rounds: number;
	session_count: number;
}

export interface SessionInfo {
	session_id: string;
	created_at: string;
	updated_at: string;
	mode: SessionMode;
}

export interface SessionsResponse {
	sessions: SessionInfo[];
}

export interface SessionTranscriptMessage {
	role: string;
	content: string;
	timestamp: string;
	tool_call_id?: string;
	tool_calls?: SessionToolCall[];
}

export interface SessionToolCall {
	id: string;
	type?: string;
	function: {
		name: string;
		arguments: string;
	};
}

export interface SessionDetailResponse {
	session: SessionInfo;
	messages: SessionTranscriptMessage[];
}

export interface ApiError {
	error: string;
	message: string;
}

// ── Monitor types ───────────────────────────────────────────

export interface SessionMemoryResponse {
	session_id: string;
	content: string;
}

export interface SessionFileInfo {
	name: string;
	size_bytes: number;
	modified: string;
}

export interface SessionFilesResponse {
	session_id: string;
	files: SessionFileInfo[];
}

// ── View types ──────────────────────────────────────────────

export type ActiveView = 'chat' | 'status';
