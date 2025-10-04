"""
RAG Service for Araliya Bot - HF Course FAISS Implementation
Handles document ingestion, vector storage, and retrieval using official HF patterns.
"""

import os
import logging
from typing import List, Dict, Any
import torch

# Import transformers and datasets (HF Course pattern)
try:
    from transformers import AutoTokenizer, AutoModel
    from datasets import Dataset
    TRANSFORMERS_AVAILABLE = True
except ImportError:
    TRANSFORMERS_AVAILABLE = False
    AutoTokenizer = None
    AutoModel = None
    Dataset = None

logger = logging.getLogger(__name__)


class RAGService:
    """
    RAG service following HF LLM Course FAISS pattern.
    """
    
    def __init__(self, model_name: str = "sentence-transformers/multi-qa-mpnet-base-dot-v1"):
        """
        Initialize the RAG service following HF course pattern.
        
        Args:
            model_name: Name of the model to use (HF course default)
        """
        self.model_name = model_name
        
        # Check if transformers is available
        if not TRANSFORMERS_AVAILABLE:
            logger.error("Transformers is not available. RAG functionality will be disabled.")
            logger.error("To fix: pip install transformers datasets")
            self.tokenizer = None
            self.model = None
            self.dataset = None
            return
        
        # Load model and tokenizer (HF course pattern)
        logger.info(f"Loading model and tokenizer: {model_name}")
        self.tokenizer = AutoTokenizer.from_pretrained(model_name)
        self.model = AutoModel.from_pretrained(model_name)
        
        # Move to GPU in main process (following official ZeroGPU pattern)
        self.device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
        self.model.to(self.device)
        logger.info(f"Model moved to device: {self.device}")
        
        # Dataset for FAISS integration (HF course pattern)
        self.dataset = None
    
    def cls_pooling(self, model_output):
        """CLS pooling as shown in HF course."""
        return model_output.last_hidden_state[:, 0]
    
    def get_embeddings(self, text_list):
        """Generate embeddings following HF course pattern."""
        if not TRANSFORMERS_AVAILABLE or self.model is None:
            logger.error("Model not available for embeddings")
            return None
        
        encoded_input = self.tokenizer(
            text_list, padding=True, truncation=True, return_tensors="pt"
        )
        encoded_input = {k: v.to(self.device) for k, v in encoded_input.items()}
        
        with torch.no_grad():
            model_output = self.model(**encoded_input)
        
        return self.cls_pooling(model_output)
    
    def add_documents(self, documents: List[Dict[str, Any]]):
        """
        Add documents using Datasets FAISS integration (HF course pattern).
        
        Args:
            documents: List of document dictionaries with 'content' and metadata
        """
        if not documents:
            return
            
        if not TRANSFORMERS_AVAILABLE:
            logger.warning("Transformers not available - documents not indexed")
            return
        
        logger.info(f"Adding {len(documents)} documents to vector store")
        
        # Create dataset from documents (HF course pattern)
        dataset_dict = {
            "text": [doc.get("content", "") for doc in documents],
            "title": [doc.get("title", "Unknown") for doc in documents],
            "metadata": documents
        }
        
        self.dataset = Dataset.from_dict(dataset_dict)
        
        # Add embeddings using map (HF course pattern)
        logger.info("Generating embeddings using HF course pattern...")
        self.dataset = self.dataset.map(
            lambda x: {
                "embeddings": self.get_embeddings([x["text"]]).detach().cpu().numpy()[0]
            },
            batched=False
        )
        
        # Create FAISS index (HF course pattern)
        self.dataset.add_faiss_index(column="embeddings")
        
        logger.info(f"Successfully added {len(documents)} documents with FAISS index")
    
    def search(self, query: str, k: int = 5) -> List[Dict[str, Any]]:
        """
        Search using Datasets FAISS integration (HF course pattern).
        
        Args:
            query: Search query
            k: Number of results to return
            
        Returns:
            List of relevant documents with scores
        """
        if not TRANSFORMERS_AVAILABLE or self.dataset is None:
            logger.warning("Dataset not available - returning empty results")
            return []
        
        # Generate query embedding (HF course pattern)
        query_embedding = self.get_embeddings([query]).cpu().detach().numpy()
        
        # Search using Datasets FAISS (HF course pattern)
        scores, samples = self.dataset.get_nearest_examples(
            "embeddings", query_embedding, k=k
        )
        
        # Format results
        results = []
        for i, score in enumerate(scores):
            result = samples["metadata"][i].copy()
            result["similarity_score"] = float(score)
            results.append(result)
        
        logger.info(f"Found {len(results)} relevant documents for query: {query[:50]}...")
        return results
    
    def get_context(self, query: str, max_tokens: int = 2000) -> str:
        """
        Get formatted context for RAG generation (HF course pattern).
        
        Args:
            query: User query
            max_tokens: Approximate maximum tokens for context
            
        Returns:
            Formatted context string
        """
        relevant_docs = self.search(query, k=3)
        
        if not relevant_docs:
            return ""
        
        context_parts = []
        current_length = 0
        
        for doc in relevant_docs:
            # Create a formatted context entry
            doc_context = f"Title: {doc.get('title', 'Unknown')}\n"
            doc_context += f"Content: {doc.get('content', '')}\n"
            doc_context += f"Source: {doc.get('author', 'Unknown')} ({doc.get('date', 'Unknown')})\n"
            doc_context += "---\n"
            
            # Rough token estimation (1 token ≈ 4 characters)
            estimated_tokens = len(doc_context) // 4
            
            if current_length + estimated_tokens > max_tokens:
                break
            
            context_parts.append(doc_context)
            current_length += estimated_tokens
        
        context = "\n".join(context_parts)
        logger.info(f"Generated context with ~{current_length} tokens from {len(context_parts)} documents")
        
        return context
    
    def get_stats(self) -> Dict[str, Any]:
        """Get statistics about the RAG service."""
        return {
            "total_documents": len(self.dataset) if self.dataset else 0,
            "model_name": self.model_name,
            "device": str(self.device),
            "transformers_available": TRANSFORMERS_AVAILABLE
        }
    
    def is_initialized(self) -> bool:
        """Check if the RAG service is properly initialized."""
        return (TRANSFORMERS_AVAILABLE and 
                self.model is not None and 
                self.tokenizer is not None and 
                self.dataset is not None and 
                len(self.dataset) > 0)


# Global RAG service instance
rag_service = None


def get_rag_service() -> RAGService:
    """Get the global RAG service instance."""
    global rag_service
    if rag_service is None:
        rag_service = RAGService()
    return rag_service


def initialize_rag_service():
    """Initialize RAG service with sample data (HF course pattern)."""
    global rag_service
    
    # Import sample data
    from data.sample_posts import SAMPLE_BLOG_POSTS
    
    rag_service = RAGService()
    
    # Add sample documents
    if rag_service.dataset is None or len(rag_service.dataset) == 0:
        logger.info("Initializing RAG service with sample data")
        rag_service.add_documents(SAMPLE_BLOG_POSTS)
    else:
        logger.info("RAG service already initialized with existing data")
    
    return rag_service
