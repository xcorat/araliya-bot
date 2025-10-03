"""
Configuration management for Araliya Bot HF Space.
Handles environment variables and application settings.
"""

import os
from functools import lru_cache
from typing import List
from pydantic import Field
from pydantic_settings import BaseSettings

class Settings(BaseSettings):
    """Application settings with environment variable support."""
    
    # Environment
    environment: str = Field(default="production", env="ENVIRONMENT")
    
    # OpenAI Configuration
    openai_api_key: str = Field(..., env="OPENAI_API_KEY")
    openai_model: str = Field(default="gpt-3.5-turbo", env="OPENAI_MODEL")
    openai_max_tokens: int = Field(default=1000, env="OPENAI_MAX_TOKENS")
    openai_temperature: float = Field(default=0.7, env="OPENAI_TEMPERATURE")
    
    # Session Management
    session_timeout_minutes: int = Field(default=60, env="SESSION_TIMEOUT_MINUTES")
    max_conversation_history: int = Field(default=20, env="MAX_CONVERSATION_HISTORY")
    session_cleanup_interval_minutes: int = Field(default=30, env="SESSION_CLEANUP_INTERVAL")
    
    # API Configuration
    max_concurrent_sessions: int = Field(default=10, env="MAX_CONCURRENT_SESSIONS")
    request_timeout_seconds: int = Field(default=30, env="REQUEST_TIMEOUT_SECONDS")
    
    # CORS Configuration
    allowed_origins: List[str] = Field(
        default=["*"], 
        env="ALLOWED_ORIGINS",
        description="Comma-separated list of allowed origins"
    )
    
    # Logging
    log_level: str = Field(default="INFO", env="LOG_LEVEL")
    
    class Config:
        env_file = ".env"
        case_sensitive = False
        extra = "ignore"  # Ignore extra environment variables
        
    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        # Parse comma-separated origins
        if isinstance(self.allowed_origins, str):
            self.allowed_origins = [origin.strip() for origin in self.allowed_origins.split(",")]
    
    def validate_openai_config(self) -> bool:
        """Validate OpenAI configuration."""
        if not self.openai_api_key or self.openai_api_key == "your-api-key-here":
            return False
        return True


@lru_cache()
def get_settings() -> Settings:
    """Get cached application settings."""
    return Settings()
