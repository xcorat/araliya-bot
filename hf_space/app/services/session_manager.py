"""
Session management for conversation context.
"""

import asyncio
import logging
import uuid
from datetime import datetime, timedelta
from typing import Dict, List, Optional
import threading

from app.config import get_settings
from app.models.chat import ChatMessage

logger = logging.getLogger(__name__)


class SessionManager:
    """Manages user sessions and conversation history."""
    
    def __init__(self):
        """Initialize session manager."""
        self.settings = get_settings()
        self._sessions: Dict[str, Dict] = {}
        self._lock = threading.RLock()
        self._cleanup_task: Optional[asyncio.Task] = None
        self._start_cleanup_task()
    
    def _start_cleanup_task(self):
        """Start the background cleanup task."""
        try:
            loop = asyncio.get_event_loop()
            if not loop.is_running():
                return
            self._cleanup_task = loop.create_task(self._periodic_cleanup())
        except RuntimeError:
            # No event loop running, cleanup will be manual
            logger.warning("No event loop available for automatic session cleanup")
    
    async def _periodic_cleanup(self):
        """Periodically clean up expired sessions."""
        while True:
            try:
                await asyncio.sleep(self.settings.session_cleanup_interval_minutes * 60)
                self.cleanup_expired_sessions()
            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error(f"Error in session cleanup task: {e}")
    
    def generate_session_id(self) -> str:
        """Generate a unique session ID."""
        return str(uuid.uuid4())
    
    def create_session(self, session_id: Optional[str] = None) -> str:
        """
        Create a new session or return existing session ID.
        
        Args:
            session_id: Optional existing session ID
            
        Returns:
            Session ID (new or existing)
        """
        with self._lock:
            if session_id and session_id in self._sessions:
                # Update last activity for existing session
                self._sessions[session_id]["last_activity"] = datetime.utcnow()
                return session_id
            
            # Create new session
            new_session_id = session_id or self.generate_session_id()
            self._sessions[new_session_id] = {
                "created_at": datetime.utcnow(),
                "last_activity": datetime.utcnow(),
                "messages": []
            }
            
            logger.info(f"Created new session: {new_session_id}")
            return new_session_id
    
    def get_conversation_history(self, session_id: str) -> List[ChatMessage]:
        """
        Get conversation history for a session.
        
        Args:
            session_id: Session identifier
            
        Returns:
            List of chat messages
        """
        with self._lock:
            if session_id not in self._sessions:
                return []
            
            messages = self._sessions[session_id]["messages"]
            # Return up to max_conversation_history messages
            return messages[-self.settings.max_conversation_history:]
    
    def add_message(self, session_id: str, message: ChatMessage) -> None:
        """
        Add a message to session history.
        
        Args:
            session_id: Session identifier
            message: Chat message to add
        """
        with self._lock:
            if session_id not in self._sessions:
                self.create_session(session_id)
            
            self._sessions[session_id]["messages"].append(message)
            self._sessions[session_id]["last_activity"] = datetime.utcnow()
            
            # Trim history if it exceeds max length
            messages = self._sessions[session_id]["messages"]
            if len(messages) > self.settings.max_conversation_history * 2:
                # Keep only the most recent max_conversation_history messages
                self._sessions[session_id]["messages"] = messages[-self.settings.max_conversation_history:]
    
    def add_user_message(self, session_id: str, content: str) -> None:
        """Add a user message to session history."""
        message = ChatMessage(role="user", content=content)
        self.add_message(session_id, message)
    
    def add_assistant_message(self, session_id: str, content: str) -> None:
        """Add an assistant message to session history."""
        message = ChatMessage(role="assistant", content=content)
        self.add_message(session_id, message)
    
    def session_exists(self, session_id: str) -> bool:
        """Check if a session exists."""
        with self._lock:
            return session_id in self._sessions
    
    def get_session_info(self, session_id: str) -> Optional[Dict]:
        """Get session information."""
        with self._lock:
            if session_id not in self._sessions:
                return None
            
            session = self._sessions[session_id]
            return {
                "session_id": session_id,
                "created_at": session["created_at"],
                "last_activity": session["last_activity"],
                "message_count": len(session["messages"])
            }
    
    def cleanup_expired_sessions(self) -> int:
        """
        Remove expired sessions.
        
        Returns:
            Number of sessions cleaned up
        """
        with self._lock:
            current_time = datetime.utcnow()
            timeout_delta = timedelta(minutes=self.settings.session_timeout_minutes)
            
            expired_sessions = [
                session_id for session_id, session_data in self._sessions.items()
                if current_time - session_data["last_activity"] > timeout_delta
            ]
            
            for session_id in expired_sessions:
                del self._sessions[session_id]
            
            if expired_sessions:
                logger.info(f"Cleaned up {len(expired_sessions)} expired sessions")
            
            return len(expired_sessions)
    
    def get_active_session_count(self) -> int:
        """Get the number of active sessions."""
        with self._lock:
            return len(self._sessions)
    
    def clear_session(self, session_id: str) -> bool:
        """
        Clear a specific session.
        
        Args:
            session_id: Session to clear
            
        Returns:
            True if session was found and cleared, False otherwise
        """
        with self._lock:
            if session_id in self._sessions:
                del self._sessions[session_id]
                logger.info(f"Cleared session: {session_id}")
                return True
            return False


# Global session manager instance
session_manager = SessionManager()
