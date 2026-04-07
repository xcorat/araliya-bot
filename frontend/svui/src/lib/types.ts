// ── Chat types ──────────────────────────────────────────────

/** Wall-clock latency reported by the backend for a single LLM turn. */
export interface LlmTiming {
	/** Time-to-first-token in ms (streaming only; absent for non-streaming). */
	ttft_ms?: number;
	/** Total request duration in ms. */
	total_ms: number;
}

export interface ChatMessage {
	id: string;
	role: 'user' | 'assistant' | 'system' | 'error';
	content: string;
	timestamp: string;
	intermediateSteps?: ToolStep[];
	/** Internal chain-of-thought from reasoning models (Qwen3, DeepSeek-R1, …). */
	thinking?: string;
	/** Per-turn token usage from the backend. */
	usage?: UsageInfo;
	/** Per-turn wall-clock timing from the backend. */
	timing?: LlmTiming;
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
}

// ── API response types ──────────────────────────────────────

export interface MessageResponse {
	session_id: string;
	mode: SessionMode;
	run_id?: string;
	reply: string;
	/** Internal chain-of-thought from reasoning models. Null for standard models. */
	thinking?: string | null;
	working_memory_updated: boolean;
	intermediate_steps?: ToolStep[];
	usage?: UsageInfo;
	timing?: LlmTiming;
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
	uptime_ms?: number;
	main_process?: MainProcessStatus;
	subsystems?: SubsystemStatus[];
}

export interface MainProcessStatus {
	id: string;
	name: string;
	status: string;
	uptime_ms: number;
	details?: Record<string, unknown>;
}

export interface SubsystemStatus {
	id: string;
	name: string;
	status: string;
	state?: string;
	details?: Record<string, unknown>;
}

export interface SessionInfo {
	session_id: string;
	created_at: string;
	updated_at: string | null;
	mode: SessionMode;
	last_agent?: string | null;
	store_types?: string[];
}

export interface SessionsResponse {
	sessions: SessionInfo[];
}

export interface AgentInfo {
	agent_id: string;
	name: string;
	last_fetched: string | null;
	session_count: number;
	store_types?: string[];
}

export interface AgentsResponse {
	agents: AgentInfo[];
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
	session_id: string;
	transcript: SessionTranscriptMessage[];
}

export interface ApiError {
	error: string;
	message: string;
}

// ── Component tree types ────────────────────────────────────

export interface TreeNode {
	id: string;
	name: string;
	status: string;
	state: string;
	uptime_ms?: number;
	children: TreeNode[];
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

// ── Knowledge Graph types ────────────────────────────────────

export type KgEntityKind = 'concept' | 'system' | 'person' | 'term' | 'acronym';

export interface KgEntity {
	id: string;
	name: string;
	kind: KgEntityKind;
	mention_count: number;
	source_chunks: string[];
}

export interface KgRelation {
	from: string;
	to: string;
	label: string;
	weight: number;
	source_chunks: string[];
}

export interface KgGraph {
	entities: Record<string, KgEntity>;
	relations: KgRelation[];
}

export interface AgentKGResponse {
	agent_id: string;
	graph: KgGraph;
}

/** Accumulated spend (tokens + cost) for an agent's active session. */
export interface SessionSpend {
	total_input_tokens: number;
	total_output_tokens: number;
	total_cached_tokens: number;
	total_cost_usd: number;
	last_updated: string;
}

export interface AgentSpendResponse {
	session_id: string | null;
	spend: SessionSpend | null;
}

// ── Agent debug types ────────────────────────────────────────

export interface SessionDebugTurn {
	n: number;
	user_input: string;
	instruct_prompt: string;
	instruction_response: string;
	tool_calls_json: string;
	tool_outputs_json: string;
	context: string;
	response_prompt: string;
}

export interface SessionDebugResponse {
	session_id: string;
	turns: SessionDebugTurn[];
}

// ── Observability types ─────────────────────────────────────

export type ObsLevel = 'TRACE' | 'DEBUG' | 'INFO' | 'WARN' | 'ERROR';

export interface ObsEvent {
	level: ObsLevel;
	target: string;
	message: string;
	fields: Record<string, unknown> | null;
	session_id?: string;
	request_id?: string;
	span_id?: string;
	ts_unix_ms: number;
}

// ── LLM types ───────────────────────────────────────────────

export interface LlmProviderInfo {
	name: string;
	model: string;
	active: boolean;
}

export interface LlmRouteInfo {
	hint: string;
	provider: string;
	model: string;
}

export interface LlmProvidersResponse {
	providers: LlmProviderInfo[];
	routes: LlmRouteInfo[];
	active: string;
}

export interface LlmSetDefaultResponse {
	ok: boolean;
	previous: string;
	active: string;
}
