#!/usr/bin/env python3
"""
Test script for RAG-enabled API endpoints.
Tests the full API with RAG integration.
"""

import requests
import json
import time
from typing import Dict, Any


def test_rag_status(base_url: str) -> bool:
    """Test the RAG status endpoint."""
    print("🔍 Testing RAG status endpoint...")
    
    try:
        response = requests.get(f"{base_url}/api/v1/rag/status", timeout=10)
        
        if response.status_code == 200:
            data = response.json()
            print(f"   ✅ RAG Status: {data['status']}")
            print(f"   📊 Documents: {data.get('total_documents', 'unknown')}")
            print(f"   🤖 Model: {data.get('embedding_model', 'unknown')}")
            return True
        else:
            print(f"   ❌ Status check failed: {response.status_code}")
            return False
            
    except Exception as e:
        print(f"   ❌ Status check error: {e}")
        return False


def test_chat_with_rag(base_url: str) -> bool:
    """Test chat endpoint with RAG-relevant queries."""
    print("\n💬 Testing chat with RAG...")
    
    # Test queries that should trigger RAG
    test_queries = [
        "What are Graph Neural Networks and how do they work?",
        "Can you explain Retrieval-Augmented Generation?",
        "What's the difference between Svelte and React?",
        "How do I build a chatbot with FastAPI?",
        "Tell me about vector databases like FAISS"
    ]
    
    session_id = f"test_session_{int(time.time())}"
    
    for i, query in enumerate(test_queries, 1):
        print(f"\n   Query {i}: {query}")
        
        try:
            payload = {
                "message": query,
                "session_id": session_id
            }
            
            response = requests.post(
                f"{base_url}/api/v1/chat",
                json=payload,
                timeout=30
            )
            
            if response.status_code == 200:
                data = response.json()
                message = data.get('message', '')
                metadata = data.get('metadata', {})
                
                print(f"   ✅ Response length: {len(message)} characters")
                print(f"   🤖 Model: {metadata.get('model', 'unknown')}")
                print(f"   ⏱️  Time: {metadata.get('response_time_ms', 'unknown')}ms")
                print(f"   🎯 Tokens: {metadata.get('tokens_used', 'unknown')}")
                print(f"   📝 Preview: {message[:100]}...")
                
                # Check if response seems to use RAG context
                rag_indicators = ['according to', 'based on', 'from the information', 'the document mentions']
                has_rag_indicators = any(indicator in message.lower() for indicator in rag_indicators)
                
                if has_rag_indicators:
                    print("   🎯 Response appears to use RAG context!")
                
            else:
                print(f"   ❌ Chat failed: {response.status_code}")
                print(f"   📄 Response: {response.text}")
                return False
                
        except Exception as e:
            print(f"   ❌ Chat error: {e}")
            return False
    
    print("\n✅ Chat with RAG test completed!")
    return True


def test_health_check(base_url: str) -> bool:
    """Test health check endpoint."""
    print("\n🏥 Testing health check...")
    
    try:
        response = requests.get(f"{base_url}/api/v1/health", timeout=10)
        
        if response.status_code == 200:
            data = response.json()
            print(f"   ✅ Health: {data['status']}")
            print(f"   🔗 OpenAI: {data.get('openai_status', 'unknown')}")
            return True
        else:
            print(f"   ❌ Health check failed: {response.status_code}")
            return False
            
    except Exception as e:
        print(f"   ❌ Health check error: {e}")
        return False


def main():
    """Main test function."""
    print("🚀 Starting API RAG Tests")
    print("=" * 60)
    
    # Test against local development server
    base_url = "http://localhost:7860"
    
    print(f"🎯 Testing against: {base_url}")
    
    try:
        # Test health first
        if not test_health_check(base_url):
            print("❌ Health check failed - server may not be running")
            return False
        
        # Test RAG status
        if not test_rag_status(base_url):
            print("❌ RAG status check failed")
            return False
        
        # Test chat with RAG
        if not test_chat_with_rag(base_url):
            print("❌ Chat with RAG failed")
            return False
        
        print("\n🎉 All API RAG tests passed!")
        return True
        
    except Exception as e:
        print(f"\n❌ Test suite failed: {e}")
        import traceback
        traceback.print_exc()
        return False


if __name__ == "__main__":
    success = main()
    exit(0 if success else 1)
