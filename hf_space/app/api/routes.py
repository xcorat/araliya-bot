"""
API routes for the Araliya Bot HF Space.
"""

import logging
import time
from datetime import datetime
from fastapi import APIRouter, HTTPException, status
from fastapi.responses import JSONResponse

# Import spaces for GPU decorator
try:
    import spaces
    HF_SPACES_AVAILABLE = True
except ImportError:
    HF_SPACES_AVAILABLE = False
    # Create a no-op decorator for local development
    def spaces_gpu_decorator(func):
        return func
    spaces = type('MockSpaces', (), {'GPU': spaces_gpu_decorator})()

# Add the parent directory to Python path for imports
import sys
import os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__ if '__file__' in globals() else '.')))

from config import get_settings
from models.chat import ChatRequest, ChatResponse, HealthResponse
from services.openai_service import OpenAIService
from services.session_manager import session_manager
from services.rag_service import get_rag_service

logger = logging.getLogger(__name__)

# Initialize router
router = APIRouter()

# Initialize services
openai_service = OpenAIService()
start_time = time.time()


@router.get("/health", response_model=HealthResponse)
async def health_check():
    """
    Health check endpoint that verifies system status and OpenAI connectivity.
    """
    try:
        settings = get_settings()
        
        # Check OpenAI connectivity
        openai_connected = await openai_service.check_connectivity()
        openai_status = "connected" if openai_connected else "disconnected"
        
        # Calculate uptime
        uptime_seconds = time.time() - start_time
        
        # Determine overall status
        overall_status = "healthy" if openai_connected else "degraded"
        
        response = HealthResponse(
            status=overall_status,
            timestamp=datetime.utcnow(),
            version="1.0.0",
            openai_status=openai_status,
            uptime_seconds=round(uptime_seconds, 2)
        )
        
        status_code = status.HTTP_200_OK if openai_connected else status.HTTP_503_SERVICE_UNAVAILABLE
        
        logger.info(f"Health check completed: {overall_status}")
        return JSONResponse(
            content=response.dict(),
            status_code=status_code
        )
        
    except Exception as e:
        logger.error(f"Health check failed: {e}")
        return JSONResponse(
            content={
                "status": "unhealthy",
                "timestamp": datetime.utcnow().isoformat(),
                "version": "1.0.0",
                "openai_status": "unknown",
                "error": "Health check failed"
            },
            status_code=status.HTTP_503_SERVICE_UNAVAILABLE
        )


@router.post("/chat", response_model=ChatResponse)
@spaces.GPU
async def chat_endpoint(request: ChatRequest):
    """
    Chat endpoint that processes user messages and returns AI responses.
    """
    try:
        settings = get_settings()
        
        # Check concurrent session limit
        active_sessions = session_manager.get_active_session_count()
        if active_sessions >= settings.max_concurrent_sessions:
            logger.warning(f"Maximum concurrent sessions reached: {active_sessions}")
            raise HTTPException(
                status_code=status.HTTP_429_TOO_MANY_REQUESTS,
                detail="Maximum number of concurrent sessions reached. Please try again later."
            )
        
        # Create or get session
        session_id = session_manager.create_session(request.session_id)
        
        # Get conversation history
        conversation_history = session_manager.get_conversation_history(session_id)
        
        # Add user message to session
        session_manager.add_user_message(session_id, request.message)
        
        # Get RAG context
        rag_service = get_rag_service()
        context = rag_service.get_context(request.message)
        
        # Generate AI response with RAG context
        response_data = await openai_service.generate_response(
            user_message=request.message,
            conversation_history=conversation_history,
            context=context
        )
        
        # Add AI response to session
        session_manager.add_assistant_message(session_id, response_data["message"])
        
        # Create response
        chat_response = ChatResponse(
            message=response_data["message"],
            session_id=session_id,
            timestamp=datetime.utcnow(),
            metadata=response_data["metadata"]
        )
        
        logger.info(f"Chat response generated for session {session_id}")
        return chat_response
        
    except HTTPException:
        # Re-raise HTTP exceptions
        raise
        
    except Exception as e:
        logger.error(f"Chat endpoint error: {e}")
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail="An error occurred while processing your request. Please try again."
        )


@router.get("/sessions/{session_id}")
async def get_session_info(session_id: str):
    """
    Get information about a specific session.
    """
    try:
        session_info = session_manager.get_session_info(session_id)
        
        if not session_info:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Session not found"
            )
        
        return session_info
        
    except HTTPException:
        raise
        
    except Exception as e:
        logger.error(f"Error getting session info: {e}")
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail="Error retrieving session information"
        )


@router.delete("/sessions/{session_id}")
async def clear_session(session_id: str):
    """
    Clear a specific session and its conversation history.
    """
    try:
        cleared = session_manager.clear_session(session_id)
        
        if not cleared:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Session not found"
            )
        
        return {"message": f"Session {session_id} cleared successfully"}
        
    except HTTPException:
        raise
        
    except Exception as e:
        logger.error(f"Error clearing session: {e}")
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail="Error clearing session"
        )


@router.get("/rag/status")
async def get_rag_status():
    """
    Get RAG service status and statistics.
    """
    try:
        rag_service = get_rag_service()
        stats = rag_service.get_stats()
        
        return {
            "status": "active",
            "timestamp": datetime.utcnow(),
            **stats
        }
        
    except Exception as e:
        logger.error(f"Error getting RAG status: {e}")
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail="Error retrieving RAG status"
        )
