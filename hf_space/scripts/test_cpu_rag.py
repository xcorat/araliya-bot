#!/usr/bin/env python3
"""
CPU-optimized test script for RAG functionality.
Tests the RAG service with CPU-only configurations.
"""

import sys
import os
import torch
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'app'))

from services.rag_service import RAGService
from data.sample_posts import SAMPLE_BLOG_POSTS


def test_cpu_setup():
    """Test CPU configuration."""
    print("🖥️  Testing CPU Setup")
    print("=" * 50)
    
    # Check PyTorch CPU setup
    print(f"PyTorch version: {torch.__version__}")
    print(f"CUDA available: {torch.cuda.is_available()}")
    print(f"CPU threads: {torch.get_num_threads()}")
    
    # Force CPU usage
    torch.set_num_threads(4)  # Limit CPU threads for stability
    
    return True


def test_rag_service_cpu():
    """Test the RAG service with CPU-only configuration."""
    print("\n🧪 Testing RAG Service (CPU-only)")
    print("=" * 50)
    
    # Initialize RAG service with CPU-friendly settings
    print("1. Initializing RAG service for CPU...")
    rag_service = RAGService(
        model_name="all-MiniLM-L6-v2",  # Lightweight model
        index_path="test_cpu_faiss_index"
    )
    
    # Add sample documents
    print("2. Adding sample documents...")
    rag_service.add_documents(SAMPLE_BLOG_POSTS)
    
    # Get stats
    stats = rag_service.get_stats()
    print(f"   📊 Stats: {stats}")
    
    # Test queries with smaller batch
    test_queries = [
        "What are Graph Neural Networks?",
        "How does RAG work?",
        "Tell me about FastAPI"
    ]
    
    print("\n3. Testing retrieval (CPU-optimized)...")
    for i, query in enumerate(test_queries, 1):
        print(f"\n   Query {i}: {query}")
        
        # Test search with smaller k
        results = rag_service.search(query, k=2)
        print(f"   📄 Found {len(results)} relevant documents:")
        
        for j, doc in enumerate(results):
            print(f"      {j+1}. {doc['title']} (score: {doc['similarity_score']:.3f})")
        
        # Test context generation with smaller token limit
        context = rag_service.get_context(query, max_tokens=300)
        print(f"   📝 Context length: {len(context)} characters")
    
    print("\n✅ CPU RAG service test completed!")
    return True


def test_performance():
    """Test performance metrics."""
    print("\n⚡ Testing Performance")
    print("=" * 50)
    
    import time
    
    rag_service = RAGService(index_path="test_cpu_faiss_index")
    
    # Test search performance
    query = "machine learning and AI"
    
    start_time = time.time()
    results = rag_service.search(query, k=3)
    search_time = time.time() - start_time
    
    print(f"Search time: {search_time:.3f} seconds")
    print(f"Results: {len(results)} documents")
    
    # Test context generation performance
    start_time = time.time()
    context = rag_service.get_context(query, max_tokens=500)
    context_time = time.time() - start_time
    
    print(f"Context generation time: {context_time:.3f} seconds")
    print(f"Context length: {len(context)} characters")
    
    return True


if __name__ == "__main__":
    print("🚀 Starting CPU-Optimized RAG Tests")
    print("=" * 60)
    
    try:
        # Test CPU setup
        test_cpu_setup()
        
        # Test RAG service
        test_rag_service_cpu()
        
        # Test performance
        test_performance()
        
        print("\n🎉 All CPU tests passed!")
        print("\n💡 Tips for HF Spaces deployment:")
        print("   • @spaces.GPU decorator will provide GPU acceleration")
        print("   • CPU fallback ensures compatibility")
        print("   • Lightweight model reduces memory usage")
        
    except Exception as e:
        print(f"\n❌ Test failed: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)
