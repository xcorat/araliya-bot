"""
Pydantic models for chat API requests and responses.
"""

from datetime import datetime
from typing import Optional, List, Dict, Any
from pydantic import BaseModel, Field


class ChatMessage(BaseModel):
    """Individual chat message model."""
    role: str = Field(..., description="Message role: 'user' or 'assistant'")
    content: str = Field(..., description="Message content")
    timestamp: datetime = Field(default_factory=datetime.utcnow)


class ChatRequest(BaseModel):
    """Request model for chat endpoint."""
    message: str = Field(..., min_length=1, max_length=2000, description="User message")
    session_id: Optional[str] = Field(None, description="Session ID for conversation context")
    
    class Config:
        schema_extra = {
            "example": {
                "message": "Hello, how can you help me today?",
                "session_id": "user-123-session"
            }
        }


class ChatResponse(BaseModel):
    """Response model for chat endpoint."""
    message: str = Field(..., description="AI-generated response")
    session_id: str = Field(..., description="Session ID for this conversation")
    timestamp: datetime = Field(default_factory=datetime.utcnow)
    metadata: Dict[str, Any] = Field(default_factory=dict, description="Additional response metadata")
    
    class Config:
        schema_extra = {
            "example": {
                "message": "Hello! I'm here to help you with any questions you have.",
                "session_id": "user-123-session",
                "timestamp": "2025-10-02T14:30:00Z",
                "metadata": {
                    "model": "gpt-3.5-turbo",
                    "tokens_used": 25,
                    "response_time_ms": 1250
                }
            }
        }


class HealthResponse(BaseModel):
    """Response model for health check endpoint."""
    status: str = Field(..., description="Service status")
    timestamp: datetime = Field(default_factory=datetime.utcnow)
    version: str = Field(..., description="API version")
    openai_status: str = Field(..., description="OpenAI connectivity status")
    uptime_seconds: Optional[float] = Field(None, description="Service uptime in seconds")
    
    class Config:
        schema_extra = {
            "example": {
                "status": "healthy",
                "timestamp": "2025-10-02T14:30:00Z",
                "version": "1.0.0",
                "openai_status": "connected",
                "uptime_seconds": 3600.5
            }
        }
