"""
Pytest configuration and fixtures.
"""

import os
import pytest
from fastapi.testclient import TestClient
from unittest.mock import Mock, patch

# Set test environment variables
os.environ["OPENAI_API_KEY"] = "test-api-key"
os.environ["ENVIRONMENT"] = "test"

from app.main import app
from app.services.session_manager import SessionManager


@pytest.fixture
def client():
    """FastAPI test client."""
    return TestClient(app)


@pytest.fixture
def mock_openai_service():
    """Mock OpenAI service for testing."""
    with patch("app.services.openai_service.OpenAI") as mock_client:
        # Mock successful response
        mock_response = Mock()
        mock_response.choices = [Mock()]
        mock_response.choices[0].message.content = "Test AI response"
        mock_response.choices[0].finish_reason = "stop"
        mock_response.usage.total_tokens = 25
        
        mock_client.return_value.chat.completions.create.return_value = mock_response
        mock_client.return_value.models.list.return_value = []
        
        yield mock_client


@pytest.fixture
def session_manager():
    """Fresh session manager for testing."""
    return SessionManager()


@pytest.fixture
def sample_chat_request():
    """Sample chat request data."""
    return {
        "message": "Hello, how are you?",
        "session_id": "test-session-123"
    }
