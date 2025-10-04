"""Araliya Bot - HF Space Gradio Application
Main entry point for the Hugging Face Space with ZeroGPU support.
"""

import os
import sys
import logging
import gradio as gr
from typing import List, Tuple

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s"
)
logger = logging.getLogger(__name__)

# Import spaces for GPU decorator
try:
    import spaces
except ImportError:
    # Create a no-op decorator for local development
    class MockSpaces:
        @staticmethod
        def GPU(duration=60):
            def decorator(func):
                return func
            return decorator
    spaces = MockSpaces()

# Import services
from config import get_settings
from services.openai_service import OpenAIService
from services.session_manager import session_manager
from services.rag_service import initialize_rag_service, get_rag_service

# Initialize services in main process with CPU-only models
logger.info("Initializing services in main process...")

# Initialize OpenAI service
openai_service = OpenAIService()

# Initialize RAG service with CPU-only models
try:
    logger.info("Initializing RAG service with CPU models...")
    initialize_rag_service()
    rag_service = get_rag_service()
    logger.info("RAG service initialized successfully")
except Exception as e:
    logger.error(f"Failed to initialize RAG service: {e}")
    rag_service = None

# GPU-accelerated chat processing function following HF Spaces patterns
@spaces.GPU(duration=120)
def process_chat_message(message: str, session_id: str = "default") -> str:
    """Process chat messages with GPU acceleration.
    
    This function follows the correct HF Spaces ZeroGPU pattern:
    - Synchronous function decorated with @spaces.GPU
    - Contains all GPU-intensive operations (RAG context generation)
    - Returns the final result
    """
    try:
        # Create or get session
        session_id = session_manager.create_session(session_id)
        
        # Get conversation history
        conversation_history = session_manager.get_conversation_history(session_id)
        
        # Add user message to session
        session_manager.add_user_message(session_id, message)
        
        # Get RAG context (model already on GPU from main process)
        context = rag_service.get_context(message) if rag_service else ""
        
        # Generate AI response with RAG context
        response_data = openai_service.generate_response(
            user_message=message,
            conversation_history=conversation_history,
            context=context
        )
        
        # Add AI response to session
        session_manager.add_assistant_message(session_id, response_data["message"])
        
        return response_data["message"]
        
    except Exception as e:
        logger.error(f"Chat processing error: {e}")
        return f"I'm sorry, an error occurred: {str(e)[:100]}... Please try again."

# Simple health check function (no GPU needed for this)
def get_health_status() -> str:
    """Get system health status."""
    try:
        # Check OpenAI connectivity
        openai_connected = openai_service.check_connectivity()
        rag_initialized = rag_service.is_initialized() if rag_service else False
        
        status_parts = []
        if openai_connected:
            status_parts.append("✅ OpenAI: Connected")
        else:
            status_parts.append("❌ OpenAI: Disconnected")
            
        if rag_initialized:
            status_parts.append("✅ RAG: Active")
        else:
            status_parts.append("⚠️ RAG: Inactive")
            
        return "\n".join(status_parts)
        
    except Exception as e:
        logger.error(f"Health check failed: {e}")
        return f"❌ System Error: {str(e)[:100]}"

# Create the Gradio interface following HF Spaces best practices
def respond(message: str, history: List[Tuple[str, str]], session_id: str = "default") -> Tuple[str, List[Tuple[str, str]]]:
    """Gradio chat response function.
    
    This function handles the chat interface and calls the GPU-decorated function.
    Following Gradio best practices for chat interfaces.
    """
    if not message.strip():
        return "", history
    
    # Call the GPU-decorated chat processing function
    bot_response = process_chat_message(message, session_id)
    
    # Update history
    history.append((message, bot_response))
    
    return "", history


# Create the Gradio interface
with gr.Blocks(
    title="Araliya Bot",
    theme=gr.themes.Soft(),
    css=".gradio-container {max-width: 1200px; margin: auto;}"
) as demo:
    gr.Markdown(
        """
        # 🌺 Araliya Bot
        AI Assistant with Graph-RAG Knowledge System
        
        Ask me anything and I'll help you with information from my knowledge base!
        """
    )
    
    with gr.Row():
        with gr.Column(scale=4):
            chatbot = gr.Chatbot(
                height=600,
                placeholder="Hi! I'm Araliya, your AI assistant. How can I help you today?",
                show_copy_button=True
            )
            
            with gr.Row():
                msg = gr.Textbox(
                    placeholder="Type your message here...",
                    container=False,
                    scale=4
                )
                submit_btn = gr.Button("Send", variant="primary", scale=1)
            
            with gr.Row():
                clear_btn = gr.Button("Clear Chat", variant="secondary")
                
        with gr.Column(scale=1):
            session_id = gr.Textbox(
                value="default",
                label="Session ID",
                info="Change this to start a new conversation"
            )
            
            with gr.Accordion("System Status", open=False):
                health_status = gr.Textbox(
                    label="Health Check",
                    value="Loading...",
                    interactive=False,
                    lines=3
                )
                refresh_btn = gr.Button("Refresh Status", size="sm")
    
    # Event handlers
    msg.submit(
        respond,
        inputs=[msg, chatbot, session_id],
        outputs=[msg, chatbot]
    )
    
    submit_btn.click(
        respond,
        inputs=[msg, chatbot, session_id],
        outputs=[msg, chatbot]
    )
    
    clear_btn.click(
        lambda: ([], "default"),
        outputs=[chatbot, session_id]
    )
    
    refresh_btn.click(
        get_health_status,
        outputs=[health_status]
    )
    
    # Initialize health status on load
    demo.load(
        get_health_status,
        outputs=[health_status]
    )

# Launch the app
if __name__ == "__main__":
    demo.queue()  # Enable API queue processing for Blocks
    demo.launch(
        server_name="0.0.0.0",
        server_port=7860
    )
