import type { ChatRequest, ChatResponse, HealthStatus, APIError } from '$lib/types/api.js';

const API_BASE_URL = 'https://your-hf-space-url.hf.space'; // TODO: Replace with actual HF Space URL

class APIClient {
  private baseURL: string;
  private timeout: number;

  constructor(baseURL: string = API_BASE_URL, timeout: number = 30000) {
    this.baseURL = baseURL;
    this.timeout = timeout;
  }

  private async request<T>(
    endpoint: string,
    options: RequestInit = {}
  ): Promise<T> {
    const url = `${this.baseURL}${endpoint}`;
    
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), this.timeout);

    try {
      const response = await fetch(url, {
        ...options,
        headers: {
          'Content-Type': 'application/json',
          ...options.headers,
        },
        signal: controller.signal,
      });

      clearTimeout(timeoutId);

      if (!response.ok) {
        const error: APIError = {
          type: response.status >= 500 ? 'SERVER_ERROR' : 'VALIDATION_ERROR',
          message: `HTTP ${response.status}: ${response.statusText}`,
          statusCode: response.status,
          retryable: response.status >= 500,
        };
        throw error;
      }

      return await response.json();
    } catch (err) {
      clearTimeout(timeoutId);
      
      if (err instanceof Error) {
        if (err.name === 'AbortError') {
          const error: APIError = {
            type: 'TIMEOUT_ERROR',
            message: 'Request timed out',
            retryable: true,
          };
          throw error;
        }
        
        const error: APIError = {
          type: 'NETWORK_ERROR',
          message: err.message,
          retryable: true,
        };
        throw error;
      }
      
      throw err;
    }
  }

  async sendMessage(request: ChatRequest): Promise<ChatResponse> {
    return this.request<ChatResponse>('/api/v1/chat', {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  async healthCheck(): Promise<HealthStatus> {
    return this.request<HealthStatus>('/api/v1/health');
  }
}

export const apiClient = new APIClient();
