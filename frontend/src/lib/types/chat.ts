export interface ChatMessage {
  id: string;
  content: string;
  role: 'user' | 'assistant';
  timestamp: Date;
  metadata?: {
    tokens?: number;
    model?: string;
    sources?: ContextSource[];
    processingTime?: number;
  };
}

export interface ContextSource {
  id: string;
  title: string;
  excerpt: string;
  relevance: number;
  url?: string;
  type: 'document' | 'blog' | 'knowledge_base';
}

export interface ChatSession {
  id: string;
  title: string;
  createdAt: Date;
  updatedAt: Date;
  messageCount: number;
  lastMessage?: string;
  messages: ChatMessage[];
}

export interface StreamingState {
  isStreaming: boolean;
  currentMessage?: string;
  sessionId?: string;
}
