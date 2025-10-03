#!/usr/bin/env python3
"""
Test script to verify the @spaces.GPU import fix.
"""

import sys
import os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'app'))

def test_imports():
    """Test that all modules can be imported without errors."""
    print("🧪 Testing imports after @spaces.GPU fix")
    print("=" * 50)
    
    try:
        print("1. Testing main app import...")
        import main
        print("   ✅ main.py imported successfully")
        
        print("2. Testing API routes import...")
        from api import routes
        print("   ✅ api.routes imported successfully")
        
        print("3. Testing OpenAI service import...")
        from services import openai_service
        print("   ✅ services.openai_service imported successfully")
        
        print("4. Testing RAG service import...")
        from services import rag_service
        print("   ✅ services.rag_service imported successfully")
        
        print("5. Testing GPU accelerated functions...")
        from services import gpu_accelerated
        print("   ✅ services.gpu_accelerated imported successfully")
        
        print("\n🎉 All imports successful! The @spaces.GPU fix works.")
        return True
        
    except Exception as e:
        print(f"\n❌ Import failed: {e}")
        import traceback
        traceback.print_exc()
        return False

def test_gpu_detection():
    """Test GPU detection logic."""
    print("\n🖥️  Testing GPU detection")
    print("=" * 50)
    
    try:
        from services.rag_service import FAISS_GPU_AVAILABLE, HF_SPACES_AVAILABLE
        from services.gpu_accelerated import HF_SPACES_AVAILABLE as GPU_HF_AVAILABLE
        
        print(f"FAISS GPU Available: {FAISS_GPU_AVAILABLE}")
        print(f"HF Spaces Available (RAG): {HF_SPACES_AVAILABLE}")
        print(f"HF Spaces Available (GPU): {GPU_HF_AVAILABLE}")
        
        # Check environment variables
        space_id = os.environ.get('SPACE_ID')
        zero_gpu = os.environ.get('SPACES_ZERO_GPU')
        
        print(f"SPACE_ID: {space_id}")
        print(f"SPACES_ZERO_GPU: {zero_gpu}")
        
        if space_id or zero_gpu:
            print("🚀 Running in HF Spaces environment")
        else:
            print("💻 Running in local development environment")
        
        return True
        
    except Exception as e:
        print(f"❌ GPU detection test failed: {e}")
        return False

if __name__ == "__main__":
    print("🚀 Testing @spaces.GPU Import Fix")
    print("=" * 60)
    
    success = True
    
    # Test imports
    if not test_imports():
        success = False
    
    # Test GPU detection
    if not test_gpu_detection():
        success = False
    
    if success:
        print("\n✅ All tests passed! The application should start correctly now.")
    else:
        print("\n❌ Some tests failed. Check the errors above.")
        sys.exit(1)
