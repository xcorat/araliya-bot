"""
OpenAI service for handling LLM interactions.
"""

import logging
import time
from typing import List, Dict, Any
import openai
from openai import OpenAI

# Import from app package
from config import get_settings
from models.chat import ChatMessage

logger = logging.getLogger(__name__)


class OpenAIService:
    """Service for OpenAI API interactions."""
    
    def __init__(self):
        """Initialize OpenAI service with configuration."""
        self.settings = get_settings()
        self.client = OpenAI(api_key=self.settings.openai_api_key)
        self._validate_configuration()
    
    def _validate_configuration(self) -> None:
        """Validate OpenAI configuration."""
        if not self.settings.validate_openai_config():
            raise ValueError("Invalid OpenAI configuration. Please check your API key.")
    
    def check_connectivity(self) -> bool:
        """Check OpenAI API connectivity without making a full request."""
        try:
            # Simple way to check if API key is valid
            models = self.client.models.list()
            return True
        except Exception as e:
            logger.error(f"OpenAI connectivity check failed: {e}")
            return False
    
    def _format_conversation_history(self, messages: List[ChatMessage], context: str = "") -> List[Dict[str, str]]:
        """Format conversation history for OpenAI API."""
        formatted_messages = []
        
        # Create system message with optional RAG context
        system_content = "You are Araliya, a helpful AI assistant. Provide clear, concise, and helpful responses to user questions."
        
        if context:
            system_content += f"\n\nYou have access to the following relevant information to help answer questions:\n\n{context}\n\nUse this information to provide accurate and helpful responses. If the context doesn't contain relevant information for the user's question, you can still provide general assistance based on your knowledge."
        
        formatted_messages.append({
            "role": "system",
            "content": system_content
        })
        
        # Add conversation history
        for message in messages:
            formatted_messages.append({
                "role": message.role,
                "content": message.content
            })
        
        return formatted_messages
    
    def generate_response(
        self, 
        user_message: str, 
        conversation_history: List[ChatMessage],
        context: str = ""
    ) -> Dict[str, Any]:
        """
        Generate AI response using OpenAI API.
        
        Args:
            user_message: The user's message
            conversation_history: Previous conversation messages
            context: Optional RAG context to include in the prompt
            
        Returns:
            Dictionary containing response and metadata
        """
        start_time = time.time()
        
        try:
            # Add current user message to history
            current_messages = conversation_history + [
                ChatMessage(role="user", content=user_message)
            ]
            
            # Format for OpenAI API with context
            formatted_messages = self._format_conversation_history(current_messages, context)
            
            # Make API call
            response = self.client.chat.completions.create(
                model=self.settings.openai_model,
                messages=formatted_messages,
                max_tokens=self.settings.openai_max_tokens,
                temperature=self.settings.openai_temperature,
                timeout=self.settings.request_timeout_seconds
            )
            
            # Extract response
            ai_message = response.choices[0].message.content
            
            # Calculate response time
            response_time_ms = (time.time() - start_time) * 1000
            
            # Prepare metadata
            metadata = {
                "model": self.settings.openai_model,
                "tokens_used": response.usage.total_tokens if response.usage else None,
                "response_time_ms": round(response_time_ms, 2),
                "finish_reason": response.choices[0].finish_reason
            }
            
            logger.info(f"Generated response in {response_time_ms:.2f}ms using {metadata.get('tokens_used', 'unknown')} tokens")
            
            return {
                "message": ai_message,
                "metadata": metadata
            }
            
        except openai.RateLimitError as e:
            logger.error(f"OpenAI rate limit exceeded: {e}")
            raise Exception("Rate limit exceeded. Please try again later.")
        
        except openai.APITimeoutError as e:
            logger.error(f"OpenAI API timeout: {e}")
            raise Exception("Request timeout. Please try again.")
        
        except openai.APIError as e:
            logger.error(f"OpenAI API error: {e}")
            raise Exception("AI service temporarily unavailable. Please try again.")
        
        except Exception as e:
            logger.error(f"Unexpected error in OpenAI service: {e}")
            raise Exception("An unexpected error occurred. Please try again.")
    
