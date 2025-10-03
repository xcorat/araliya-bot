#!/usr/bin/env python3
"""
Test script for RAG functionality.
Tests the RAG service independently of the full API.
"""

import sys
import os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'app'))

from services.rag_service import RAGService
from data.sample_posts import SAMPLE_BLOG_POSTS


def test_rag_service():
    """Test the RAG service functionality."""
    print("🧪 Testing RAG Service")
    print("=" * 50)
    
    # Initialize RAG service
    print("1. Initializing RAG service...")
    rag_service = RAGService(index_path="test_faiss_index")
    
    # Add sample documents
    print("2. Adding sample documents...")
    rag_service.add_documents(SAMPLE_BLOG_POSTS)
    
    # Get stats
    stats = rag_service.get_stats()
    print(f"   📊 Stats: {stats}")
    
    # Test queries
    test_queries = [
        "What are Graph Neural Networks?",
        "How does RAG work?",
        "Tell me about vector databases",
        "What is FastAPI good for?",
        "Svelte vs React comparison"
    ]
    
    print("\n3. Testing retrieval...")
    for i, query in enumerate(test_queries, 1):
        print(f"\n   Query {i}: {query}")
        
        # Test search
        results = rag_service.search(query, k=2)
        print(f"   📄 Found {len(results)} relevant documents:")
        
        for j, doc in enumerate(results):
            print(f"      {j+1}. {doc['title']} (score: {doc['similarity_score']:.3f})")
        
        # Test context generation
        context = rag_service.get_context(query, max_tokens=500)
        print(f"   📝 Context length: {len(context)} characters")
        print(f"   📝 Context preview: {context[:100]}...")
    
    print("\n✅ RAG service test completed!")
    return True


def test_integration():
    """Test integration with the full service stack."""
    print("\n🔗 Testing Integration")
    print("=" * 50)
    
    try:
        from services.rag_service import initialize_rag_service, get_rag_service
        
        print("1. Initializing RAG service via integration...")
        initialize_rag_service()
        
        print("2. Getting service instance...")
        rag_service = get_rag_service()
        
        stats = rag_service.get_stats()
        print(f"   📊 Integration stats: {stats}")
        
        print("3. Testing context retrieval...")
        context = rag_service.get_context("What is machine learning?")
        print(f"   📝 Context generated: {len(context)} characters")
        
        print("\n✅ Integration test completed!")
        return True
        
    except Exception as e:
        print(f"❌ Integration test failed: {e}")
        return False


if __name__ == "__main__":
    print("🚀 Starting RAG Tests")
    print("=" * 60)
    
    try:
        # Test RAG service
        test_rag_service()
        
        # Test integration
        test_integration()
        
        print("\n🎉 All tests passed!")
        
    except Exception as e:
        print(f"\n❌ Test failed: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)
