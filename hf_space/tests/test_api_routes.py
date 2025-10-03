"""
Integration tests for API routes.
"""

import pytest
from fastapi import status
from unittest.mock import patch


class TestHealthEndpoint:
    """Tests for health check endpoint."""
    
    def test_health_check_success(self, client, mock_openai_service):
        """Test successful health check."""
        response = client.get("/api/v1/health")
        
        assert response.status_code == status.HTTP_200_OK
        data = response.json()
        assert data["status"] == "healthy"
        assert data["version"] == "1.0.0"
        assert data["openai_status"] == "connected"
        assert "timestamp" in data
        assert "uptime_seconds" in data
    
    def test_health_check_openai_failure(self, client):
        """Test health check with OpenAI connectivity failure."""
        with patch("app.api.routes.openai_service.check_connectivity", return_value=False):
            response = client.get("/api/v1/health")
            
            assert response.status_code == status.HTTP_503_SERVICE_UNAVAILABLE
            data = response.json()
            assert data["status"] == "degraded"
            assert data["openai_status"] == "disconnected"


class TestChatEndpoint:
    """Tests for chat endpoint."""
    
    def test_chat_success(self, client, mock_openai_service, sample_chat_request):
        """Test successful chat interaction."""
        response = client.post("/api/v1/chat", json=sample_chat_request)
        
        assert response.status_code == status.HTTP_200_OK
        data = response.json()
        assert data["message"] == "Test AI response"
        assert data["session_id"] == sample_chat_request["session_id"]
        assert "timestamp" in data
        assert "metadata" in data
    
    def test_chat_new_session(self, client, mock_openai_service):
        """Test chat with new session creation."""
        request_data = {"message": "Hello!"}
        response = client.post("/api/v1/chat", json=request_data)
        
        assert response.status_code == status.HTTP_200_OK
        data = response.json()
        assert "session_id" in data
        assert len(data["session_id"]) > 0
    
    def test_chat_invalid_request(self, client):
        """Test chat with invalid request data."""
        # Empty message
        response = client.post("/api/v1/chat", json={"message": ""})
        assert response.status_code == status.HTTP_422_UNPROCESSABLE_ENTITY
        
        # Missing message
        response = client.post("/api/v1/chat", json={})
        assert response.status_code == status.HTTP_422_UNPROCESSABLE_ENTITY
        
        # Message too long
        long_message = "x" * 2001
        response = client.post("/api/v1/chat", json={"message": long_message})
        assert response.status_code == status.HTTP_422_UNPROCESSABLE_ENTITY
    
    def test_chat_openai_error(self, client):
        """Test chat with OpenAI service error."""
        with patch("app.api.routes.openai_service.generate_response", side_effect=Exception("OpenAI error")):
            response = client.post("/api/v1/chat", json={"message": "Hello"})
            assert response.status_code == status.HTTP_500_INTERNAL_SERVER_ERROR


class TestSessionEndpoints:
    """Tests for session management endpoints."""
    
    def test_get_session_info(self, client, mock_openai_service):
        """Test getting session information."""
        # First create a session by sending a chat message
        chat_response = client.post("/api/v1/chat", json={"message": "Hello", "session_id": "test-session"})
        assert chat_response.status_code == status.HTTP_200_OK
        
        # Then get session info
        response = client.get("/api/v1/sessions/test-session")
        assert response.status_code == status.HTTP_200_OK
        
        data = response.json()
        assert data["session_id"] == "test-session"
        assert "created_at" in data
        assert "last_activity" in data
        assert "message_count" in data
    
    def test_get_nonexistent_session(self, client):
        """Test getting info for nonexistent session."""
        response = client.get("/api/v1/sessions/nonexistent")
        assert response.status_code == status.HTTP_404_NOT_FOUND
    
    def test_clear_session(self, client, mock_openai_service):
        """Test clearing a session."""
        # First create a session
        chat_response = client.post("/api/v1/chat", json={"message": "Hello", "session_id": "test-clear"})
        assert chat_response.status_code == status.HTTP_200_OK
        
        # Clear the session
        response = client.delete("/api/v1/sessions/test-clear")
        assert response.status_code == status.HTTP_200_OK
        
        # Verify session is gone
        get_response = client.get("/api/v1/sessions/test-clear")
        assert get_response.status_code == status.HTTP_404_NOT_FOUND
    
    def test_clear_nonexistent_session(self, client):
        """Test clearing a nonexistent session."""
        response = client.delete("/api/v1/sessions/nonexistent")
        assert response.status_code == status.HTTP_404_NOT_FOUND


class TestRootEndpoint:
    """Tests for root endpoint."""
    
    def test_root_endpoint(self, client):
        """Test root endpoint returns basic info."""
        response = client.get("/")
        
        assert response.status_code == status.HTTP_200_OK
        data = response.json()
        assert data["message"] == "Araliya Bot API - Phase 1"
        assert data["version"] == "1.0.0"
        assert data["status"] == "active"
        assert data["docs"] == "/docs"
