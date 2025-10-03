"""
Unit tests for service classes.
"""

import pytest
from unittest.mock import Mock, patch
from datetime import datetime, timedelta

from app.services.session_manager import SessionManager
from app.services.openai_service import OpenAIService
from app.models.chat import ChatMessage


class TestSessionManager:
    """Tests for SessionManager class."""
    
    def test_generate_session_id(self, session_manager):
        """Test session ID generation."""
        session_id = session_manager.generate_session_id()
        assert isinstance(session_id, str)
        assert len(session_id) > 0
        
        # Should generate unique IDs
        another_id = session_manager.generate_session_id()
        assert session_id != another_id
    
    def test_create_session(self, session_manager):
        """Test session creation."""
        session_id = session_manager.create_session()
        assert isinstance(session_id, str)
        assert session_manager.session_exists(session_id)
    
    def test_create_session_with_existing_id(self, session_manager):
        """Test creating session with existing ID."""
        existing_id = "test-session-123"
        
        # Create session with specific ID
        returned_id = session_manager.create_session(existing_id)
        assert returned_id == existing_id
        
        # Creating again should return same ID
        returned_id2 = session_manager.create_session(existing_id)
        assert returned_id2 == existing_id
    
    def test_conversation_history(self, session_manager):
        """Test conversation history management."""
        session_id = session_manager.create_session()
        
        # Initially empty
        history = session_manager.get_conversation_history(session_id)
        assert len(history) == 0
        
        # Add messages
        session_manager.add_user_message(session_id, "Hello")
        session_manager.add_assistant_message(session_id, "Hi there!")
        
        history = session_manager.get_conversation_history(session_id)
        assert len(history) == 2
        assert history[0].role == "user"
        assert history[0].content == "Hello"
        assert history[1].role == "assistant"
        assert history[1].content == "Hi there!"
    
    def test_conversation_history_limit(self, session_manager):
        """Test conversation history length limiting."""
        session_id = session_manager.create_session()
        
        # Add more messages than the limit
        for i in range(25):
            session_manager.add_user_message(session_id, f"Message {i}")
        
        history = session_manager.get_conversation_history(session_id)
        # Should be limited to max_conversation_history (20)
        assert len(history) <= session_manager.settings.max_conversation_history
    
    def test_session_info(self, session_manager):
        """Test getting session information."""
        session_id = session_manager.create_session()
        session_manager.add_user_message(session_id, "Test message")
        
        info = session_manager.get_session_info(session_id)
        assert info is not None
        assert info["session_id"] == session_id
        assert "created_at" in info
        assert "last_activity" in info
        assert info["message_count"] == 1
    
    def test_session_cleanup(self, session_manager):
        """Test session cleanup functionality."""
        # Create a session
        session_id = session_manager.create_session()
        
        # Manually set old last_activity
        with session_manager._lock:
            old_time = datetime.utcnow() - timedelta(hours=2)
            session_manager._sessions[session_id]["last_activity"] = old_time
        
        # Run cleanup
        cleaned = session_manager.cleanup_expired_sessions()
        assert cleaned == 1
        assert not session_manager.session_exists(session_id)
    
    def test_clear_session(self, session_manager):
        """Test clearing specific session."""
        session_id = session_manager.create_session()
        assert session_manager.session_exists(session_id)
        
        cleared = session_manager.clear_session(session_id)
        assert cleared is True
        assert not session_manager.session_exists(session_id)
        
        # Clearing non-existent session should return False
        cleared = session_manager.clear_session("nonexistent")
        assert cleared is False


class TestOpenAIService:
    """Tests for OpenAIService class."""
    
    @patch("app.services.openai_service.OpenAI")
    def test_initialization(self, mock_openai_client):
        """Test OpenAI service initialization."""
        service = OpenAIService()
        assert service.client is not None
        mock_openai_client.assert_called_once()
    
    @patch("app.services.openai_service.OpenAI")
    def test_check_connectivity_success(self, mock_openai_client):
        """Test successful connectivity check."""
        # Mock successful models.list() call
        mock_openai_client.return_value.models.list.return_value = []
        
        service = OpenAIService()
        result = service.check_connectivity()
        assert result is True
    
    @patch("app.services.openai_service.OpenAI")
    def test_check_connectivity_failure(self, mock_openai_client):
        """Test failed connectivity check."""
        # Mock failed models.list() call
        mock_openai_client.return_value.models.list.side_effect = Exception("API Error")
        
        service = OpenAIService()
        result = service.check_connectivity()
        assert result is False
    
    def test_format_conversation_history(self):
        """Test conversation history formatting."""
        with patch("app.services.openai_service.OpenAI"):
            service = OpenAIService()
            
            messages = [
                ChatMessage(role="user", content="Hello"),
                ChatMessage(role="assistant", content="Hi there!"),
                ChatMessage(role="user", content="How are you?")
            ]
            
            formatted = service._format_conversation_history(messages)
            
            # Should include system message + conversation
            assert len(formatted) == 4
            assert formatted[0]["role"] == "system"
            assert formatted[1]["role"] == "user"
            assert formatted[1]["content"] == "Hello"
            assert formatted[2]["role"] == "assistant"
            assert formatted[2]["content"] == "Hi there!"
    
    @patch("app.services.openai_service.OpenAI")
    def test_generate_response_success(self, mock_openai_client):
        """Test successful response generation."""
        # Mock successful API response
        mock_response = Mock()
        mock_response.choices = [Mock()]
        mock_response.choices[0].message.content = "Test response"
        mock_response.choices[0].finish_reason = "stop"
        mock_response.usage.total_tokens = 50
        
        mock_openai_client.return_value.chat.completions.create.return_value = mock_response
        
        service = OpenAIService()
        
        result = service.generate_response("Hello", [])
        
        assert result["message"] == "Test response"
        assert result["metadata"]["tokens_used"] == 50
        assert result["metadata"]["finish_reason"] == "stop"
        assert "response_time_ms" in result["metadata"]
    
    @patch("app.services.openai_service.OpenAI")
    def test_generate_response_with_history(self, mock_openai_client):
        """Test response generation with conversation history."""
        mock_response = Mock()
        mock_response.choices = [Mock()]
        mock_response.choices[0].message.content = "Response with context"
        mock_response.choices[0].finish_reason = "stop"
        mock_response.usage.total_tokens = 75
        
        mock_openai_client.return_value.chat.completions.create.return_value = mock_response
        
        service = OpenAIService()
        
        history = [
            ChatMessage(role="user", content="Previous message"),
            ChatMessage(role="assistant", content="Previous response")
        ]
        
        result = service.generate_response("Current message", history)
        
        # Verify the API was called with formatted messages
        call_args = mock_openai_client.return_value.chat.completions.create.call_args
        messages = call_args[1]["messages"]
        
        # Should have system + history + current message
        assert len(messages) == 4  # system + 2 history + 1 current
        assert messages[-1]["content"] == "Current message"
