"""
Araliya Bot - HF Space FastAPI Application
Main entry point for the Hugging Face Space backend.
"""

import logging
from contextlib import asynccontextmanager
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

# Add the parent directory to Python path for imports
import sys
import os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from config import get_settings
from api.routes import router as api_router
from utils.error_handlers import setup_error_handlers
from services.rag_service import initialize_rag_service

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s"
)
logger = logging.getLogger(__name__)


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Application lifespan manager."""
    logger.info("Starting Araliya Bot HF Space application")
    settings = get_settings()
    logger.info(f"Application configured for environment: {settings.environment}")
    
    # Initialize RAG service with sample data
    try:
        logger.info("Initializing RAG service...")
        initialize_rag_service()
        logger.info("RAG service initialized successfully")
    except Exception as e:
        logger.error(f"Failed to initialize RAG service: {e}")
        # Continue startup even if RAG fails - the app can still work without it
    
    # Startup
    yield
    
    # Shutdown
    logger.info("Shutting down Araliya Bot HF Space application")


# Initialize FastAPI app
app = FastAPI(
    title="Araliya Bot API",
    description="Backend API for Araliya Graph-RAG Chatbot - Phase 1",
    version="1.0.0",
    lifespan=lifespan
)

# Configure CORS
settings = get_settings()
app.add_middleware(
    CORSMiddleware,
    allow_origins=settings.allowed_origins,
    allow_credentials=True,
    allow_methods=["GET", "POST"],
    allow_headers=["*"],
)

# Set up error handlers
setup_error_handlers(app)

# Include API routes
app.include_router(api_router, prefix="/api/v1")


@app.get("/")
async def root():
    """Root endpoint with basic information."""
    return {
        "message": "Araliya Bot API - Phase 1",
        "version": "1.0.0",
        "status": "active",
        "docs": "/docs"
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=7860)
