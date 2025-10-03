"""
Araliya Bot - HF Space Gradio Application
Main entry point for the Hugging Face Space with ZeroGPU support.
"""

import os
import sys
import logging
import gradio as gr
from datetime import datetime
from typing import List, Dict, Any

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s"
)
logger = logging.getLogger(__name__)

# Add the parent directory to Python path for imports
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

# Import spaces for GPU decorator
try:
    import spaces
    HF_SPACES_AVAILABLE = True
    logger.info("HF Spaces ZeroGPU is available")
except ImportError:
    HF_SPACES_AVAILABLE = False
    # Create a no-op decorator for local development
    def spaces_gpu_decorator(func):
        return func
    spaces = type('MockSpaces', (), {'GPU': spaces_gpu_decorator})()
    logger.info("HF Spaces ZeroGPU is not available, using mock decorator")

# Import services
from config import get_settings
from services.openai_service import OpenAIService
from services.session_manager import session_manager
from services.rag_service import initialize_rag_service, get_rag_service

# Initialize services
openai_service = OpenAIService()

# Initialize RAG service
try:
    logger.info("Initializing RAG service...")
    initialize_rag_service()
    logger.info("RAG service initialized successfully")
except Exception as e:
    logger.error(f"Failed to initialize RAG service: {e}")
    # Continue startup even if RAG fails - the app can still work without it

# Synchronous function for GPU-intensive operations that can be properly decorated
@spaces.GPU
def _process_chat_gpu(message: str, conversation_history, context):
    """GPU-accelerated chat processing function.
    
    This synchronous function is properly decorated with @spaces.GPU and handles
    the GPU-intensive operations.
    """
    settings = get_settings()
    
    # Generate AI response with RAG context
    response_data = openai_service.generate_response_sync(
        user_message=message,
        conversation_history=conversation_history,
        context=context
    )
    
    return response_data

# Async chat processing function that calls the GPU-decorated function
async def process_chat(message: str, history: List[List[str]], session_id: str = "default") -> str:
    """
    Process chat messages with GPU acceleration.
    
    Args:
        message: User message
        history: Chat history
        session_id: Session identifier
        
    Returns:
        Bot response
    """
    try:
        settings = get_settings()
        
        # Create or get session
        session_id = session_manager.create_session(session_id)
        
        # Get conversation history
        conversation_history = session_manager.get_conversation_history(session_id)
        
        # Add user message to session
        session_manager.add_user_message(session_id, message)
        
        # Get RAG context
        rag_service = get_rag_service()
        context = rag_service.get_context(message)
        
        # Call the GPU-decorated synchronous function
        response_data = _process_chat_gpu(
            message=message,
            conversation_history=conversation_history,
            context=context
        )
        
        # Add AI response to session
        session_manager.add_assistant_message(session_id, response_data["message"])
        
        logger.info(f"Chat response generated for session {session_id}")
        return response_data["message"]
        
    except Exception as e:
        logger.error(f"Chat processing error: {e}")
        return f"I'm sorry, an error occurred: {str(e)[:100]}... Please try again."

# Synchronous function for GPU-intensive health check operations
@spaces.GPU
def _check_health_gpu():
    """GPU-accelerated health check function.
    
    This synchronous function is properly decorated with @spaces.GPU and handles
    the GPU-intensive operations for health checking.
    """
    settings = get_settings()
    
    # Check OpenAI connectivity
    openai_connected = openai_service.check_connectivity_sync()
    openai_status = "connected" if openai_connected else "disconnected"
    
    # Determine overall status
    overall_status = "healthy" if openai_connected else "degraded"
    
    return {
        "status": overall_status,
        "timestamp": datetime.utcnow().isoformat(),
        "version": "1.0.0",
        "openai_status": openai_status,
        "rag_status": "active" if get_rag_service().is_initialized() else "inactive"
    }

# Async health check function that calls the GPU-decorated function
async def check_health() -> Dict[str, Any]:
    """
    Check system health and OpenAI connectivity.
    """
    try:
        # Call the GPU-decorated synchronous function
        return _check_health_gpu()
        
    except Exception as e:
        logger.error(f"Health check failed: {e}")
        return {
            "status": "unhealthy",
            "timestamp": datetime.utcnow().isoformat(),
            "version": "1.0.0",
            "openai_status": "unknown",
            "error": str(e)[:100]
        }

# Create the Gradio interface
with gr.Blocks(title="Araliya Bot") as demo:
    gr.Markdown("# Araliya Bot")
    gr.Markdown("AI Assistant with Graph-RAG Knowledge System")
    
    with gr.Row():
        with gr.Column(scale=4):
            chatbot = gr.Chatbot(height=500)
            msg = gr.Textbox(placeholder="Ask me anything...", container=False)
            clear = gr.Button("Clear")
        
        with gr.Column(scale=1):
            session_id = gr.Textbox(value="default", label="Session ID")
            with gr.Accordion("System Status", open=False):
                health_info = gr.JSON(value={"status": "Loading..."})
                refresh_btn = gr.Button("Refresh Status")
    
    def update_chat(message, chat_history, session_id):
        if message.strip() == "":
            return "", chat_history
        
        # Add user message to history
        chat_history.append([message, None])
        return "", chat_history
    
    async def bot_response(chat_history, session_id):
        if len(chat_history) == 0:
            return chat_history
        
        user_message = chat_history[-1][0]
        bot_message = await process_chat(user_message, chat_history, session_id)
        
        # Update last message with bot response
        chat_history[-1][1] = bot_message
        return chat_history
    
    # Set up event handlers
    msg.submit(update_chat, [msg, chatbot, session_id], [msg, chatbot]).then(
        bot_response, [chatbot, session_id], [chatbot]
    )
    
    clear.click(lambda: ([], "default"), outputs=[chatbot, session_id])
    refresh_btn.click(check_health, outputs=[health_info])
    
    # Initialize health status on load
    demo.load(check_health, outputs=[health_info])

# Launch the app
if __name__ == "__main__":
    demo.launch(server_name="0.0.0.0", server_port=7860)
