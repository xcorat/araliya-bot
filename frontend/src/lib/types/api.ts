import type { ChatMessage } from './chat.js';

export interface ChatRequest {
  message: string;
  session_id: string;
  history?: ChatMessage[];
}

export interface ChatResponse {
  message: ChatMessage;
  session_id: string;
  metadata: {
    processingTime: number;
    tokenUsage?: TokenUsage;
  };
}

export interface TokenUsage {
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
}

export interface HealthStatus {
  status: 'healthy' | 'unhealthy';
  timestamp: string;
  version?: string;
  uptime?: number;
}

export interface APIError {
  type: 'NETWORK_ERROR' | 'TIMEOUT_ERROR' | 'SERVER_ERROR' | 'VALIDATION_ERROR' | 'RATE_LIMIT_ERROR';
  message: string;
  statusCode?: number;
  retryable: boolean;
  retryAfter?: number;
}
