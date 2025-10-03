"""
RAG Service for Araliya Bot - Basic FAISS Implementation
Handles document ingestion, vector storage, and retrieval.
"""

import logging
import os
import pickle
from typing import List, Dict, Any, Optional
import numpy as np
import faiss
from sentence_transformers import SentenceTransformer

logger = logging.getLogger(__name__)


class RAGService:
    """
    Basic RAG service using FAISS for vector storage and sentence-transformers for embeddings.
    """
    
    def __init__(self, model_name: str = "all-MiniLM-L6-v2", index_path: str = "faiss_index"):
        """
        Initialize the RAG service.
        
        Args:
            model_name: Name of the sentence-transformers model to use
            index_path: Path to store/load the FAISS index
        """
        self.model_name = model_name
        self.index_path = index_path
        self.metadata_path = f"{index_path}_metadata.pkl"
        
        # Initialize embedding model
        logger.info(f"Loading embedding model: {model_name}")
        self.embedding_model = SentenceTransformer(model_name)
        self.embedding_dim = self.embedding_model.get_sentence_embedding_dimension()
        
        # Initialize FAISS index
        self.index = None
        self.metadata = []  # Store document metadata
        
        # Try to load existing index
        self._load_index()
        
        if self.index is None:
            logger.info("Creating new FAISS index")
            self.index = faiss.IndexFlatIP(self.embedding_dim)  # Inner product for cosine similarity
    
    def _load_index(self) -> bool:
        """Load existing FAISS index and metadata if they exist."""
        try:
            if os.path.exists(f"{self.index_path}.index") and os.path.exists(self.metadata_path):
                logger.info("Loading existing FAISS index")
                self.index = faiss.read_index(f"{self.index_path}.index")
                
                with open(self.metadata_path, 'rb') as f:
                    self.metadata = pickle.load(f)
                
                logger.info(f"Loaded index with {self.index.ntotal} documents")
                return True
        except Exception as e:
            logger.warning(f"Failed to load existing index: {e}")
        
        return False
    
    def _save_index(self):
        """Save FAISS index and metadata to disk."""
        try:
            # Create directory if it doesn't exist
            os.makedirs(os.path.dirname(self.index_path) if os.path.dirname(self.index_path) else ".", exist_ok=True)
            
            faiss.write_index(self.index, f"{self.index_path}.index")
            
            with open(self.metadata_path, 'wb') as f:
                pickle.dump(self.metadata, f)
            
            logger.info(f"Saved index with {self.index.ntotal} documents")
        except Exception as e:
            logger.error(f"Failed to save index: {e}")
    
    def add_documents(self, documents: List[Dict[str, Any]]):
        """
        Add documents to the vector store.
        
        Args:
            documents: List of document dictionaries with 'content' and metadata
        """
        if not documents:
            return
        
        logger.info(f"Adding {len(documents)} documents to vector store")
        
        # Extract content for embedding
        texts = []
        doc_metadata = []
        
        for doc in documents:
            # Combine title and content for better retrieval
            text = f"{doc.get('title', '')} {doc.get('content', '')}"
            texts.append(text)
            doc_metadata.append(doc)
        
        # Generate embeddings
        logger.info("Generating embeddings...")
        embeddings = self.embedding_model.encode(texts, convert_to_numpy=True)
        
        # Normalize embeddings for cosine similarity
        faiss.normalize_L2(embeddings)
        
        # Add to FAISS index
        self.index.add(embeddings)
        self.metadata.extend(doc_metadata)
        
        # Save to disk
        self._save_index()
        
        logger.info(f"Successfully added {len(documents)} documents. Total: {self.index.ntotal}")
    
    def search(self, query: str, k: int = 5) -> List[Dict[str, Any]]:
        """
        Search for relevant documents.
        
        Args:
            query: Search query
            k: Number of results to return
            
        Returns:
            List of relevant documents with scores
        """
        if self.index.ntotal == 0:
            logger.warning("No documents in index")
            return []
        
        # Generate query embedding
        query_embedding = self.embedding_model.encode([query], convert_to_numpy=True)
        faiss.normalize_L2(query_embedding)
        
        # Search
        scores, indices = self.index.search(query_embedding, min(k, self.index.ntotal))
        
        # Format results
        results = []
        for score, idx in zip(scores[0], indices[0]):
            if idx >= 0:  # Valid index
                doc = self.metadata[idx].copy()
                doc['similarity_score'] = float(score)
                results.append(doc)
        
        logger.info(f"Found {len(results)} relevant documents for query: {query[:50]}...")
        return results
    
    def get_context(self, query: str, max_tokens: int = 2000) -> str:
        """
        Get formatted context for RAG generation.
        
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
            "total_documents": self.index.ntotal if self.index else 0,
            "embedding_model": self.model_name,
            "embedding_dimension": self.embedding_dim,
            "index_path": self.index_path
        }


# Global RAG service instance
rag_service = None


def get_rag_service() -> RAGService:
    """Get the global RAG service instance."""
    global rag_service
    if rag_service is None:
        rag_service = RAGService()
    return rag_service


def initialize_rag_service():
    """Initialize RAG service with sample data."""
    global rag_service
    
    # Import sample data
    from data.sample_posts import SAMPLE_BLOG_POSTS
    
    rag_service = RAGService()
    
    # Add sample documents if index is empty
    if rag_service.index.ntotal == 0:
        logger.info("Initializing RAG service with sample data")
        rag_service.add_documents(SAMPLE_BLOG_POSTS)
    else:
        logger.info("RAG service already initialized with existing data")
    
    return rag_service
