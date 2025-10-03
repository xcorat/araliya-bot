"""
GPU-accelerated functions for HF Spaces ZeroGPU.
These functions use @spaces.GPU decorator for compute-intensive operations.
"""

import logging
from typing import List, Dict, Any
from openai import OpenAI

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

logger = logging.getLogger(__name__)


@spaces.GPU
def generate_embeddings(embedding_model, texts: List[str]):
    """
    Generate embeddings using GPU acceleration.
    
    Args:
        embedding_model: SentenceTransformer model instance
        texts: List of texts to embed
        
    Returns:
        numpy array of embeddings
    """
    logger.info(f"Generating embeddings for {len(texts)} texts with GPU acceleration")
    embeddings = embedding_model.encode(texts, convert_to_numpy=True)
    return embeddings


@spaces.GPU
def search_embeddings(embedding_model, query: str):
    """
    Generate query embedding using GPU acceleration.
    
    Args:
        embedding_model: SentenceTransformer model instance
        query: Query text
        
    Returns:
        numpy array of query embedding
    """
    logger.info(f"Generating query embedding with GPU acceleration")
    query_embedding = embedding_model.encode([query], convert_to_numpy=True)
    return query_embedding


@spaces.GPU
async def generate_openai_response(client: OpenAI, messages: List[Dict], model: str, max_tokens: int, temperature: float, timeout: int):
    """
    Generate OpenAI response using GPU acceleration.
    
    Args:
        client: OpenAI client instance
        messages: Formatted messages for OpenAI API
        model: Model name
        max_tokens: Maximum tokens
        temperature: Temperature setting
        timeout: Request timeout
        
    Returns:
        OpenAI response object
    """
    logger.info(f"Generating OpenAI response with GPU acceleration")
    response = client.chat.completions.create(
        model=model,
        messages=messages,
        max_tokens=max_tokens,
        temperature=temperature,
        timeout=timeout
    )
    return response
