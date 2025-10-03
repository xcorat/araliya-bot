"""
Unit tests for Pydantic models.
"""

import pytest
from datetime import datetime
from pydantic import ValidationError

from app.models.chat import ChatMessage, ChatRequest, ChatResponse, HealthResponse


class TestChatMessage:
    """Tests for ChatMessage model."""
    
    def test_valid_chat_message(self):
        """Test creating valid chat message."""
        message = ChatMessage(role="user", content="Hello world")
        
        assert message.role == "user"
        assert message.content == "Hello world"
        assert isinstance(message.timestamp, datetime)
    
    def test_chat_message_with_timestamp(self):
        """Test chat message with custom timestamp."""
        custom_time = datetime(2025, 1, 1, 12, 0, 0)
        message = ChatMessage(role="assistant", content="Response", timestamp=custom_time)
        
        assert message.timestamp == custom_time


class TestChatRequest:
    """Tests for ChatRequest model."""
    
    def test_valid_chat_request(self):
        """Test creating valid chat request."""
        request = ChatRequest(message="Hello", session_id="test-123")
        
        assert request.message == "Hello"
        assert request.session_id == "test-123"
    
    def test_chat_request_without_session_id(self):
        """Test chat request without session ID."""
        request = ChatRequest(message="Hello")
        
        assert request.message == "Hello"
        assert request.session_id is None
    
    def test_chat_request_validation_errors(self):
        """Test chat request validation errors."""
        # Empty message
        with pytest.raises(ValidationError):
            ChatRequest(message="")
        
        # Message too long
        with pytest.raises(ValidationError):
            ChatRequest(message="x" * 2001)
        
        # Missing message
        with pytest.raises(ValidationError):
            ChatRequest()


class TestChatResponse:
    """Tests for ChatResponse model."""
    
    def test_valid_chat_response(self):
        """Test creating valid chat response."""
        response = ChatResponse(
            message="AI response",
            session_id="test-123",
            metadata={"model": "gpt-3.5-turbo", "tokens": 25}
        )
        
        assert response.message == "AI response"
        assert response.session_id == "test-123"
        assert response.metadata["model"] == "gpt-3.5-turbo"
        assert isinstance(response.timestamp, datetime)
    
    def test_chat_response_default_metadata(self):
        """Test chat response with default metadata."""
        response = ChatResponse(message="Response", session_id="test")
        
        assert response.metadata == {}
    
    def test_chat_response_validation_errors(self):
        """Test chat response validation errors."""
        # Missing required fields
        with pytest.raises(ValidationError):
            ChatResponse()
        
        with pytest.raises(ValidationError):
            ChatResponse(message="Response")


class TestHealthResponse:
    """Tests for HealthResponse model."""
    
    def test_valid_health_response(self):
        """Test creating valid health response."""
        response = HealthResponse(
            status="healthy",
            version="1.0.0",
            openai_status="connected",
            uptime_seconds=3600.5
        )
        
        assert response.status == "healthy"
        assert response.version == "1.0.0"
        assert response.openai_status == "connected"
        assert response.uptime_seconds == 3600.5
        assert isinstance(response.timestamp, datetime)
    
    def test_health_response_without_uptime(self):
        """Test health response without uptime."""
        response = HealthResponse(
            status="healthy",
            version="1.0.0",
            openai_status="connected"
        )
        
        assert response.uptime_seconds is None
    
    def test_health_response_validation_errors(self):
        """Test health response validation errors."""
        # Missing required fields
        with pytest.raises(ValidationError):
            HealthResponse()
        
        with pytest.raises(ValidationError):
            HealthResponse(status="healthy")
